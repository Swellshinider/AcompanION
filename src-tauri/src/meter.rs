//! Live combat meter: damage dealt and taken per player, per skill and per target, split into
//! fights. Transient except for the finished fights, which `save_history` writes. The clock is the
//! capture's own, so replays are deterministic. Layouts and evidence: docs/protocol.md ("Combat").

use std::collections::{BTreeMap, HashSet, VecDeque};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{Value, json};

use crate::decode::Decoded;
use crate::events::Event;

/// A fight ends this long after its last hit.
/// ponytail: fixed gap; per-boss logic if a boss that goes untargetable splits real fights.
const FIGHT_GAP_US: u64 = 10_000_000;

/// Hits still in flight when a boss reaches 0 HP (a spirit's and a tick landed 0.10 and 0.15 s later
/// in a `spirit-1` boss) count in its fight; the fight closes at the first event after this.
/// ponytail: fixed 1 s; raise it if a slower tail shows up.
const BOSS_TAIL_US: u64 = 1_000_000;
/// Finished fights kept.
/// ponytail: fixed cap; make it a setting if anyone wants more.
pub const MAX_HISTORY: usize = 50;
/// Entities remembered at once in each map (players, monster templates, summon owners, health,
/// stagger gauges, engaged targets); the server despawns what leaves the view, so this only bounds a
/// run that lost despawn frames.
const MAX_ENTITIES: usize = 4096;

#[derive(Default, Clone, Copy)]
struct Tally {
    amount: u64,
    /// Every hit, ticks included (`hits`).
    hits: u32,
    /// Direct hits among `hits` (ticks are not): the crit rate is `crits / direct`.
    direct: u32,
    /// Critical hits among `direct`.
    crits: u32,
}

impl Tally {
    fn add(&mut self, amount: u32, crit: Option<bool>) {
        self.amount += u64::from(amount);
        self.hits += 1;
        self.direct += u32::from(crit.is_some());
        self.crits += u32::from(crit == Some(true));
    }

    /// With the direct and critical hits: only damage dealt has them verified.
    fn to_json(self) -> Value {
        json!({"amount": self.amount, "hits": self.hits, "direct": self.direct, "crits": self.crits})
    }

    /// Amount and hits only (damage taken, heals: their crits are not verified).
    fn plain(self) -> Value {
        json!({"amount": self.amount, "hits": self.hits})
    }
}

#[derive(Default)]
struct Who {
    /// Name and class code as the wire gave them (identity or player spawn); `None` when unknown.
    name: Option<String>,
    class_code: Option<u32>,
    dealt: Tally,
    taken: Tally,
    /// Raw wire skill id -> what it dealt. The frontend folds variants of one skill.
    skills: BTreeMap<u32, Tally>,
    /// Monster template (0 when the spawn was not seen) -> what was dealt to it.
    targets: BTreeMap<u32, Tally>,
    /// Healing done (what the target's HP had room for) and the rest of it, wasted at full HP.
    healed: Tally,
    overheal: u64,
    /// Healing received, self-heals included.
    received: Tally,
}

/// An entity's HP as the wire last gave it (`008d`).
#[derive(Default)]
pub(crate) struct Health {
    hp: Option<u64>,
    max: Option<u64>,
}

impl Health {
    pub(crate) fn set(&mut self, hp: Option<u64>, max: Option<u64>) {
        self.hp = hp.or(self.hp);
        self.max = max.or(self.max);
    }

    /// The part of a heal of `amount` the HP had room for (the rest is overheal): an unknown HP is full, an unknown max leaves no overheal.
    /// ponytail: the identity carries HP and max too (not decoded); a capture that starts after the
    /// world entry has no max until it changes, and counts every heal as done until then.
    pub(crate) fn heal(&mut self, amount: u32) -> u32 {
        let Some(max) = self.max else { return amount };
        let hp = self.hp.unwrap_or(max);
        let done = u64::from(amount).min(max.saturating_sub(hp));
        self.hp = Some(hp + done);
        done as u32
    }
}

/// A monster the players dealt damage to in a fight, with its HP and stagger as last seen. Kept
/// with the fight after it is gone, so a finished fight still shows its target.
struct Foe {
    template: u32,
    dealt: u64,
    hp: Option<u64>,
    /// From its spawn (`41 36`).
    max_hp: Option<u64>,
    /// `{state, value, max}` of its last `05 e0`.
    stagger: Option<Value>,
}

struct Fight {
    start_us: u64,
    last_us: u64,
    who: BTreeMap<u32, Who>,
    /// ponytail: bounded only by the fight gap (`FIGHT_GAP_US`), not capped inside one fight; a
    /// fight that never pauses keeps every foe it hit. Cap it by evicting the least damaged dead foe
    /// if a long open-world fight grows too large.
    foes: BTreeMap<u32, Foe>,
    /// The foe the fight shows, and when it was picked.
    target: Option<u32>,
    picked_us: u64,
    /// When a boss of this fight reached 0 HP: the fight closes at the first event after that instant.
    boss_dead_us: Option<u64>,
}

impl Fight {
    /// Keep the target while it lives; when it dies (HP 0), or at the first hit, take the living foe
    /// the players dealt the most damage to. A pick made at this instant is redone, so the frames
    /// of one packet count together. Ties go to the highest entity. When no foe lives, a target picked before
    /// stays, else the most damaged foe is taken (one killed by its first hit, its HP 0 coming first).
    /// ponytail: a reference meter picked a less damaged foe in two fights of one capture, for a reason
    /// not found. Record a fight with several monsters at once if the meter shows the wrong one.
    fn pick(&mut self, ts_us: u64) {
        let alive = |f: &Foe| f.hp != Some(0);
        if self.picked_us != ts_us && self.target.is_some_and(|t| self.foes.get(&t).is_some_and(alive)) {
            return;
        }
        let best = |living: bool| {
            self.foes.iter().filter(|(_, f)| !living || alive(f)).max_by_key(|(_, f)| f.dealt).map(|(&e, _)| e)
        };
        let keep = self.target.is_some() && self.picked_us != ts_us;
        if let Some(e) = best(true).or_else(|| if keep { None } else { best(false) }) {
            (self.target, self.picked_us) = (Some(e), ts_us);
        }
    }

    /// `me`: the player's own entity when the fight is shown or closed (entity ids are per session);
    /// `power` is their combat power.
    fn to_json(&self, me: Option<u32>, power: Option<u32>) -> Value {
        let players: Vec<Value> = self
            .who
            .iter()
            .map(|(entity, w)| {
                json!({
                    "entity": entity,
                    "name": w.name,
                    "class_code": w.class_code,
                    "combat_power": if me == Some(*entity) { power } else { None },
                    "dealt": w.dealt.to_json(),
                    "taken": w.taken.plain(),
                    "skills": w.skills.iter().map(|(&id, &t)| keyed(t, "id", id)).collect::<Vec<_>>(),
                    "targets": w.targets.iter().map(|(&template, &t)| keyed(t, "template", template)).collect::<Vec<_>>(),
                    "healing": w.healed.plain(),
                    "overheal": w.overheal,
                    "healing_received": w.received.plain(),
                })
            })
            .collect();
        let target = self.target.and_then(|e| {
            let f = self.foes.get(&e)?;
            Some(json!({"entity": e, "template": f.template, "hp": f.hp, "max_hp": f.max_hp, "stagger": f.stagger}))
        });
        json!({"start_us": self.start_us, "end_us": self.last_us, "me": me, "players": players, "target": target})
    }
}

/// `t` as JSON with `key: id` added (a skill's or a target's row).
fn keyed(t: Tally, key: &str, id: u32) -> Value {
    let mut v = t.to_json();
    v[key] = json!(id);
    v
}

#[derive(Default)]
pub struct Meter {
    /// The player's own entity, from the last identity.
    me: Option<u32>,
    /// The player's own combat power, as last sent.
    /// ponytail: other players' combat power is not decoded yet, so only the own row carries it.
    power: Option<u32>,
    /// `entity -> (name, class code)` of the players in view, the player's own included.
    /// Another player's hits count only on `engaged` targets.
    players: BTreeMap<u32, (String, u32)>,
    /// Targets the player (or their summon) has hit.
    /// ponytail: this rule rests on one capture only (`login`: 3 other players, a handful of their
    /// hits). Another player's hits on a target before the player's first hit on it are dropped;
    /// a reference meter credits them at that first hit; hold them here if it matters.
    engaged: HashSet<u32>,
    /// `entity -> template` of the monsters in view.
    templates: BTreeMap<u32, u32>,
    /// `summon -> owner`: a summon's hits are its owner's.
    owners: BTreeMap<u32, u32>,
    /// HP of the entities in view; the player's own decides overheal. The max comes at world entry,
    /// sometimes before the identity, so every entity's is kept.
    health: BTreeMap<u32, Health>,
    /// `entity -> {state, value, max}` of the last stagger frame (bosses); it comes before the first hit.
    staggers: BTreeMap<u32, Value>,
    fight: Option<Fight>,
    /// Finished fights as JSON, oldest first.
    history: VecDeque<Value>,
    /// Fights closed since the start, whatever the history cap dropped.
    closed: u64,
}

impl Meter {
    /// A meter that starts with `history` (the last `MAX_HISTORY` fights).
    pub fn with_history(history: Vec<Value>) -> Self {
        let skip = history.len().saturating_sub(MAX_HISTORY);
        Self { history: history.into_iter().skip(skip).collect(), ..Self::default() }
    }

    fn is_player(&self, entity: u32) -> bool {
        self.players.contains_key(&entity)
    }

    fn add_player(&mut self, entity: u32, name: &str, class_code: u32) {
        if self.players.len() >= MAX_ENTITIES {
            self.players.retain(|&e, _| Some(e) == self.me);
        }
        self.players.insert(entity, (name.to_owned(), class_code));
    }

    /// Fights closed so far (grows even when the history cap drops old ones).
    pub fn closed(&self) -> u64 {
        self.closed
    }

    /// Forget the finished fights; the open fight and `closed()` stay.
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Fold one event in; `true` when something the meter shows changed.
    /// ponytail: one meter for the whole capture, not per connection; a second game client on the
    /// same machine would mix them.
    pub fn apply(&mut self, d: &Decoded) -> bool {
        let mut changed = self.advance(d.ts_us);
        match d.event {
            Event::Identity { ref name, class_code, entity, .. } => {
                changed |= self.me != Some(entity);
                if let Some(old) = self.me.replace(entity) {
                    self.players.remove(&old);
                }
                self.add_player(entity, name, class_code);
            }
            Event::PlayerSpawn { entity, ref name, class_code } => self.add_player(entity, name, class_code),
            Event::CombatPower(p) => self.power = Some(p),
            Event::Spawn { entity, template, max_hp, .. } => {
                if self.templates.len() >= MAX_ENTITIES {
                    self.templates.clear();
                }
                self.templates.insert(entity, template);
                if max_hp.is_some() {
                    self.set_hp(d.ts_us, entity, None, max_hp);
                }
            }
            Event::Summon { entity, owner } => {
                if self.owners.len() >= MAX_ENTITIES {
                    self.owners.clear();
                }
                self.owners.insert(entity, owner);
            }
            Event::Gone { entity } => {
                self.templates.remove(&entity);
                if self.me != Some(entity) {
                    self.health.remove(&entity);
                }
                self.owners.remove(&entity);
                self.engaged.remove(&entity);
                self.staggers.remove(&entity);
                if self.me != Some(entity) {
                    self.players.remove(&entity);
                }
            }
            Event::Hit { target, attacker, skill, amount, crit } => {
                changed |= self.hit(d.ts_us, target, attacker, skill, amount, crit);
            }
            Event::Hp { entity, hp, max } => changed |= self.set_hp(d.ts_us, entity, hp, max),
            Event::Stagger { entity, state, value, max } => {
                if self.staggers.len() >= MAX_ENTITIES {
                    self.staggers.clear();
                }
                let gauge = json!({"state": state, "value": value, "max": max});
                self.staggers.insert(entity, gauge.clone());
                if let Some(foe) = self.fight.as_mut().and_then(|f| f.foes.get_mut(&entity)) {
                    foe.stagger = Some(gauge);
                    changed = true;
                }
            }
            Event::Heal { target, healer, amount, .. } => changed |= self.heal(target, healer, amount),
            _ => {}
        }
        changed
    }

    /// Move the clock: closes the fight when `FIGHT_GAP_US` passed since its last hit. `true` when it closed.
    pub fn advance(&mut self, ts_us: u64) -> bool {
        let over = |f: &Fight| {
            ts_us.saturating_sub(f.last_us) > FIGHT_GAP_US || f.boss_dead_us.is_some_and(|d| ts_us > d + BOSS_TAIL_US)
        };
        match self.fight.take_if(|f| over(f)) {
            Some(f) => {
                self.finish(&f);
                true
            }
            None => false,
        }
    }

    /// The connection ended: close the fight and forget who is who. `true` when anything changed.
    pub fn end_connection(&mut self) -> bool {
        let closing = self.fight.take();
        let changed = closing.is_some()
            || self.me.is_some()
            || !self.players.is_empty()
            || !self.templates.is_empty()
            || !self.owners.is_empty()
            || !self.engaged.is_empty()
            || !self.health.is_empty()
            || !self.staggers.is_empty();
        if let Some(f) = closing {
            self.finish(&f);
        }
        self.me = None;
        self.players.clear();
        self.templates.clear();
        self.owners.clear();
        self.engaged.clear();
        self.health.clear();
        self.staggers.clear();
        changed
    }

    /// An entity's HP or max HP; a foe of the current fight follows it. `true` when that foe changed.
    fn set_hp(&mut self, ts_us: u64, entity: u32, hp: Option<u64>, max: Option<u64>) -> bool {
        if self.health.len() >= MAX_ENTITIES {
            self.health.retain(|&e, _| Some(e) == self.me);
        }
        self.health.entry(entity).or_default().set(hp, max);
        let Some(fight) = self.fight.as_mut() else { return false };
        let Some(foe) = fight.foes.get_mut(&entity) else { return false };
        (foe.hp, foe.max_hp) = (hp.or(foe.hp), max.or(foe.max_hp));
        if hp == Some(0) && foe.stagger.is_some() {
            fight.boss_dead_us.get_or_insert(ts_us);
        }
        if hp == Some(0) && fight.target == Some(entity) {
            fight.pick(ts_us);
        }
        true
    }

    fn finish(&mut self, f: &Fight) {
        if self.history.len() == MAX_HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(f.to_json(self.me, self.power));
        self.closed += 1;
    }

    /// Damage taken: every hit on the player, and the hits on another player by an
    /// attacker in view, engaged or not. Those count in the current fight for the players already in
    /// it, and neither start nor extend it, like heals.
    /// ponytail: another player's damage taken from before their first counted hit is not added;
    /// keep it per player if that matters.
    /// ponytail: the attacker-in-view rule rests on one capture (`login`, 3 dropped hits), and a
    /// monster already in view before a mid-stream start is not in view, so its hits on other
    /// players are dropped; count any non-player attacker once a spawn was missed if that matters.
    /// ponytail: a hit on a player is never damage dealt. PvP and heals on an ally are unevidenced
    /// (every heal recorded so far targets its healer); an ally heal with the hit layout would
    /// otherwise parse as a hit and inflate `dealt`. Decode ally heals when a recording has them.
    fn hit(&mut self, ts_us: u64, target: u32, attacker: u32, skill: u32, amount: u32, crit: Option<bool>) -> bool {
        let in_view = self.templates.contains_key(&attacker) || self.owners.contains_key(&attacker) || self.is_player(attacker);
        let attacker = self.owners.get(&attacker).copied().unwrap_or(attacker);
        let mine = self.me == Some(attacker);
        let foe = !self.is_player(target);
        if mine && foe {
            if self.engaged.len() >= MAX_ENTITIES {
                self.engaged.clear();
            }
            self.engaged.insert(target);
        }
        let dealt = foe && self.is_player(attacker) && (mine || self.engaged.contains(&target));
        let on_me = self.me == Some(target);
        let on_other = !on_me && in_view && self.fight.as_ref().is_some_and(|f| f.who.contains_key(&target));
        if !dealt && !on_me {
            let Some(fight) = self.fight.as_mut().filter(|_| on_other) else { return false };
            who(fight, &self.players, target).taken.add(amount, crit);
            return true;
        }
        // only a hit on a boss opens a fight; once open, it counts its adds and the damage taken too
        // ponytail: the stagger gauge is the boss marker; a boss without one is not metered. Add a
        // curated boss list if one shows up.
        if self.fight.is_none() && !(dealt && self.staggers.contains_key(&target)) {
            return false;
        }
        let taken = on_me || on_other;
        let template = self.templates.get(&target).copied().unwrap_or(0);
        let fight = self.fight.get_or_insert_with(|| Fight {
            start_us: ts_us,
            last_us: ts_us,
            who: BTreeMap::new(),
            foes: BTreeMap::new(),
            target: None,
            picked_us: ts_us,
            boss_dead_us: None,
        });
        fight.last_us = ts_us;
        if dealt {
            let w = who(fight, &self.players, attacker);
            w.dealt.add(amount, crit);
            w.skills.entry(skill).or_default().add(amount, crit);
            w.targets.entry(template).or_default().add(amount, crit);
        }
        if dealt {
            let new = || {
                let (hp, max_hp) = self.health.get(&target).map_or((None, None), |h| (h.hp, h.max));
                Foe { template, dealt: 0, hp, max_hp, stagger: self.staggers.get(&target).cloned() }
            };
            fight.foes.entry(target).or_insert_with(new).dealt += u64::from(amount);
            fight.pick(ts_us);
        }
        if taken {
            who(fight, &self.players, target).taken.add(amount, crit);
        }
        true
    }

    /// A heal counts in the current fight for the player and the players already in it; it neither
    /// starts nor extends a fight. Only the player's own HP is followed (overheal); every other
    /// player's heals are taken as done.
    /// ponytail: heals outside a fight and before a player's first counted hit are not counted;
    /// keep them per player if the meter grows a session view.
    fn heal(&mut self, target: u32, healer: u32, amount: u32) -> bool {
        let healer = self.owners.get(&healer).copied().unwrap_or(healer);
        let done = match self.me {
            Some(me) if me == target => self.health.entry(me).or_default().heal(amount),
            _ => amount,
        };
        let me = self.me;
        let Some(fight) = self.fight.as_mut() else { return false };
        let counts = |fight: &Fight, e: u32| me == Some(e) || fight.who.contains_key(&e);
        let (by, to) = (counts(fight, healer), counts(fight, target));
        if by {
            let w = who(fight, &self.players, healer);
            w.healed.add(done, None);
            w.overheal += u64::from(amount - done);
        }
        if to {
            who(fight, &self.players, target).received.add(done, None);
        }
        by || to
    }

    /// The meter as ids and raw values; finished fights come through `history_json`.
    pub fn to_json(&self) -> Value {
        json!({"me": self.me, "current": self.fight.as_ref().map(|f| f.to_json(self.me, self.power)), "fights": self.history.len()})
    }

    pub fn history_json(&self) -> Value {
        Value::Array(self.history.iter().cloned().collect())
    }
}

/// `entity`'s row in `fight`, named from `players` when it is first seen there.
fn who<'a>(fight: &'a mut Fight, players: &BTreeMap<u32, (String, u32)>, entity: u32) -> &'a mut Who {
    let w = fight.who.entry(entity).or_default();
    if w.name.is_none()
        && let Some((name, class_code)) = players.get(&entity)
    {
        (w.name, w.class_code) = (Some(name.clone()), Some(*class_code));
    }
    w
}

/// Where the fights are saved: `meter.json` next to the state file.
pub fn history_path(state_path: &Path) -> PathBuf {
    state_path.with_file_name("meter.json")
}

/// The saved fights. A missing, unreadable or unknown file (not version 1, or a fight that is not
/// an object) is an empty history: it is a convenience, so it never stops capture.
/// ponytail: such a file is dropped silently and overwritten by the next save.
pub fn load_history(path: &Path) -> Vec<Value> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    let Ok(root) = serde_json::from_str::<Value>(&text) else { return Vec::new() };
    match root["fights"].as_array() {
        Some(fights) if root["version"] == 1 && fights.iter().all(Value::is_object) => fights.clone(),
        _ => Vec::new(),
    }
}

/// Write `<path>.tmp`, sync it, then rename it over `path`, like `State::save`.
pub fn save_history(path: &Path, meter: &Meter) -> Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let body = serde_json::to_vec(&json!({"version": 1, "fights": meter.history_json()}))?;
    let mut file = File::create(&tmp).with_context(|| format!("cannot create {}", tmp.display()))?;
    file.write_all(&body).and_then(|()| file.sync_all()).with_context(|| format!("cannot write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("cannot replace {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(ts_us: u64, event: Event) -> Decoded {
        Decoded { ts_us, conn: 1, event }
    }

    fn identity(entity: u32) -> Event {
        Event::Identity { name: "A".into(), server: 1, class_code: 1, level: 1, item_level: 1, entity }
    }

    fn hit(target: u32, attacker: u32, skill: u32, amount: u32) -> Event {
        Event::Hit { target, attacker, skill, amount, crit: Some(false) }
    }

    /// The stagger gauge that marks a boss: only a hit on one opens a fight.
    fn stagger(entity: u32) -> Event {
        Event::Stagger { entity, state: 1, value: Some(10), max: Some(10) }
    }

    const S: u64 = 1_000_000;

    #[test]
    fn a_fight_starts_at_the_first_hit_that_involves_a_player_and_adds_up() {
        let mut m = Meter::default();
        m.apply(&at(0, identity(77)));
        m.apply(&at(S, Event::Spawn { entity: 5, template: 2_100_025, x: 0, y: 0, max_hp: None }));
        m.apply(&at(S, stagger(5)));
        assert!(m.apply(&at(2 * S, hit(5, 77, 14_090_130, 100))));
        assert!(m.apply(&at(3 * S, Event::Hit { target: 5, attacker: 77, skill: 14_090_130, amount: 50, crit: Some(true) })));
        // a tick: a hit, but not a direct one
        assert!(m.apply(&at(4 * S, Event::Hit { target: 5, attacker: 77, skill: 14_800_007, amount: 7, crit: None })));
        let cur = m.to_json()["current"].clone();
        assert_eq!((cur["start_us"].clone(), cur["end_us"].clone(), cur["me"].clone()), (json!(2 * S), json!(4 * S), json!(77)));
        let p = &cur["players"][0];
        assert_eq!(p["entity"], 77);
        assert_eq!(p["dealt"], json!({"amount": 157, "hits": 3, "direct": 2, "crits": 1}));
        // crits are not verified on damage taken or heals: not emitted there
        assert_eq!(p["taken"], json!({"amount": 0, "hits": 0}));
        assert_eq!(
            p["skills"],
            json!([{"id": 14_090_130, "amount": 150, "hits": 2, "direct": 2, "crits": 1}, {"id": 14_800_007, "amount": 7, "hits": 1, "direct": 0, "crits": 0}])
        );
        assert_eq!(p["targets"], json!([{"template": 2_100_025, "amount": 157, "hits": 3, "direct": 2, "crits": 1}]));
    }

    #[test]
    fn hits_between_strangers_are_ignored_and_a_hit_on_the_player_is_damage_taken() {
        let mut m = Meter::default();
        m.apply(&at(0, identity(77)));
        m.apply(&at(0, stagger(5)));
        assert!(!m.apply(&at(S, hit(5, 8, 1, 10)))); // neither is a player
        assert!(!m.apply(&at(2 * S, hit(77, 5, 1, 30)))); // damage taken alone opens no fight
        assert_eq!(m.to_json()["current"], Value::Null);
        assert!(m.apply(&at(3 * S, hit(5, 77, 1, 10))));
        assert!(m.apply(&at(4 * S, hit(77, 5, 1, 30))));
        let p = m.to_json()["current"]["players"][0].clone();
        assert_eq!((p["taken"].clone(), p["dealt"].clone()), (json!({"amount": 30, "hits": 1}), json!({"amount": 10, "hits": 1, "direct": 1, "crits": 0})));
    }

    #[test]
    fn a_summons_hits_are_its_owners_until_it_is_gone() {
        let mut m = Meter::default();
        m.apply(&at(0, identity(77)));
        m.apply(&at(0, Event::Summon { entity: 500, owner: 77 }));
        m.apply(&at(0, stagger(5)));
        assert!(m.apply(&at(S, hit(5, 500, 14_170_037, 40))));
        let p = m.to_json()["current"]["players"][0].clone();
        assert_eq!((p["entity"].clone(), p["dealt"].clone()), (json!(77), json!({"amount": 40, "hits": 1, "direct": 1, "crits": 0})));
        assert_eq!(p["skills"], json!([{"id": 14_170_037, "amount": 40, "hits": 1, "direct": 1, "crits": 0}]));
        m.apply(&at(2 * S, Event::Gone { entity: 500 }));
        assert!(!m.apply(&at(3 * S, hit(5, 500, 14_170_037, 40)))); // a stranger now
    }

    #[test]
    fn the_own_row_carries_the_combat_power_and_the_others_do_not() {
        let mut m = Meter::default();
        m.apply(&at(0, Event::CombatPower(12_345)));
        m.apply(&at(0, identity(77)));
        m.apply(&at(0, Event::PlayerSpawn { entity: 88, name: "Testname".into(), class_code: 25 }));
        m.apply(&at(0, stagger(5)));
        m.apply(&at(S, hit(5, 77, 1, 10)));
        m.apply(&at(S, hit(5, 88, 2, 30)));
        let ps = m.to_json()["current"]["players"].clone();
        assert_eq!((ps[0]["combat_power"].clone(), ps[1]["combat_power"].clone()), (json!(12_345), Value::Null));
    }

    #[test]
    fn players_in_view_count_with_their_name_and_class_until_they_are_gone() {
        let mut m = Meter::default();
        m.apply(&at(0, identity(77)));
        m.apply(&at(0, Event::PlayerSpawn { entity: 88, name: "Testname".into(), class_code: 25 }));
        m.apply(&at(0, Event::Spawn { entity: 5, template: 2_100_025, x: 0, y: 0, max_hp: None }));
        m.apply(&at(0, stagger(5)));
        assert!(m.apply(&at(S, hit(5, 77, 1, 10)))); // the player engages 5 first
        assert!(m.apply(&at(S, hit(5, 88, 2, 30))));
        assert!(m.apply(&at(S, hit(88, 5, 3, 4))));
        let ps = m.to_json()["current"]["players"].clone();
        assert_eq!((ps[0]["entity"].clone(), ps[0]["name"].clone(), ps[0]["class_code"].clone()), (json!(77), json!("A"), json!(1)));
        assert_eq!((ps[1]["entity"].clone(), ps[1]["name"].clone(), ps[1]["class_code"].clone()), (json!(88), json!("Testname"), json!(25)));
        assert_eq!((ps[1]["dealt"].clone(), ps[1]["taken"].clone()), (json!({"amount": 30, "hits": 1, "direct": 1, "crits": 0}), json!({"amount": 4, "hits": 1})));
        m.apply(&at(2 * S, Event::Gone { entity: 88 }));
        assert!(!m.apply(&at(3 * S, hit(5, 88, 2, 30)))); // left the view: a stranger now
        // the name stays with the fight after the player left
        assert_eq!(m.to_json()["current"]["players"][1]["name"], "Testname");
        m.apply(&at(4 * S, Event::Gone { entity: 77 })); // never forgets the player's own entity
        assert!(m.apply(&at(5 * S, hit(5, 77, 1, 10))));
        // a new identity replaces the old own entity
        m.apply(&at(6 * S, identity(99)));
        assert!(!m.apply(&at(7 * S, hit(5, 77, 1, 10))));
        m.apply(&at(7 * S, Event::PlayerSpawn { entity: 88, name: "Testname".into(), class_code: 25 }));
        assert!(m.end_connection());
        assert!(!m.apply(&at(8 * S, hit(5, 88, 2, 30)))); // forgotten with the connection
    }

    #[test]
    fn another_players_hits_count_only_on_targets_the_player_engaged() {
        let mut m = Meter::default();
        m.apply(&at(0, identity(77)));
        m.apply(&at(0, Event::PlayerSpawn { entity: 88, name: "Testname".into(), class_code: 25 }));
        m.apply(&at(0, Event::Summon { entity: 500, owner: 77 }));
        m.apply(&at(0, Event::Spawn { entity: 5, template: 2_100_025, x: 0, y: 0, max_hp: None }));
        m.apply(&at(0, stagger(5)));
        m.apply(&at(0, stagger(6)));
        assert!(!m.apply(&at(S, hit(5, 88, 2, 30)))); // 5 not engaged: no fight
        assert!(!m.apply(&at(S, hit(88, 5, 3, 4)))); // nor a hit by 5 on 88
        assert_eq!(m.to_json()["current"], Value::Null);
        assert!(m.apply(&at(2 * S, hit(6, 77, 1, 10)))); // a fight on 6
        // the stranger on unengaged 5 does not keep it alive past the gap
        assert!(!m.apply(&at(10 * S, hit(5, 88, 2, 30))));
        assert!(m.advance(12 * S + 1));
        assert_eq!(m.history_json()[0]["players"].as_array().unwrap().len(), 1);
        // the summon engages 5 for its owner; then 88's hits on it and 5's on 88 count
        assert!(m.apply(&at(20 * S, hit(5, 500, 9, 1))));
        assert!(m.apply(&at(21 * S, hit(5, 88, 2, 30))));
        assert!(m.apply(&at(21 * S, hit(88, 5, 3, 4))));
        let p = m.to_json()["current"]["players"][1].clone();
        assert_eq!((p["dealt"].clone(), p["taken"].clone()), (json!({"amount": 30, "hits": 1, "direct": 1, "crits": 0}), json!({"amount": 4, "hits": 1})));
        // a target gone, or the connection ended, is no longer engaged
        m.apply(&at(22 * S, Event::Gone { entity: 5 }));
        assert!(!m.apply(&at(22 * S, hit(5, 88, 2, 30))));
        m.apply(&at(23 * S, hit(6, 77, 1, 10)));
        m.end_connection();
        m.apply(&at(24 * S, identity(77)));
        m.apply(&at(24 * S, Event::PlayerSpawn { entity: 88, name: "Testname".into(), class_code: 25 }));
        assert!(!m.apply(&at(25 * S, hit(6, 88, 2, 30))));
    }

    #[test]
    fn a_hit_on_a_player_is_never_damage_dealt() {
        let mut m = Meter::default();
        m.apply(&at(0, identity(77)));
        m.apply(&at(0, Event::PlayerSpawn { entity: 88, name: "Testname".into(), class_code: 25 }));
        m.apply(&at(0, stagger(5)));
        assert!(!m.apply(&at(S, hit(88, 77, 1, 50)))); // an ally heal read as a hit: no fight
        assert_eq!(m.to_json()["current"], Value::Null);
        assert!(m.apply(&at(2 * S, hit(5, 77, 1, 10))));
        assert!(m.apply(&at(2 * S, hit(5, 88, 2, 30))));
        m.apply(&at(3 * S, hit(88, 77, 1, 50)));
        m.apply(&at(3 * S, hit(77, 88, 2, 60)));
        let ps = m.to_json()["current"]["players"].clone();
        assert_eq!((ps[0]["dealt"]["amount"].clone(), ps[1]["dealt"]["amount"].clone()), (json!(10), json!(30)));
        assert_eq!(ps[0]["targets"], json!([{"template": 0, "amount": 10, "hits": 1, "direct": 1, "crits": 0}]));
        assert_eq!(m.to_json()["current"]["target"]["entity"], 5); // a player is never the target
    }

    #[test]
    fn an_identity_that_changes_the_players_entity_is_a_change() {
        let mut m = Meter::default();
        assert!(m.apply(&at(0, identity(77))));
        assert!(!m.apply(&at(S, identity(77))));
        assert!(m.apply(&at(2 * S, identity(78))));
        assert_eq!(m.to_json()["me"], 78);
    }

    #[test]
    fn a_player_in_the_fight_takes_damage_from_any_attacker_in_view_engaged_or_not() {
        let mut m = Meter::default();
        m.apply(&at(0, identity(77)));
        m.apply(&at(0, Event::PlayerSpawn { entity: 88, name: "Testname".into(), class_code: 25 }));
        m.apply(&at(0, Event::Spawn { entity: 5, template: 2_100_025, x: 0, y: 0, max_hp: None }));
        m.apply(&at(0, Event::Spawn { entity: 7, template: 2_100_025, x: 0, y: 0, max_hp: None }));
        m.apply(&at(0, stagger(5)));
        assert!(!m.apply(&at(S, hit(88, 7, 3, 10)))); // no fight yet: not counted
        assert!(m.apply(&at(2 * S, hit(5, 77, 1, 10)))); // the player engages 5 only
        assert!(m.apply(&at(2 * S, hit(5, 88, 2, 30)))); // 88 is in the fight
        // 7 is in view and was never hit by the player: its hit on 88 counts
        assert!(m.apply(&at(3 * S, hit(88, 7, 3, 10))));
        // 9 never spawned: not in view, not counted
        assert!(!m.apply(&at(3 * S, hit(88, 9, 3, 100))));
        assert_eq!(m.to_json()["current"]["players"][1]["taken"], json!({"amount": 10, "hits": 1}));
        // it does not extend the fight
        assert!(m.apply(&at(9 * S, hit(88, 7, 3, 10))));
        assert_eq!(m.to_json()["current"]["end_us"], 2 * S);
        // once 7 is gone its hits no longer count
        m.apply(&at(9 * S, Event::Gone { entity: 7 }));
        assert!(!m.apply(&at(9 * S, hit(88, 7, 3, 10))));
        assert_eq!(m.to_json()["current"]["players"][1]["taken"], json!({"amount": 20, "hits": 2}));
    }

    #[test]
    fn heals_count_in_a_fight_with_overheal_from_the_players_own_hp() {
        let heal = |target: u32, healer: u32, amount: u32| Event::Heal { target, healer, skill: 9, amount };
        let mut m = Meter::default();
        m.apply(&at(0, Event::Hp { entity: 77, hp: None, max: Some(1000) })); // before the identity
        m.apply(&at(0, identity(77)));
        m.apply(&at(0, Event::PlayerSpawn { entity: 88, name: "Testname".into(), class_code: 25 }));
        m.apply(&at(0, Event::Summon { entity: 500, owner: 77 }));
        m.apply(&at(0, stagger(5)));
        assert!(!m.apply(&at(S, heal(77, 77, 50)))); // no fight: not counted, and HP unknown is full
        m.apply(&at(S, Event::Hp { entity: 77, hp: Some(900), max: None }));
        assert!(m.apply(&at(2 * S, hit(5, 77, 1, 10))));
        assert!(m.apply(&at(2 * S, heal(77, 77, 60)))); // 60 done
        assert!(m.apply(&at(3 * S, heal(77, 500, 70)))); // the summon's: 40 done, 30 over (HP 960 of 1000)
        assert!(!m.apply(&at(3 * S, heal(88, 88, 25)))); // 88 is not in the fight
        let p = m.to_json()["current"]["players"][0].clone();
        assert_eq!(p["healing"], json!({"amount": 100, "hits": 2}));
        assert_eq!(p["overheal"], 30);
        assert_eq!(p["healing_received"], json!({"amount": 100, "hits": 2}));
        // once 88 is in the fight its heals count, as done: only the player's HP is followed
        assert!(m.apply(&at(4 * S, hit(5, 88, 2, 30))));
        assert!(m.apply(&at(4 * S, heal(88, 88, 25))));
        let p = m.to_json()["current"]["players"][1].clone();
        assert_eq!((p["healing"]["amount"].clone(), p["overheal"].clone()), (json!(25), json!(0)));
        // heals do not keep a fight alive
        assert!(m.apply(&at(13 * S, heal(77, 77, 5))));
        assert!(m.advance(14 * S + 1));
    }

    #[test]
    fn the_target_is_the_most_damaged_living_foe_kept_until_it_dies() {
        let hp = |entity: u32, hp: u64| Event::Hp { entity, hp: Some(hp), max: None };
        let mut m = Meter::default();
        m.apply(&at(0, identity(77)));
        m.apply(&at(0, Event::PlayerSpawn { entity: 88, name: "Testname".into(), class_code: 25 }));
        m.apply(&at(0, Event::Spawn { entity: 5, template: 2_100_025, x: 0, y: 0, max_hp: Some(1000) }));
        m.apply(&at(0, Event::Spawn { entity: 6, template: 2_100_026, x: 0, y: 0, max_hp: Some(500) }));
        m.apply(&at(0, Event::Stagger { entity: 5, state: 1, value: Some(240), max: Some(240) }));
        assert!(!m.apply(&at(0, hp(5, 1000)))); // no fight yet
        assert!(!m.apply(&at(S, hit(77, 5, 1, 9)))); // damage taken alone: no fight
        assert_eq!(m.to_json()["current"], Value::Null);
        // one packet: the boss opens the fight, 6 ties and takes the pick, then 5 gets more damage; the pick is redone at the same instant
        m.apply(&at(2 * S, hit(5, 77, 1, 1)));
        m.apply(&at(2 * S, hit(6, 77, 1, 1)));
        m.apply(&at(2 * S, hit(5, 77, 1, 99)));
        assert!(m.apply(&at(2 * S, hp(5, 900))));
        let want = json!({"entity": 5, "template": 2_100_025, "hp": 900, "max_hp": 1000, "stagger": {"state": 1, "value": 240, "max": 240}});
        assert_eq!(m.to_json()["current"]["target"], want);
        // kept while alive, even when 6 has taken more (88's hit counts: the player engaged 6)
        m.apply(&at(3 * S, hit(6, 77, 1, 200)));
        m.apply(&at(3 * S, hit(6, 88, 2, 100)));
        assert!(m.apply(&at(3 * S, Event::Stagger { entity: 5, state: 3, value: None, max: None })));
        assert_eq!(m.to_json()["current"]["target"]["entity"], 5);
        assert_eq!(m.to_json()["current"]["target"]["stagger"], json!({"state": 3, "value": null, "max": null}));
        // 5 dies: the living 6 is the target until the fight closes with its boss
        assert!(m.apply(&at(4 * S, hp(5, 0))));
        assert_eq!(m.to_json()["current"]["target"]["entity"], 6);
        // 6 dies and despawns before the fight closes: it stays that finished fight's target
        m.apply(&at(4 * S + 500_000, hp(6, 0)));
        m.apply(&at(4 * S + 600_000, Event::Gone { entity: 6 }));
        m.advance(6 * S);
        assert_eq!(
            m.history_json()[0]["target"],
            json!({"entity": 6, "template": 2_100_026, "hp": 0, "max_hp": 500, "stagger": null})
        );
    }

    #[test]
    fn a_foe_killed_by_its_first_hit_is_the_target_when_no_foe_lives() {
        let mut m = Meter::default();
        m.apply(&at(0, identity(77)));
        m.apply(&at(0, stagger(5)));
        // one packet: the HP 0 comes before the hit that killed it
        m.apply(&at(S, Event::Hp { entity: 5, hp: Some(0), max: None }));
        assert!(m.apply(&at(S, hit(5, 77, 1, 100))));
        assert_eq!(m.to_json()["current"]["target"]["entity"], 5);
        // another one-shot later, with less damage: the target picked before stays
        m.apply(&at(2 * S, Event::Hp { entity: 6, hp: Some(0), max: None }));
        m.apply(&at(2 * S, hit(6, 77, 1, 50)));
        assert_eq!(m.to_json()["current"]["target"]["entity"], 5);
        // a living foe takes over
        m.apply(&at(3 * S, hit(7, 77, 1, 1)));
        assert_eq!(m.to_json()["current"]["target"]["entity"], 7);
    }

    #[test]
    fn a_fight_ends_after_ten_seconds_without_hits_and_the_next_hit_starts_another() {
        let mut m = Meter::default();
        m.apply(&at(0, identity(77)));
        m.apply(&at(0, stagger(5)));
        m.apply(&at(S, hit(5, 77, 1, 10)));
        assert!(!m.advance(10 * S)); // nine seconds since the last hit: still the same fight
        assert!(m.advance(11 * S + 1));
        let j = m.to_json();
        assert_eq!((j["current"].clone(), j["fights"].clone()), (Value::Null, json!(1)));
        assert_eq!(m.closed(), 1);
        m.apply(&at(30 * S, hit(5, 77, 1, 20)));
        assert_eq!(m.to_json()["current"]["start_us"], 30 * S);
        assert_eq!(m.history_json()[0]["players"][0]["dealt"], json!({"amount": 10, "hits": 1, "direct": 1, "crits": 0}));
    }

    #[test]
    fn each_boss_gets_its_own_fight() {
        let boss = |entity: u32, template: u32| {
            [
                Event::Spawn { entity, template, x: 0, y: 0, max_hp: Some(1000) },
                Event::Stagger { entity, state: 1, value: Some(10), max: Some(10) },
            ]
        };
        let mut m = Meter::default();
        m.apply(&at(0, identity(77)));
        m.apply(&at(0, Event::Spawn { entity: 4, template: 2_100_025, x: 0, y: 0, max_hp: None }));
        for e in boss(5, 2_300_909).into_iter().chain(boss(6, 2_300_910)) {
            m.apply(&at(0, e));
        }
        // trash opens no fight, then a boss does
        assert!(!m.apply(&at(S, hit(4, 77, 1, 10))));
        m.apply(&at(2 * S, hit(5, 77, 1, 100)));
        assert_eq!(m.closed(), 0);
        // the boss dies: hits in flight (the same instant, and 0.2 s later) still count
        m.apply(&at(3 * S, Event::Hp { entity: 5, hp: Some(0), max: None }));
        m.apply(&at(3 * S, hit(5, 77, 1, 50)));
        m.apply(&at(3 * S + 200_000, hit(5, 77, 1, 25)));
        assert_eq!(m.closed(), 0);
        // a second boss right away: the first fight is over, another starts
        m.apply(&at(4 * S + 1, hit(6, 77, 1, 70)));
        assert_eq!(m.closed(), 1);
        let first = &m.history_json()[0];
        assert_eq!((first["target"]["template"].clone(), first["players"][0]["dealt"]["amount"].clone()), (json!(2_300_909), json!(175)));
        assert_eq!(m.to_json()["current"]["target"]["template"], 2_300_910);
    }

    #[test]
    fn only_a_boss_opens_a_fight_and_the_closed_boss_fight_stays_the_latest() {
        let mut m = Meter::default();
        m.apply(&at(0, identity(77)));
        m.apply(&at(0, Event::Spawn { entity: 4, template: 2_100_025, x: 0, y: 0, max_hp: None }));
        m.apply(&at(0, stagger(5)));
        assert!(!m.apply(&at(S, hit(4, 77, 1, 10)))); // trash alone: no fight
        assert!(!m.apply(&at(S, hit(77, 4, 1, 10)))); // nor damage taken from it
        assert_eq!(m.to_json()["current"], Value::Null);
        assert!(m.apply(&at(2 * S, hit(5, 77, 1, 100))));
        assert!(m.apply(&at(2 * S, hit(4, 77, 1, 10)))); // inside a boss fight its adds count
        assert_eq!(m.to_json()["current"]["players"][0]["targets"].as_array().unwrap().len(), 2);
        m.apply(&at(3 * S, Event::Hp { entity: 5, hp: Some(0), max: None }));
        assert!(m.advance(5 * S));
        // trash after the boss's fight does not replace it
        assert!(!m.apply(&at(6 * S, hit(4, 77, 1, 10))));
        assert_eq!(m.to_json()["current"], Value::Null);
        assert_eq!(m.history_json().as_array().unwrap().len(), 1);
        assert_eq!(m.history_json()[0]["players"][0]["dealt"]["amount"], 110);
    }

    #[test]
    fn a_connection_ending_closes_the_fight_and_forgets_who_is_who() {
        let mut m = Meter::default();
        m.apply(&at(0, identity(77)));
        m.apply(&at(0, stagger(5)));
        m.apply(&at(S, hit(5, 77, 1, 10)));
        assert!(m.end_connection());
        assert!(!m.end_connection()); // nothing left to close
        assert_eq!(m.to_json()["me"], Value::Null);
        assert!(!m.apply(&at(2 * S, hit(5, 77, 1, 10)))); // 77 is nobody until the next identity
        assert_eq!(m.history_json().as_array().unwrap().len(), 1);
    }

    #[test]
    fn history_keeps_the_last_fights_only() {
        let mut m = Meter::default();
        m.apply(&at(0, identity(77)));
        m.apply(&at(0, stagger(5)));
        for i in 0..MAX_HISTORY as u64 + 3 {
            m.apply(&at(i * 20 * S, hit(5, 77, 1, 1)));
        }
        m.end_connection();
        let h = m.history_json();
        assert_eq!(h.as_array().unwrap().len(), MAX_HISTORY);
        assert_eq!(h[0]["start_us"], 3 * 20 * S); // the three oldest were dropped
        assert_eq!(m.closed(), MAX_HISTORY as u64 + 3);
    }

    #[test]
    fn clearing_the_history_keeps_the_open_fight_and_the_closed_count() {
        let mut m = Meter::default();
        m.apply(&at(0, identity(77)));
        m.apply(&at(0, stagger(5)));
        m.apply(&at(S, hit(5, 77, 1, 10)));
        m.end_connection();
        m.apply(&at(2 * S, identity(77)));
        m.apply(&at(2 * S, stagger(5)));
        m.apply(&at(3 * S, hit(5, 77, 1, 10)));
        assert_eq!(m.history_json().as_array().unwrap().len(), 1);
        m.clear_history();
        assert_eq!(m.history_json(), json!([]));
        assert_eq!(m.closed(), 1);
        assert!(!m.to_json()["current"].is_null()); // the open fight stays
    }

    #[test]
    fn history_survives_a_save_and_a_missing_or_corrupt_file_is_empty() {
        let dir = std::env::temp_dir().join(format!("acompanion-meter-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("meter.json");
        assert!(load_history(&path).is_empty());
        let mut m = Meter::default();
        m.apply(&at(0, identity(77)));
        m.apply(&at(0, stagger(5)));
        m.apply(&at(S, hit(5, 77, 9, 10)));
        m.end_connection();
        save_history(&path, &m).unwrap();
        assert!(!dir.join("meter.json.tmp").exists());
        let loaded = Meter::with_history(load_history(&path));
        assert_eq!(loaded.history_json(), m.history_json());
        assert_eq!(loaded.history_json()[0]["me"], 77); // which row is the player's
        // a corrupt file, an unknown version or a fight that is not an object is an empty history
        for body in ["not json", r#"{"version":2,"fights":[{}]}"#, r#"{"fights":[{}]}"#, r#"{"version":1,"fights":[{},3]}"#] {
            std::fs::write(&path, body).unwrap();
            assert!(load_history(&path).is_empty(), "{body}");
        }
        std::fs::write(&path, r#"{"version":1,"fights":[{}]}"#).unwrap();
        assert_eq!(load_history(&path).len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
