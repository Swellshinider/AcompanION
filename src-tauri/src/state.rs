//! Per-character state kept by the live engine, bound to the connection that reported it.

use std::collections::{BTreeMap, HashMap};
use std::fs::{File, OpenOptions, TryLockError};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::decode::Decoded;
use crate::events::{CounterRecord, Event};

/// `(server, name)`: a character's identity on the wire.
pub type Key = (u16, String);

/// One checklist counter as last reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Counter {
    /// Remaining amount: a count, or milliseconds when `ms`.
    pub value: u64,
    pub ms: bool,
    pub updated_us: u64,
}

/// A "gained today" amount (Supply Emergency deliveries, kinah) as last reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gain {
    pub value: u64,
    pub updated_us: u64,
    /// The board group the value counts (Supply Emergency only), so live deliveries can be matched.
    pub group: Option<u32>,
}

/// One pet as last reported: `souls` count toward the next level, not from the start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pet {
    pub level: u32,
    pub souls: u32,
}

/// The pet collection as last reported by a character's session, plus the pet it has out.
/// The collection is account-wide on the wire; only `active` differs per character.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pets {
    pub active: u32,
    pub list: BTreeMap<u32, Pet>,
    pub updated_us: u64,
}

/// `Character.gains` keys.
const SUPPLY_DAILY: &str = "supply_daily";
const SUPPLY_WEEKLY: &str = "supply_weekly";
const KINAH_TODAY: &str = "kinah_today";
const DUTY_DAILY: &str = "duty_daily";

#[derive(Clone, Debug)]
pub struct Character {
    pub server: u16,
    pub name: String,
    pub class_code: u32,
    pub level: u32,
    pub combat_power: Option<u32>,
    pub item_level: Option<u32>,
    pub counters: BTreeMap<u32, Counter>,
    pub gains: BTreeMap<String, Gain>,
    pub pets: Option<Pets>,
    pub last_seen_us: u64,
}

/// Most events held per connection while it waits for its identity.
const PENDING_MAX: usize = 256;

/// What one live connection (client TCP port) is doing.
#[derive(Default)]
struct Conn {
    /// The character this connection plays, once its identity has arrived.
    session: Option<Key>,
    /// Events that precede their identity on the wire: `(ts_us, event)`, in arrival order.
    pending: Vec<(u64, Event)>,
}

/// Characters seen so far, plus which one each live connection is playing.
#[derive(Default)]
pub struct State {
    chars: BTreeMap<Key, Character>,
    conns: HashMap<u16, Conn>,
    /// Held events discarded because a connection exceeded `PENDING_MAX` without an identity.
    pub dropped: u64,
}

impl Character {
    fn new(key: &Key) -> Self {
        Character {
            server: key.0,
            name: key.1.clone(),
            class_code: 0,
            level: 0,
            combat_power: None,
            item_level: None,
            counters: BTreeMap::new(),
            gains: BTreeMap::new(),
            pets: None,
            last_seen_us: 0,
        }
    }

    /// Apply a power, item level, counter, gain or pet event that carries its own time.
    fn record(&mut self, ts_us: u64, event: &Event) {
        let mut put = |r: &CounterRecord| {
            self.counters.insert(r.id, Counter { value: r.value, ms: r.ms, updated_us: ts_us });
        };
        match event {
            Event::CombatPower(p) => self.combat_power = Some(*p),
            Event::ItemLevel(v) => self.item_level = Some(*v),
            Event::Counters(rs) => rs.iter().for_each(&mut put),
            Event::Counter(r) => put(r),
            Event::Boards(boards) => {
                for b in boards {
                    let key = match b.slot {
                        1 => SUPPLY_DAILY,
                        2 => SUPPLY_WEEKLY,
                        _ => continue,
                    };
                    self.gains.insert(
                        key.into(),
                        Gain { value: u64::from(b.done), updated_us: ts_us, group: Some(b.group) },
                    );
                }
            }
            Event::BoardEntry { group, done: true, .. } => {
                // ponytail: a delivery that crosses the 07:00 reset before the next world entry
                // carries a new group and matches nothing; it shows up with the next `56 e2`.
                // Deliveries are counted per frame, not per entry: a resent `3e e2` for an entry
                // that is already done would count twice until the next `56 e2` corrects it.
                for g in self.gains.values_mut().filter(|g| g.group == Some(*group)) {
                    g.value += 1;
                    g.updated_us = ts_us;
                }
            }
            Event::KinahToday(v) => {
                self.gains.insert(KINAH_TODAY.into(), Gain { value: *v, updated_us: ts_us, group: None });
            }
            Event::DutyMissions(v) => {
                self.gains.insert(DUTY_DAILY.into(), Gain { value: u64::from(*v), updated_us: ts_us, group: None });
            }
            Event::PassUsed(entries) => {
                // the key is the counter id; the frontend turns `used` into what is left
                for &(id, used) in entries {
                    self.gains
                        .insert(id.to_string(), Gain { value: u64::from(used), updated_us: ts_us, group: None });
                }
            }
            Event::Pets { active, levels, souls } => {
                let mut list: BTreeMap<u32, Pet> = BTreeMap::new();
                for &(id, level) in levels {
                    list.insert(id, Pet { level, souls: 0 });
                }
                for &(id, s) in souls {
                    list.entry(id).or_insert(Pet { level: 0, souls: 0 }).souls = s;
                }
                self.pets = Some(Pets { active: *active, list, updated_us: ts_us });
            }
            // ponytail: a delta that crosses a threshold is followed by a `PetLevel`, which zeroes the
            // souls; the overflow past the threshold is not modelled. Deltas before a snapshot are lost.
            Event::PetSoulsDelta(entries) => {
                if let Some(p) = &mut self.pets {
                    for &(id, n) in entries {
                        p.list.entry(id).or_insert(Pet { level: 0, souls: 0 }).souls += n;
                    }
                    p.updated_us = ts_us;
                }
            }
            Event::PetLevel { id, level } => {
                if let Some(p) = &mut self.pets {
                    p.list.insert(*id, Pet { level: *level, souls: 0 });
                    p.updated_us = ts_us;
                }
            }
            _ => {}
        }
        self.last_seen_us = self.last_seen_us.max(ts_us);
    }
}

impl State {
    /// Fold one decoded event in; returns the character it changed, `None` when it is held.
    ///
    /// The identity frame trails the world-entry bundle on the wire (`CombatPower`, `0b 61`
    /// snapshot, `CombatPower`s, then `Identity`), so power and snapshots are held per connection
    /// and attributed to the identity that follows (the boards, the kinah and Duty Missions snapshots and the Daeva Pass
    /// counters arrive in the same bundle). A counter snapshot also unbinds the connection: a new character is on its way.
    /// ponytail: latency ceiling = the next identity; power and snapshots are shown only once it
    /// arrives, and a connection whose identity never comes holds at most `PENDING_MAX` events.
    /// The item level is not held: unbound, it is dropped, because the identity carries the same
    /// value (and is applied last, so it wins). The pet snapshot trails the counter snapshot like the boards,
    /// so it is held too; `PetSoulsTotal` is ignored (the delta and level frames carry the same change).
    pub fn apply(&mut self, d: &Decoded) -> Option<Key> {
        let conn = self.conns.entry(d.conn).or_default();
        match &d.event {
            Event::Identity { name, server, class_code, level, item_level, .. } => {
                let key = (*server, name.clone());
                conn.session = Some(key.clone());
                let pending = std::mem::take(&mut conn.pending);
                let c = self.chars.entry(key.clone()).or_insert_with(|| Character::new(&key));
                c.class_code = *class_code;
                c.level = *level;
                c.last_seen_us = c.last_seen_us.max(d.ts_us);
                for (ts_us, event) in &pending {
                    c.record(*ts_us, event);
                }
                c.item_level = Some(*item_level);
                Some(key)
            }
            Event::Counter(_)
            | Event::ItemLevel(_)
            | Event::Boards(_)
            | Event::BoardEntry { .. }
            | Event::KinahToday(_)
            | Event::DutyMissions(_)
            | Event::PassUsed(_)
            | Event::Pets { .. }
            | Event::PetSoulsDelta(_)
            | Event::PetLevel { .. }
                if conn.session.is_some() =>
            {
                let key = conn.session.clone()?;
                self.chars.get_mut(&key)?.record(d.ts_us, &d.event);
                Some(key)
            }
            Event::CombatPower(_)
            | Event::Counters(_)
            | Event::Counter(_)
            | Event::Boards(_)
            | Event::BoardEntry { .. }
            | Event::KinahToday(_)
            | Event::DutyMissions(_)
            | Event::PassUsed(_)
            | Event::Pets { .. }
            | Event::PetSoulsDelta(_)
            | Event::PetLevel { .. } => {
                if matches!(d.event, Event::Counters(_)) {
                    conn.session = None;
                }
                conn.pending.push((d.ts_us, d.event.clone()));
                if conn.pending.len() > PENDING_MAX {
                    conn.pending.remove(0);
                    self.dropped += 1;
                }
                None
            }
            // the radar's events (positions) are transient: see `radar.rs`, nothing is saved
            Event::ItemLevel(_)
            | Event::PetSoulsTotal(_)
            | Event::Spawn { .. }
            | Event::Summon { .. }
            | Event::PlayerSpawn { .. }
            | Event::Pos { .. }
            | Event::Cast { .. }
            | Event::Gone { .. }
            | Event::Teleport { .. }
            | Event::MapEnter { .. }
            | Event::Hit { .. }
            | Event::Heal { .. }
            | Event::Hp { .. }
            | Event::Stagger { .. } => None,
        }
    }

    /// The connection closed or its port was reused: forget its character and held events.
    pub fn end_session(&mut self, conn: u16) {
        self.conns.remove(&conn);
    }

    pub fn get(&self, key: &Key) -> Option<&Character> {
        self.chars.get(key)
    }

    /// Forget a character. A connection still bound to it re-creates it only with its next identity.
    pub fn remove(&mut self, key: &Key) -> bool {
        self.chars.remove(key).is_some()
    }
}

/// On-disk format version.
const VERSION: u64 = 1;

fn field<'a>(v: &'a Value, key: &str) -> Result<&'a Value> {
    v.get(key).with_context(|| format!("missing field {key:?}"))
}

fn num<T: TryFrom<u64>>(v: &Value, key: &str) -> Result<T> {
    let n = field(v, key)?.as_u64().with_context(|| format!("field {key:?} is not an unsigned integer"))?;
    T::try_from(n).ok().with_context(|| format!("field {key:?} is out of range"))
}

impl Character {
    /// Counters are an array of objects, so ids never become string keys.
    pub fn to_json(&self) -> Value {
        let counters: Vec<Value> = self
            .counters
            .iter()
            .map(|(id, c)| json!({"id": id, "value": c.value, "ms": c.ms, "updated_us": c.updated_us}))
            .collect();
        let gains: serde_json::Map<String, Value> = self
            .gains
            .iter()
            .map(|(k, g)| {
                let mut v = json!({"value": g.value, "updated_us": g.updated_us});
                if let Some(group) = g.group {
                    v["group"] = group.into();
                }
                (k.clone(), v)
            })
            .collect();
        // an array, so pet ids never become string keys
        let pets = self.pets.as_ref().map(|p| {
            let list: Vec<Value> =
                p.list.iter().map(|(id, pet)| json!({"id": id, "level": pet.level, "souls": pet.souls})).collect();
            json!({"active": p.active, "updated_us": p.updated_us, "list": list})
        });
        json!({
            "server": self.server, "name": self.name, "class_code": self.class_code, "level": self.level,
            "combat_power": self.combat_power, "item_level": self.item_level, "counters": counters,
            "gains": gains, "pets": pets, "last_seen_us": self.last_seen_us,
        })
    }

    fn from_json(v: &Value) -> Result<Character> {
        let combat_power = match field(v, "combat_power")? {
            Value::Null => None,
            p => Some(p.as_u64().and_then(|n| u32::try_from(n).ok()).context("combat_power is not a u32")?),
        };
        // absent in files written before item levels were decoded
        let item_level = match v.get("item_level") {
            None | Some(Value::Null) => None,
            Some(n) => Some(n.as_u64().and_then(|n| u32::try_from(n).ok()).context("item_level is not a u32")?),
        };
        // absent in files written before gains were decoded
        let gains = match v.get("gains") {
            None => BTreeMap::new(),
            Some(g) => g
                .as_object()
                .context("gains is not an object")?
                .iter()
                .map(|(k, g)| {
                    let group = match g.get("group") {
                        None | Some(Value::Null) => None,
                        Some(_) => Some(num(g, "group")?),
                    };
                    Ok((k.clone(), Gain { value: num(g, "value")?, updated_us: num(g, "updated_us")?, group }))
                })
                .collect::<Result<_>>()?,
        };
        // absent in files written before pets were stored
        let pets = match v.get("pets") {
            None | Some(Value::Null) => None,
            Some(p) => {
                let list = field(p, "list")?
                    .as_array()
                    .context("pets list is not an array")?
                    .iter()
                    .map(|e| Ok((num(e, "id")?, Pet { level: num(e, "level")?, souls: num(e, "souls")? })))
                    .collect::<Result<_>>()?;
                Some(Pets { active: num(p, "active")?, list, updated_us: num(p, "updated_us")? })
            }
        };
        let counters = field(v, "counters")?
            .as_array()
            .context("counters is not an array")?
            .iter()
            .map(|c| {
                let ms = field(c, "ms")?.as_bool().context("ms is not a bool")?;
                Ok((num(c, "id")?, Counter { value: num(c, "value")?, ms, updated_us: num(c, "updated_us")? }))
            })
            .collect::<Result<_>>()?;
        Ok(Character {
            server: num(v, "server")?,
            name: field(v, "name")?.as_str().context("name is not a string")?.to_string(),
            class_code: num(v, "class_code")?,
            level: num(v, "level")?,
            combat_power,
            item_level,
            counters,
            gains,
            pets,
            last_seen_us: num(v, "last_seen_us")?,
        })
    }
}

impl State {
    /// `{"version":1,"characters":[...]}`; sessions are not persisted.
    pub fn to_json(&self) -> Value {
        json!({"version": VERSION, "characters": self.chars.values().map(Character::to_json).collect::<Vec<_>>()})
    }

    pub fn from_json(v: &Value) -> Result<State> {
        let version = field(v, "version")?.as_u64();
        if version != Some(VERSION) {
            bail!("unsupported state version {:?}", field(v, "version")?);
        }
        let mut state = State::default();
        for c in field(v, "characters")?.as_array().context("characters is not an array")? {
            let c = Character::from_json(c)?;
            state.chars.insert((c.server, c.name.clone()), c);
        }
        Ok(state)
    }

    /// A missing file is an empty state; a corrupt one is an error and is left untouched.
    pub fn load(path: &Path) -> Result<State> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(State::default()),
            Err(e) => return Err(e).with_context(|| format!("cannot read {}", path.display())),
        };
        let v: Value = serde_json::from_str(&text).with_context(|| format!("{} is not valid JSON", path.display()))?;
        State::from_json(&v).with_context(|| format!("{} is not a state file", path.display()))
    }

    /// Write `<path>.tmp`, sync it to disk, then rename it over `path`, so a kill or a power loss
    /// never leaves a half-written file.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        let text = serde_json::to_string_pretty(&self.to_json())?;
        let mut file = File::create(&tmp).with_context(|| format!("cannot create {}", tmp.display()))?;
        file.write_all(text.as_bytes())
            .and_then(|()| file.sync_all())
            .with_context(|| format!("cannot write {}", tmp.display()))?;
        drop(file);
        std::fs::rename(&tmp, path).with_context(|| format!("cannot replace {}", path.display()))
    }
}

/// Hold this handle from before loading writable state through its final save.
/// `None` means another writer owns the file; leave `.lock` in place so all writers lock the same file.
pub fn try_lock(path: &Path) -> Result<Option<File>> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let mut lock = path.as_os_str().to_owned();
    lock.push(".lock");
    let lock = PathBuf::from(lock);
    let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&lock)
        .with_context(|| format!("cannot open {}", lock.display()))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(e)) => Err(e).with_context(|| format!("cannot lock {}", lock.display())),
    }
}

/// `%APPDATA%\AcompanION\characters.json`.
pub fn default_path() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join("AcompanION").join("characters.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::Decoded;
    use crate::events::{CounterRecord, Event};

    fn ident(conn: u16, ts_us: u64, name: &str, server: u16, level: u32) -> Decoded {
        Decoded { ts_us, conn, event: Event::Identity { name: name.into(), server, class_code: 3, level, item_level: 500, entity: 1 } }
    }

    fn rec(id: u32, value: u64, ms: bool) -> CounterRecord {
        CounterRecord { id, value, ms }
    }

    fn counter(conn: u16, ts_us: u64, id: u32, value: u64) -> Decoded {
        Decoded { ts_us, conn, event: Event::Counter(rec(id, value, false)) }
    }

    fn power(conn: u16, ts_us: u64, value: u32) -> Decoded {
        Decoded { ts_us, conn, event: Event::CombatPower(value) }
    }

    fn snapshot(conn: u16, ts_us: u64, records: Vec<CounterRecord>) -> Decoded {
        Decoded { ts_us, conn, event: Event::Counters(records) }
    }

    fn key(server: u16, name: &str) -> Key {
        (server, name.to_string())
    }

    /// The wire order at world entry: the identity trails the bundle it claims.
    fn bundle(s: &mut State, conn: u16, t: u64, name: &str, power_value: u32, records: Vec<CounterRecord>) {
        for d in [
            power(conn, t, power_value),
            snapshot(conn, t + 1, records),
            power(conn, t + 2, power_value),
            power(conn, t + 3, power_value + 1),
            ident(conn, t + 4, name, 7, 40),
        ] {
            s.apply(&d);
        }
    }

    #[test]
    fn identity_creates_and_binds_the_session() {
        let mut s = State::default();
        assert_eq!(s.apply(&ident(5000, 10, "Alpha", 7, 40)), Some(key(7, "Alpha")));
        let c = s.get(&key(7, "Alpha")).unwrap();
        assert_eq!((c.server, c.name.as_str(), c.class_code, c.level, c.last_seen_us), (7, "Alpha", 3, 40, 10));
        assert!(c.combat_power.is_none() && c.counters.is_empty());
        assert_eq!(s.apply(&counter(5000, 11, 1, 2)), Some(key(7, "Alpha")));
    }

    #[test]
    fn the_bundle_that_precedes_an_identity_is_attributed_to_it() {
        let mut s = State::default();
        assert_eq!(s.apply(&power(5000, 1, 100)), None);
        assert_eq!(s.apply(&snapshot(5000, 2, vec![rec(1, 3, false), rec(2, 90_000, true)])), None);
        assert_eq!(s.apply(&power(5000, 3, 100)), None);
        assert_eq!(s.apply(&power(5000, 4, 150)), None);
        assert!(s.get(&key(7, "Alpha")).is_none());
        assert_eq!(s.apply(&ident(5000, 5, "Alpha", 7, 40)), Some(key(7, "Alpha")));
        let c = s.get(&key(7, "Alpha")).unwrap();
        assert_eq!(c.combat_power, Some(150));
        assert_eq!(c.counters[&1], Counter { value: 3, ms: false, updated_us: 2 });
        assert_eq!(c.counters[&2], Counter { value: 90_000, ms: true, updated_us: 2 });
        assert_eq!((c.level, c.last_seen_us), (40, 5));
        assert_eq!(s.dropped, 0);
    }

    #[test]
    fn a_second_bundle_goes_to_the_identity_that_trails_it() {
        let mut s = State::default();
        bundle(&mut s, 5000, 10, "Alpha", 100, vec![rec(1, 5, false)]);
        bundle(&mut s, 5000, 20, "Beta", 200, vec![rec(1, 9, false)]);
        let a = s.get(&key(7, "Alpha")).unwrap();
        assert_eq!((a.counters[&1].value, a.counters.len(), a.combat_power), (5, 1, Some(101)));
        let b = s.get(&key(7, "Beta")).unwrap();
        assert_eq!((b.counters[&1].value, b.counters.len(), b.combat_power), (9, 1, Some(201)));
        assert_eq!(s.dropped, 0);
    }

    #[test]
    fn a_counter_update_applies_at_once_when_bound_and_waits_when_not() {
        let mut s = State::default();
        assert_eq!(s.apply(&counter(5000, 1, 1, 4)), None); // unbound: held
        assert!(s.get(&key(7, "Alpha")).is_none());
        s.apply(&ident(5000, 2, "Alpha", 7, 40));
        assert_eq!(s.get(&key(7, "Alpha")).unwrap().counters[&1], Counter { value: 4, ms: false, updated_us: 1 });

        assert_eq!(s.apply(&counter(5000, 3, 1, 2)), Some(key(7, "Alpha")));
        assert_eq!(s.get(&key(7, "Alpha")).unwrap().counters[&1], Counter { value: 2, ms: false, updated_us: 3 });

        // a snapshot unbinds: later updates wait for the identity that follows
        s.apply(&snapshot(5000, 4, vec![rec(1, 7, false)]));
        assert_eq!(s.apply(&counter(5000, 5, 1, 6)), None);
        assert_eq!(s.get(&key(7, "Alpha")).unwrap().counters[&1].value, 2);
        s.apply(&ident(5000, 6, "Alpha", 7, 40));
        assert_eq!(s.get(&key(7, "Alpha")).unwrap().counters[&1], Counter { value: 6, ms: false, updated_us: 5 });
    }

    #[test]
    fn two_connections_stay_independent_while_interleaved() {
        let mut s = State::default();
        s.apply(&power(5000, 1, 100));
        s.apply(&power(5001, 2, 200));
        s.apply(&snapshot(5001, 3, vec![rec(1, 9, false)]));
        s.apply(&snapshot(5000, 4, vec![rec(1, 4, false)]));
        s.apply(&ident(5001, 5, "Beta", 7, 20));
        s.apply(&ident(5000, 6, "Alpha", 7, 40));
        let a = s.get(&key(7, "Alpha")).unwrap();
        assert_eq!((a.counters[&1].value, a.combat_power), (4, Some(100)));
        let b = s.get(&key(7, "Beta")).unwrap();
        assert_eq!((b.counters[&1].value, b.combat_power), (9, Some(200)));
    }

    #[test]
    fn a_swap_on_one_connection_rebinds_and_keeps_the_old_values() {
        let mut s = State::default();
        s.apply(&ident(5000, 1, "Alpha", 7, 40));
        s.apply(&counter(5000, 2, 1, 5));
        s.apply(&ident(5000, 3, "Beta", 7, 20));
        s.apply(&counter(5000, 4, 1, 1));
        assert_eq!(s.get(&key(7, "Alpha")).unwrap().counters[&1].value, 5);
        assert_eq!(s.get(&key(7, "Beta")).unwrap().counters[&1].value, 1);
    }

    #[test]
    fn end_session_unbinds_and_clears_what_was_held() {
        let mut s = State::default();
        s.apply(&ident(5000, 1, "Alpha", 7, 40));
        s.end_session(5000);
        assert_eq!(s.apply(&counter(5000, 2, 1, 5)), None); // unbound again: held
        s.end_session(5000);
        s.apply(&ident(5000, 3, "Alpha", 7, 40));
        assert!(s.get(&key(7, "Alpha")).unwrap().counters.is_empty());
        assert_eq!(s.dropped, 0);
    }

    #[test]
    fn held_events_overflow_drops_the_oldest_and_counts_it() {
        let mut s = State::default();
        for id in 0..=PENDING_MAX as u32 {
            s.apply(&counter(5000, u64::from(id), id, 1));
        }
        assert_eq!(s.dropped, 1);
        s.apply(&ident(5000, 1000, "Alpha", 7, 40));
        let c = s.get(&key(7, "Alpha")).unwrap();
        assert_eq!(c.counters.len(), PENDING_MAX);
        assert!(!c.counters.contains_key(&0) && c.counters.contains_key(&1));
    }

    #[test]
    fn item_level_comes_from_the_identity_and_live_updates() {
        let mut s = State::default();
        let item = |ts_us, v| Decoded { ts_us, conn: 5000, event: Event::ItemLevel(v) };
        assert_eq!(s.apply(&item(1, 480)), None); // unbound: dropped
        assert_eq!(s.apply(&ident(5000, 2, "Alpha", 7, 40)), Some(key(7, "Alpha")));
        assert_eq!(s.get(&key(7, "Alpha")).unwrap().item_level, Some(500));
        assert_eq!(s.apply(&item(3, 512)), Some(key(7, "Alpha")));
        let c = s.get(&key(7, "Alpha")).unwrap();
        assert_eq!((c.item_level, c.last_seen_us), (Some(512), 3));
    }

    #[test]
    fn a_state_file_without_item_level_loads_as_none() {
        let mut json = sample().to_json();
        json["characters"][0].as_object_mut().unwrap().remove("item_level");
        let s = State::from_json(&json).unwrap();
        assert_eq!(s.get(&key(7, "Alpha")).unwrap().item_level, None);
        json["characters"][0]["item_level"] = "x".into();
        assert!(State::from_json(&json).is_err());
    }

    fn ev(conn: u16, ts_us: u64, event: Event) -> Decoded {
        Decoded { ts_us, conn, event }
    }

    fn boards() -> Event {
        use crate::events::Board;
        Event::Boards(vec![
            Board { slot: 1, group: 10, done: 1 },
            Board { slot: 2, group: 20, done: 4 },
            Board { slot: 3, group: 30, done: 9 },
        ])
    }

    fn entry(group: u32, done: bool) -> Event {
        Event::BoardEntry { group, id: 1, value: 0, done }
    }

    #[test]
    fn duty_missions_are_held_until_the_identity_then_follow_live_updates() {
        let mut s = State::default();
        assert_eq!(s.apply(&snapshot(5000, 1, vec![rec(1, 3, false)])), None);
        assert_eq!(s.apply(&ev(5000, 2, Event::DutyMissions(0))), None);
        assert_eq!(s.apply(&ident(5000, 3, "Alpha", 7, 40)), Some(key(7, "Alpha")));
        let g = &s.get(&key(7, "Alpha")).unwrap().gains;
        assert_eq!(g["duty_daily"], Gain { value: 0, updated_us: 2, group: None });
        assert_eq!(s.apply(&ev(5000, 4, Event::DutyMissions(2))), Some(key(7, "Alpha")));
        let g = &s.get(&key(7, "Alpha")).unwrap().gains;
        assert_eq!(g["duty_daily"], Gain { value: 2, updated_us: 4, group: None });
    }

    #[test]
    fn gains_are_held_until_the_identity_then_replayed() {
        let mut s = State::default();
        assert_eq!(s.apply(&snapshot(5000, 1, vec![rec(1, 3, false)])), None);
        assert_eq!(s.apply(&ev(5000, 2, boards())), None);
        assert_eq!(s.apply(&ev(5000, 3, Event::KinahToday(500))), None);
        assert!(s.get(&key(7, "Alpha")).is_none());
        assert_eq!(s.apply(&ident(5000, 4, "Alpha", 7, 40)), Some(key(7, "Alpha")));
        let g = &s.get(&key(7, "Alpha")).unwrap().gains;
        assert_eq!(g["supply_daily"], Gain { value: 1, updated_us: 2, group: Some(10) });
        assert_eq!(g["supply_weekly"], Gain { value: 4, updated_us: 2, group: Some(20) });
        assert_eq!(g["kinah_today"], Gain { value: 500, updated_us: 3, group: None });
        assert_eq!(g.len(), 3); // slot 3 is ignored
    }

    #[test]
    fn a_delivery_increments_only_the_matching_group() {
        let mut s = State::default();
        s.apply(&ev(5000, 1, boards()));
        s.apply(&ident(5000, 2, "Alpha", 7, 40));
        assert_eq!(s.apply(&ev(5000, 3, entry(20, true))), Some(key(7, "Alpha")));
        let c = s.get(&key(7, "Alpha")).unwrap();
        assert_eq!((c.gains["supply_daily"].value, c.gains["supply_weekly"].value), (1, 5));
        assert_eq!((c.gains["supply_daily"].updated_us, c.gains["supply_weekly"].updated_us), (1, 3));
        assert_eq!(c.last_seen_us, 3);
        s.apply(&ev(5000, 4, entry(10, false))); // not done: ignored
        s.apply(&ev(5000, 5, entry(99, true))); // unknown group: ignored
        let c = s.get(&key(7, "Alpha")).unwrap();
        assert_eq!((c.gains["supply_daily"].value, c.gains["supply_weekly"].value), (1, 5));
    }

    #[test]
    fn boards_resent_while_bound_replace_the_gains_of_the_bound_character() {
        let mut s = State::default();
        s.apply(&ev(5000, 1, boards()));
        s.apply(&ident(5000, 2, "Alpha", 7, 40));
        let resent = Event::Boards(vec![crate::events::Board { slot: 1, group: 11, done: 0 }]);
        assert_eq!(s.apply(&ev(5000, 3, resent)), Some(key(7, "Alpha")));
        let g = &s.get(&key(7, "Alpha")).unwrap().gains;
        assert_eq!(g["supply_daily"], Gain { value: 0, updated_us: 3, group: Some(11) });
        assert_eq!(g["supply_weekly"].value, 4); // not in the resent frame: kept
    }

    #[test]
    fn a_kinah_update_applies_when_bound_and_waits_when_not() {
        let mut s = State::default();
        s.apply(&ident(5000, 1, "Alpha", 7, 40));
        assert_eq!(s.apply(&ev(5000, 2, Event::KinahToday(700))), Some(key(7, "Alpha")));
        assert_eq!(s.get(&key(7, "Alpha")).unwrap().gains["kinah_today"].value, 700);
        s.apply(&snapshot(5000, 3, vec![rec(1, 3, false)])); // unbinds
        assert_eq!(s.apply(&ev(5000, 4, entry(10, true))), None);
        assert_eq!(s.apply(&ev(5000, 5, Event::KinahToday(900))), None);
        assert_eq!(s.get(&key(7, "Alpha")).unwrap().gains["kinah_today"].value, 700);
        s.apply(&ident(5000, 6, "Alpha", 7, 40));
        assert_eq!(s.get(&key(7, "Alpha")).unwrap().gains["kinah_today"].value, 900);
    }

    #[test]
    fn the_pass_is_held_until_the_identity_and_a_live_update_replaces_one_value() {
        let mut s = State::default();
        let pass = |ts, v: Vec<(u32, u32)>| ev(5000, ts, Event::PassUsed(v));
        assert_eq!(s.apply(&snapshot(5000, 1, vec![rec(1, 3, false)])), None);
        assert_eq!(s.apply(&pass(2, vec![(10044, 12_000), (10045, 300)])), None);
        assert!(s.get(&key(7, "Alpha")).is_none());
        assert_eq!(s.apply(&ident(5000, 3, "Alpha", 7, 40)), Some(key(7, "Alpha")));
        let g = &s.get(&key(7, "Alpha")).unwrap().gains;
        assert_eq!(g["10044"], Gain { value: 12_000, updated_us: 2, group: None });
        assert_eq!(g["10045"], Gain { value: 300, updated_us: 2, group: None });

        assert_eq!(s.apply(&pass(4, vec![(10045, 450)])), Some(key(7, "Alpha")));
        let c = s.get(&key(7, "Alpha")).unwrap();
        assert_eq!((c.gains["10044"].value, c.gains["10044"].updated_us), (12_000, 2));
        assert_eq!((c.gains["10045"].value, c.gains["10045"].updated_us), (450, 4));
        // they survive a save as plain gain keys
        let json = s.to_json();
        assert_eq!(json["characters"][0]["gains"]["10044"], json!({"value": 12_000, "updated_us": 2}));
        assert_eq!(State::from_json(&json).unwrap().get(&key(7, "Alpha")).unwrap().gains, c.gains);
    }

    #[test]
    fn gains_survive_a_save_and_an_old_file_without_them_loads_empty() {
        let mut s = State::default();
        s.apply(&ev(5000, 1, boards()));
        s.apply(&ev(5000, 2, Event::KinahToday(500)));
        s.apply(&ident(5000, 3, "Alpha", 7, 40));
        let json = s.to_json();
        assert_eq!(json["characters"][0]["gains"]["kinah_today"], json!({"value": 500, "updated_us": 2}));
        assert_eq!(State::from_json(&json).unwrap().get(&key(7, "Alpha")).unwrap().gains, s.get(&key(7, "Alpha")).unwrap().gains);

        let mut old = json.clone();
        old["characters"][0].as_object_mut().unwrap().remove("gains");
        assert!(State::from_json(&old).unwrap().get(&key(7, "Alpha")).unwrap().gains.is_empty());
        for bad in [json!("x"), json!({"kinah_today": {"value": "x", "updated_us": 1}}), json!({"k": {"value": 1}})] {
            old["characters"][0]["gains"] = bad;
            assert!(State::from_json(&old).is_err());
        }
    }

    #[test]
    fn a_repeated_identity_keeps_counters_and_updates_the_level() {
        let mut s = State::default();
        s.apply(&ident(5000, 1, "Alpha", 7, 40));
        s.apply(&counter(5000, 2, 1, 5));
        s.apply(&ident(5000, 3, "Alpha", 7, 41));
        let c = s.get(&key(7, "Alpha")).unwrap();
        assert_eq!((c.level, c.counters[&1].value), (41, 5));
    }

    #[test]
    fn a_removed_character_stays_gone_until_its_next_identity() {
        let mut s = State::default();
        s.apply(&ident(5000, 1, "Alpha", 7, 40));
        s.apply(&ident(5001, 1, "Beta", 7, 20));
        assert!(!s.remove(&key(7, "Nobody")));
        assert!(s.remove(&key(7, "Alpha")));
        assert!(s.get(&key(7, "Alpha")).is_none() && s.get(&key(7, "Beta")).is_some());
        assert_eq!(s.apply(&counter(5000, 2, 1, 5)), None); // still bound, but nothing to update
        assert!(s.get(&key(7, "Alpha")).is_none());
        s.apply(&ident(5000, 3, "Alpha", 7, 40));
        assert!(s.get(&key(7, "Alpha")).unwrap().counters.is_empty());
    }

    #[test]
    fn the_same_name_on_another_server_is_another_character() {
        let mut s = State::default();
        s.apply(&ident(5000, 1, "Alpha", 7, 40));
        s.apply(&ident(5001, 1, "Alpha", 8, 10));
        s.apply(&counter(5001, 2, 1, 3));
        assert!(s.get(&key(7, "Alpha")).unwrap().counters.is_empty());
        assert_eq!(s.get(&key(8, "Alpha")).unwrap().counters[&1].value, 3);
    }

    fn pets_snapshot() -> Event {
        Event::Pets { active: 2, levels: vec![(1, 1), (2, 0), (3, 3)], souls: vec![(1, 24), (2, 3)] }
    }

    #[test]
    fn the_pet_snapshot_is_held_until_the_identity_and_deltas_and_levels_apply_when_bound() {
        let mut s = State::default();
        assert_eq!(s.apply(&snapshot(5000, 1, vec![rec(1, 3, false)])), None);
        assert_eq!(s.apply(&ev(5000, 2, pets_snapshot())), None);
        assert!(s.get(&key(7, "Alpha")).is_none());
        assert_eq!(s.apply(&ident(5000, 3, "Alpha", 7, 40)), Some(key(7, "Alpha")));
        let p = s.get(&key(7, "Alpha")).unwrap().pets.clone().unwrap();
        assert_eq!((p.active, p.updated_us), (2, 2));
        assert_eq!(p.list[&1], Pet { level: 1, souls: 24 });
        assert_eq!(p.list[&2], Pet { level: 0, souls: 3 });
        assert_eq!(p.list[&3], Pet { level: 3, souls: 0 });

        assert_eq!(s.apply(&ev(5000, 4, Event::PetSoulsDelta(vec![(1, 1), (9, 2)]))), Some(key(7, "Alpha")));
        assert_eq!(s.apply(&ev(5000, 5, Event::PetLevel { id: 1, level: 2 })), Some(key(7, "Alpha")));
        let c = s.get(&key(7, "Alpha")).unwrap();
        let p = c.pets.as_ref().unwrap();
        assert_eq!((p.list[&1], p.list[&9], p.updated_us), (Pet { level: 2, souls: 0 }, Pet { level: 0, souls: 2 }, 5));
        assert_eq!(c.last_seen_us, 5);
    }

    #[test]
    fn pet_deltas_without_a_snapshot_are_lost_and_total_frames_are_ignored() {
        let mut s = State::default();
        s.apply(&ident(5000, 1, "Alpha", 7, 40));
        s.apply(&ev(5000, 2, Event::PetSoulsDelta(vec![(1, 1)])));
        s.apply(&ev(5000, 3, Event::PetLevel { id: 1, level: 2 }));
        assert_eq!(s.apply(&ev(5000, 4, Event::PetSoulsTotal(vec![(1, 25)]))), None);
        let c = s.get(&key(7, "Alpha")).unwrap();
        assert_eq!((c.pets.clone(), c.last_seen_us), (None, 3));
        assert_eq!(s.dropped, 0);
    }

    #[test]
    fn pets_survive_a_save_and_an_old_file_without_them_loads_as_none() {
        let mut s = State::default();
        s.apply(&ev(5000, 1, pets_snapshot()));
        s.apply(&ident(5000, 2, "Alpha", 7, 40));
        let json = s.to_json();
        assert_eq!(json["characters"][0]["pets"]["list"][0], json!({"id": 1, "level": 1, "souls": 24}));
        assert_eq!(State::from_json(&json).unwrap().to_json(), json);

        let mut old = json.clone();
        old["characters"][0].as_object_mut().unwrap().remove("pets");
        assert_eq!(State::from_json(&old).unwrap().get(&key(7, "Alpha")).unwrap().pets, None);
        for bad in [json!({"active": 1, "updated_us": 1}), json!({"active": 1, "updated_us": 1, "list": [{"id": 1}]})] {
            old["characters"][0]["pets"] = bad;
            assert!(State::from_json(&old).is_err());
        }
    }

    fn temp_dir() -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("acompanion-state-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample() -> State {
        let mut s = State::default();
        s.apply(&power(5000, 1, 12345));
        s.apply(&snapshot(5000, 3, vec![rec(1, 3, false), rec(2, 90_000, true)]));
        s.apply(&ident(5000, 4, "Alpha", 7, 40));
        s.apply(&ident(5001, 1, "Beta", 7, 20));
        s
    }

    #[test]
    fn json_round_trip_is_lossless() {
        let json = sample().to_json();
        assert_eq!(json["version"], 1);
        assert_eq!(json["characters"].as_array().unwrap().len(), 2);
        assert_eq!(State::from_json(&json).unwrap().to_json(), json);
    }

    #[test]
    fn a_missing_file_loads_empty() {
        let dir = temp_dir();
        let s = State::load(&dir.join("characters.json")).unwrap();
        assert!(s.to_json()["characters"].as_array().unwrap().is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_state_lock_excludes_other_writers_until_its_handle_closes() {
        let dir = temp_dir();
        let path = dir.join("nested").join("characters.json");
        let guard = try_lock(&path).unwrap().unwrap();
        assert!(!path.exists());
        let original = sample().to_json();
        sample().save(&path).unwrap();
        assert!(try_lock(&path).unwrap().is_none());
        assert_eq!(State::load(&path).unwrap().to_json(), original);
        assert!(try_lock(&dir.join("other.json")).unwrap().is_some());
        drop(guard);
        assert!(try_lock(&path).unwrap().is_some());
        assert!(path.with_extension("json.lock").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_corrupt_file_is_an_error_and_is_left_alone() {
        let dir = temp_dir();
        let path = dir.join("characters.json");
        std::fs::write(&path, b"{not json").unwrap();
        assert!(State::load(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{not json");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn save_replaces_the_file_and_leaves_no_tmp() {
        let dir = temp_dir();
        let path = dir.join("nested").join("characters.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"old").unwrap();
        let s = sample();
        s.save(&path).unwrap();
        assert_eq!(State::load(&path).unwrap().to_json(), s.to_json());
        assert!(!path.with_extension("json.tmp").exists());
        // a directory that does not exist yet is created
        let deep = dir.join("a").join("b").join("characters.json");
        s.save(&deep).unwrap();
        assert!(deep.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_wrong_version_or_a_missing_field_is_an_error() {
        let mut json = sample().to_json();
        json["version"] = 2.into();
        assert!(State::from_json(&json).is_err());

        let mut json = sample().to_json();
        json["characters"][0].as_object_mut().unwrap().remove("level");
        assert!(State::from_json(&json).is_err());

        let mut json = sample().to_json();
        json["characters"][0]["counters"][0].as_object_mut().unwrap().remove("updated_us");
        assert!(State::from_json(&json).is_err());
    }
}
