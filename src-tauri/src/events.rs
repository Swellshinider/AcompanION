//! Typed events decoded from server frames. Rust emits ids and raw values only, never display
//! text. Layouts and evidence: docs/protocol.md ("Opcode table").

use std::collections::HashMap;

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

const TABLE: &str = include_str!("../opcodes.json");

/// Largest `0b 61` frame accepted (sanity bound).
/// ponytail: real snapshots are ~500 bytes; raise if a patch adds many counters.
const MAX_SNAPSHOT: usize = 8 << 10;
/// Most Daeva Pass counters in one `24 e3` frame (two seen; sanity bound).
const MAX_PASS: u8 = 8;
/// Counter ids stay below this; used to reject a misaligned "next id".
const MAX_ID: u32 = 100_000_000;
/// Effects (the `u32` id a hit or tick carries) that restore something other than HP: none of
/// them counts as healing. The wire does not say which resource an effect restores.
/// ponytail: the four seen in the captures (the MP part of an HP and MP restore, two parts of
/// another restore, a Sorcerer's periodic self effect); a skill table would cover the rest.
const NOT_HP: [u32; 4] = [19_009_112, 190_000_151, 190_000_161, 1_576_000_711];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Identity,
    CombatPower,
    ItemLevel,
    QuestBoards,
    QuestEntry,
    KinahToday,
    DutyMissions,
    PassUsed,
    CountersSnapshot,
    CounterUpdate,
    PetsSnapshot,
    PetSoulsDelta,
    PetLevel,
    PetSoulsTotal,
    MonsterSpawn,
    MonsterMoveShort,
    MonsterMoveJump,
    MonsterMove,
    MonsterGone,
    PlayerSpawn,
    SkillPos,
    ZoneLoad,
    MapEnter,
    Hit,
    HitTick,
    Hp,
    Stagger,
}

/// Key in `opcodes.json` for each kind.
const KINDS: [(&str, Kind); 31] = [
    ("identity", Kind::Identity),
    ("combat_power", Kind::CombatPower),
    ("item_level", Kind::ItemLevel),
    ("quest_boards", Kind::QuestBoards),
    ("quest_entry", Kind::QuestEntry),
    ("kinah_today", Kind::KinahToday),
    ("kinah_today_update", Kind::KinahToday),
    ("duty_missions", Kind::DutyMissions),
    ("pass_used", Kind::PassUsed),
    ("counters_snapshot", Kind::CountersSnapshot),
    ("counter_update", Kind::CounterUpdate),
    ("pets_snapshot", Kind::PetsSnapshot),
    ("pet_souls_delta", Kind::PetSoulsDelta),
    ("pet_level", Kind::PetLevel),
    ("pet_souls_total", Kind::PetSoulsTotal),
    ("monster_spawn", Kind::MonsterSpawn),
    ("monster_move_a", Kind::MonsterMoveShort),
    ("monster_move_b", Kind::MonsterMove),
    ("monster_move_c", Kind::MonsterMove),
    ("monster_move_d", Kind::MonsterMoveJump),
    ("monster_gone", Kind::MonsterGone),
    ("player_spawn", Kind::PlayerSpawn),
    ("player_gone", Kind::MonsterGone),
    ("skill_pos_a", Kind::SkillPos),
    ("skill_pos_b", Kind::SkillPos),
    ("zone_load", Kind::ZoneLoad),
    ("map_enter", Kind::MapEnter),
    ("hit", Kind::Hit),
    ("hit_tick", Kind::HitTick),
    ("hp", Kind::Hp),
    ("stagger", Kind::Stagger),
];

/// A checklist counter as sent on the wire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CounterRecord {
    pub id: u32,
    /// Remaining amount: a count, or milliseconds when `ms` (time counters).
    pub value: u64,
    pub ms: bool,
}

impl CounterRecord {
    /// A count, or whole seconds for time counters: the unit the checklist counters are compared in.
    pub fn normalized(&self) -> i64 {
        if self.ms { (self.value / 1000) as i64 } else { self.value as i64 }
    }

    fn to_json(&self) -> Value {
        json!({"id": self.id, "value": self.value, "ms": self.ms})
    }
}

/// One quest board of the `56 e2` snapshot: a window (`slot` 1 = daily, 2 = weekly) and how many
/// of its entries are done.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Board {
    pub slot: u8,
    pub group: u32,
    pub done: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// `entity` is the player's own id in the world; skill frames led by it carry the player's position.
    Identity { name: String, server: u16, class_code: u32, level: u32, item_level: u32, entity: u32 },
    CombatPower(u32),
    /// Gear score (item level): sent live as equipment changes, and again in the identity.
    ItemLevel(u32),
    /// Quest boards (world entry); the Supply Emergency counts are the done entries of slots 1 and 2.
    Boards(Vec<Board>),
    /// One board entry changed (a Supply Emergency delivery).
    BoardEntry { group: u32, id: u32, value: u32, done: bool },
    /// Kinah gained today (snapshot at world entry, then on every change).
    KinahToday(u64),
    /// Duty Missions done today (world entry, then on every change).
    DutyMissions(u32),
    /// Daeva Pass weekly counters (world entry, then on change): `(counter id, used)` pairs; the
    /// wire carries what is used, not what is left.
    PassUsed(Vec<(u32, u32)>),
    /// Full checklist snapshot (world entry).
    Counters(Vec<CounterRecord>),
    /// One counter changed.
    Counter(CounterRecord),
    /// Pet collection snapshot: `levels` and `souls` are `(pet id, value)` pairs.
    Pets { active: u32, levels: Vec<(u32, u32)>, souls: Vec<(u32, u32)> },
    /// Souls gained, per pet (a delta).
    PetSoulsDelta(Vec<(u32, u32)>),
    PetLevel { id: u32, level: u32 },
    /// Cumulative souls, per pet.
    PetSoulsTotal(Vec<(u32, u32)>),
    /// A monster came into view. Positions (here and below) are hundredths of a map unit: the
    /// wire's f32 rounded, `/ 100` is the unit of `pet_monsters.spawns`.
    /// `max_hp` when the frame carries it in the layout found so far (never on a named spawn).
    Spawn { entity: u32, template: u32, x: i32, y: i32, max_hp: Option<u64> },
    /// An entity owned by a player came into view (a summon): its hits are the owner's damage.
    Summon { entity: u32, owner: u32 },
    /// Another player came into view (`class_code` as in `Identity`; `combat_power` read off the
    /// frame's tail, `None` when the tail does not fit). Leaving the view is a `Gone`.
    PlayerSpawn { entity: u32, name: String, class_code: u32, combat_power: Option<u32> },
    /// An entity moved.
    Pos { entity: u32, x: i32, y: i32 },
    /// A skill frame led by `entity`. When it is the player's own (the identity's) the position is
    /// theirs; led by a monster it is somebody else's, so only the own entity's is used.
    Cast { entity: u32, x: i32, y: i32 },
    /// An entity left the view (or died and was removed).
    Gone { entity: u32 },
    /// The player's own position at a zone load.
    Teleport { x: i32, y: i32 },
    /// The player entered a map (`map` is the game's map id, e.g. 1010 for Verteron), also at world entry.
    MapEnter { map: u32 },
    /// One damage event: `attacker` dealt `amount` to `target` with the raw wire `skill` id (the
    /// game's skill id plus a variant, see docs/protocol.md "Combat"). Direct hits and damage ticks;
    /// `crit` says whether a direct hit was critical and is `None` on a tick, which carries no such flag.
    Hit { target: u32, attacker: u32, skill: u32, amount: u32, crit: Option<bool> },
    /// `healer` healed `target` by `amount` with `skill`, the raw amount: the part the target's HP
    /// had no room for (overheal) is the meter's to work out from `Hp` (docs/protocol.md "Combat").
    Heal { target: u32, healer: u32, skill: u32, amount: u32 },
    /// An entity's HP and max HP (`008d`), each when the frame carries it.
    Hp { entity: u32, hp: Option<u64>, max: Option<u64> },
    /// A boss's stagger gauge (`05e0`): `value` of `max` with `state` 1 while it fills, both `None`
    /// with `state` 3 when it broke (docs/protocol.md "Combat").
    Stagger { entity: u32, state: u8, value: Option<u32>, max: Option<u32> },
}

fn pairs(v: &[(u32, u32)]) -> Value {
    Value::Array(v.iter().map(|&(a, b)| json!([a, b])).collect())
}

impl Event {
    pub fn to_json(&self) -> Value {
        match self {
            Event::Identity { name, server, class_code, level, item_level, entity } => {
                json!({"type": "identity", "name": name, "server": server, "class_code": class_code, "level": level, "item_level": item_level, "entity": entity})
            }
            Event::CombatPower(p) => json!({"type": "combat_power", "value": p}),
            Event::ItemLevel(v) => json!({"type": "item_level", "value": v}),
            Event::Boards(bs) => {
                let boards: Vec<Value> =
                    bs.iter().map(|b| json!({"slot": b.slot, "group": b.group, "done": b.done})).collect();
                json!({"type": "boards", "boards": boards})
            }
            Event::BoardEntry { group, id, value, done } => {
                json!({"type": "board_entry", "group": group, "id": id, "value": value, "done": done})
            }
            Event::KinahToday(v) => json!({"type": "kinah_today", "value": v}),
            Event::DutyMissions(v) => json!({"type": "duty_missions", "value": v}),
            Event::PassUsed(v) => json!({"type": "pass_used", "entries": pairs(v)}),
            Event::Counters(rs) => {
                json!({"type": "counters", "records": rs.iter().map(CounterRecord::to_json).collect::<Vec<_>>()})
            }
            Event::Counter(r) => json!({"type": "counter", "record": r.to_json()}),
            Event::Pets { active, levels, souls } => {
                json!({"type": "pets", "active": active, "levels": pairs(levels), "souls": pairs(souls)})
            }
            Event::PetSoulsDelta(v) => json!({"type": "pet_souls_delta", "entries": pairs(v)}),
            Event::PetLevel { id, level } => json!({"type": "pet_level", "id": id, "level": level}),
            Event::PetSoulsTotal(v) => json!({"type": "pet_souls_total", "entries": pairs(v)}),
            Event::Spawn { entity, template, x, y, max_hp } => {
                json!({"type": "spawn", "entity": entity, "template": template, "x": x, "y": y, "max_hp": max_hp})
            }
            Event::Summon { entity, owner } => json!({"type": "summon", "entity": entity, "owner": owner}),
            Event::PlayerSpawn { entity, name, class_code, combat_power } => {
                json!({"type": "player_spawn", "entity": entity, "name": name, "class_code": class_code, "combat_power": combat_power})
            }
            Event::Pos { entity, x, y } => json!({"type": "pos", "entity": entity, "x": x, "y": y}),
            Event::Cast { entity, x, y } => json!({"type": "cast", "entity": entity, "x": x, "y": y}),
            Event::Gone { entity } => json!({"type": "gone", "entity": entity}),
            Event::Teleport { x, y } => json!({"type": "teleport", "x": x, "y": y}),
            Event::MapEnter { map } => json!({"type": "map_enter", "map": map}),
            Event::Hit { target, attacker, skill, amount, crit } => {
                json!({"type": "hit", "target": target, "attacker": attacker, "skill": skill, "amount": amount, "crit": crit})
            }
            Event::Heal { target, healer, skill, amount } => {
                json!({"type": "heal", "target": target, "healer": healer, "skill": skill, "amount": amount})
            }
            Event::Hp { entity, hp, max } => json!({"type": "hp", "entity": entity, "hp": hp, "max": max}),
            Event::Stagger { entity, state, value, max } => {
                json!({"type": "stagger", "entity": entity, "state": state, "value": value, "max": max})
            }
        }
    }
}

/// Opcode → event kind for one game build.
pub struct Opcodes {
    by_op: HashMap<[u8; 2], Kind>,
}

fn parse_hex2(s: &str) -> Result<[u8; 2]> {
    ensure!(s.len() == 4 && s.is_ascii(), "opcode {s:?} is not 4 hex digits");
    Ok([u8::from_str_radix(&s[0..2], 16)?, u8::from_str_radix(&s[2..4], 16)?])
}

impl Opcodes {
    /// The table for `build` from the `opcodes.json` compiled into the binary.
    /// ponytail: one embedded table, no build detection. Load an external file and pick the build
    /// from the stream once a patch shows which opcodes move.
    pub fn embedded(build: &str) -> Result<Self> {
        Self::from_json(TABLE, build)
    }

    pub fn from_json(text: &str, build: &str) -> Result<Self> {
        let root: Value = serde_json::from_str(text)?;
        let table = root["builds"][build]
            .as_object()
            .with_context(|| format!("no opcode table for build {build:?}"))?;
        let mut by_op = HashMap::new();
        for (name, kind) in KINDS {
            let hex = table
                .get(name)
                .and_then(Value::as_str)
                .with_context(|| format!("opcode {name:?} missing for build {build:?}"))?;
            by_op.insert(parse_hex2(hex)?, kind);
        }
        Ok(Self { by_op })
    }

    /// Every event of one frame: `parse`, then the heal a draining hit gives its attacker.
    pub fn parse_all(&self, frame: &[u8]) -> impl Iterator<Item = Event> {
        self.parse(frame).into_iter().chain(self.drain(frame))
    }

    /// A `04 38` hit with bit 2 of its second byte heals the attacker by a varint at the end:
    /// `[amount]`, the multi-hit list when `flags & 0x20` (`[u8 n][n varints]`), 2 bytes, `[varint heal]`.
    /// ponytail: none seen with `flags & 0x50` (more fields before the trailer, not decoded); such
    /// a drain is dropped. Decode those fields if the healing total comes up short.
    fn drain(&self, frame: &[u8]) -> Option<Event> {
        let op: [u8; 2] = frame.get(..2)?.try_into().ok()?;
        if self.by_op.get(&op) != Some(&Kind::Hit) {
            return None;
        }
        let mut c = Cur { b: frame, i: 2 };
        let h = hit_head(&mut c)?;
        if h.extra & 4 == 0 || h.flags & 0x50 != 0 || h.target == h.attacker {
            return None;
        }
        if h.flags & 0x20 != 0 {
            for _ in 0..c.u8()? {
                c.varint()?;
            }
        }
        c.take(2)?;
        Some(Event::Heal { target: h.attacker, healer: h.attacker, skill: h.skill, amount: c.varint()? })
    }

    /// Decode one frame body (opcode first). `None` for an unknown opcode or a frame that does
    /// not parse; never panics on malformed input. A frame can carry a second event (a drain
    /// heal) that this leaves out: only `parse_all` returns every event of a frame.
    pub(crate) fn parse(&self, frame: &[u8]) -> Option<Event> {
        let op: [u8; 2] = frame.get(..2)?.try_into().ok()?;
        let kind = *self.by_op.get(&op)?;
        let mut c = Cur { b: frame, i: 2 };
        match kind {
            Kind::Identity => {
                let entity = c.varint()?;
                // ponytail: the prefix is a 2-byte entity varint plus 5 bytes in every capture; the
                // name is read at a fixed offset, so an entity id above 16383 would misalign it.
                c.i = 9;
                let n = usize::from(c.u8()?);
                let name = String::from_utf8(c.take(n)?.to_vec()).ok()?;
                let server = c.u16()?;
                let class_code = c.u32()?;
                c.u8()?;
                let level = c.u32()?;
                // ponytail: sent as an equal u32 pair in every capture; only the first is read.
                let item_level = c.u32()?;
                Some(Event::Identity { name, server, class_code, level, item_level, entity })
            }
            Kind::CombatPower => {
                c.take(8)?;
                Some(Event::CombatPower(c.u32()?))
            }
            Kind::ItemLevel => {
                let v = c.u32()?;
                c.u32()?; // the same value again
                if !c.rest().is_empty() {
                    return None;
                }
                Some(Event::ItemLevel(v))
            }
            Kind::QuestBoards => {
                // The done bits of all boards share one LSB-first pool: a byte is pulled from the
                // stream whenever the pool runs dry, so flag bytes sit between the 12-byte entries.
                let (mut pool, mut left) = (0u8, 0u8);
                let mut boards = Vec::new();
                for _ in 0..c.u8()? {
                    let group = c.u32()?;
                    let slot = c.u8()?;
                    c.take(16)?; // window start and end, ms
                    let mut done = 0;
                    for _ in 0..c.u8()? {
                        c.take(12)?; // group, id, value
                        if left == 0 {
                            (pool, left) = (c.u8()?, 8);
                        }
                        done += u32::from(pool & 1);
                        (pool, left) = (pool >> 1, left - 1);
                    }
                    boards.push(Board { slot, group, done });
                }
                if !c.rest().is_empty() {
                    return None;
                }
                Some(Event::Boards(boards))
            }
            Kind::QuestEntry => {
                c.take(2)?; // `00 00`
                let (group, id, value) = (c.u32()?, c.u32()?, c.u32()?);
                let done = c.u8()? & 1 != 0;
                // ponytail: one sample (a delivery): the 18 bytes after the flags are not decoded,
                // only their count is pinned. Revisit once more deliveries are captured.
                if c.rest().len() != 18 {
                    return None;
                }
                Some(Event::BoardEntry { group, id, value, done })
            }
            Kind::KinahToday => {
                let v = c.u64()?;
                c.take(8)?; // ms timestamp
                if !c.rest().is_empty() {
                    return None;
                }
                Some(Event::KinahToday(v))
            }
            Kind::DutyMissions => {
                // Only the frames with `a == 0` (28 to ~1400 bytes: sent when the missions change or
                // the board is opened) carry the done count. The world-entry list (`a` = 1 or 2,
                // ~1600-1800 bytes) has `b` = 5 even on a day with nothing done, so it is not read.
                // ponytail: everything after `b` is skipped, and the done count is not available
                // until the first update after a login; `a` may be the Attendance state.
                if c.u32()? != 0 {
                    return None;
                }
                Some(Event::DutyMissions(c.u32()?))
            }
            Kind::PassUsed => {
                // [u8 n] n x entry, each ending in a `[u32 == 1]` preceded by a tag byte, except the
                // last entry of a frame with more than one (n = 1 frames carry the tag).
                // ponytail: `a`, the ms and i64 pair, the three sub-lists and the tag are skipped, not
                // interpreted (docs/protocol.md "Not yet known"); decode them when a UI needs them.
                let n = c.u8()?;
                if !(1..=MAX_PASS).contains(&n) {
                    return None;
                }
                let mut v = Vec::with_capacity(usize::from(n));
                for i in 0..n {
                    let id = c.u32()?;
                    c.u32()?; // `a`: equals `used` under the cap, exceeds the cap past it
                    let used = c.u32()?;
                    c.take(16)?; // ms timestamp, i64
                    for width in [4, 5, 8] {
                        let k = usize::from(c.u8()?);
                        c.take(k * width)?;
                    }
                    if n == 1 || i + 1 < n {
                        c.u8()?; // tag
                    }
                    if c.u32()? != 1 {
                        return None;
                    }
                    v.push((id, used));
                }
                if !c.rest().is_empty() {
                    return None;
                }
                Some(Event::PassUsed(v))
            }
            Kind::CountersSnapshot => {
                let count = usize::from(c.u8()?);
                Some(Event::Counters(counter_records(c.rest(), count)?))
            }
            Kind::CounterUpdate => {
                // `[u8 0]`, one `0b 61` record, one trailer byte (`02`/`03`, meaning unknown)
                c.u8()?;
                let (_, rec) = c.rest().split_last()?;
                let [r] = <[CounterRecord; 1]>::try_from(counter_records(rec, 1)?).ok()?;
                Some(Event::Counter(r))
            }
            Kind::PetsSnapshot => {
                let active = c.u32()?;
                let n = c.varint()?;
                let mut levels = Vec::new();
                for _ in 0..n {
                    let id = c.u32()?;
                    c.u32()?; // id again
                    levels.push((id, c.u32()?));
                }
                let m = c.varint()?;
                let mut souls = Vec::new();
                for _ in 0..m {
                    souls.push((c.u32()?, c.u32()?));
                }
                Some(Event::Pets { active, levels, souls })
            }
            Kind::PetSoulsDelta | Kind::PetSoulsTotal => {
                let n = c.u8()?;
                let mut v = Vec::new();
                for _ in 0..n {
                    v.push((c.u32()?, c.u32()?));
                }
                Some(if kind == Kind::PetSoulsDelta { Event::PetSoulsDelta(v) } else { Event::PetSoulsTotal(v) })
            }
            Kind::PetLevel => {
                c.u16()?;
                let id = c.u32()?;
                Some(Event::PetLevel { id, level: c.u32()? })
            }
            Kind::MonsterSpawn => {
                let entity = c.varint()?;
                if let Some(owner) = summon_owner(frame.get(c.i..)?) {
                    return Some(Event::Summon { entity, owner });
                }
                // [3 bytes; bit 0 of the last: a name follows][u32 template][2 bytes; bit 3 of the
                // first: 12 more bytes below][f32 x][f32 y][f32 z][4 bytes][the 12][3 bytes]
                // [varint hp][varint max hp]
                let head = c.take(3)?;
                let template = c.u32()?;
                let moving = c.take(2)?[0] & 8 != 0;
                let (x, y) = (c.cm()?, c.cm()?);
                let mut max_hp = || -> Option<u64> {
                    c.take(if moving { 23 } else { 11 })?;
                    let (hp, max) = (c.varint()?, c.varint()?);
                    (hp <= max).then_some(u64::from(max))
                };
                let max_hp = if head[2] & 1 == 0 { max_hp() } else { None };
                Some(Event::Spawn { entity, template, x, y, max_hp })
            }
            Kind::MonsterMoveShort => {
                let entity = c.varint()?;
                c.take(2)?;
                Some(Event::Pos { entity, x: c.cm()?, y: c.cm()? })
            }
            Kind::MonsterMoveJump => {
                // a flag byte (`03` in every capture) that, unlike `1b 37`/`1c 37`, adds no byte
                let entity = c.varint()?;
                c.u8()?;
                Some(Event::Pos { entity, x: c.cm()?, y: c.cm()? })
            }
            Kind::MonsterMove => {
                // a flag byte; its bit 0 inserts one more byte before the position
                let entity = c.varint()?;
                let flags = c.u8()?;
                c.take(usize::from(flags & 1))?;
                Some(Event::Pos { entity, x: c.cm()?, y: c.cm()? })
            }
            Kind::MonsterGone => Some(Event::Gone { entity: c.varint()? }),
            Kind::PlayerSpawn => {
                // [varint entity][5 bytes][u8 n][name][u32 class], then over 1 KiB not read but the combat power at the tail
                let entity = c.varint()?;
                c.take(5)?;
                let n = usize::from(c.u8()?);
                let name = String::from_utf8(c.take(n)?.to_vec()).ok()?;
                let class_code = c.u32()?;
                Some(Event::PlayerSpawn { entity, name, class_code, combat_power: tail_power(frame, c.i) })
            }
            Kind::SkillPos => {
                // the frame's last 12 bytes are a position (x, y, z); the first entity's only when it is the player
                let entity = c.varint()?;
                let at = frame.len().checked_sub(12)?;
                if at < c.i {
                    return None;
                }
                c.i = at;
                Some(Event::Cast { entity, x: c.cm()?, y: c.cm()? })
            }
            Kind::ZoneLoad => {
                c.take(6)?;
                Some(Event::Teleport { x: c.cm()?, y: c.cm()? })
            }
            Kind::MapEnter => {
                // [u32 load counter][u32 map id][instance and position, not read]
                c.take(4)?;
                Some(Event::MapEnter { map: c.u32()? })
            }
            Kind::Hit => {
                let h = hit_head(&mut c)?;
                // a frame whose target is its attacker is a self effect: a heal unless it restores
                // something else
                if h.target == h.attacker {
                    let heal = Event::Heal { target: h.target, healer: h.attacker, skill: h.skill, amount: h.amount };
                    return (!NOT_HP.contains(&h.effect)).then_some(heal);
                }
                Some(Event::Hit { target: h.target, attacker: h.attacker, skill: h.skill, amount: h.amount, crit: Some(h.crit) })
            }
            Kind::HitTick => {
                // [varint target][u8 fields][varint attacker][varint serial][u32 effect], then by
                // the bits of `fields`: 0 a varint (what is left of a heal over time), 1 the amount,
                // 2 a varint and a u32 (not read), 3 the skill
                let target = c.varint()?;
                let fields = c.u8()?;
                let attacker = c.varint()?;
                if fields & 2 == 0 || fields & !0x0f != 0 {
                    return None; // no amount, or fields not known
                }
                c.varint()?;
                let effect = c.u32()?;
                if fields & 1 != 0 {
                    c.varint()?;
                }
                let amount = c.varint()?;
                if fields & 4 != 0 {
                    c.varint()?;
                    c.take(4)?;
                }
                let skill = if fields & 8 != 0 { Some(c.u32()?) } else { None };
                if target == attacker {
                    // a heal over time (or a restore) of the attacker itself; without a skill the
                    // effect id is the skill times 100 plus the effect's number
                    let heal = Event::Heal { target, healer: attacker, skill: skill.unwrap_or(effect / 100), amount };
                    return (!NOT_HP.contains(&effect)).then_some(heal);
                }
                // no crit flag in a tick: the crit rate leaves ticks out (docs/protocol.md "Combat")
                Some(Event::Hit { target, attacker, skill: skill?, amount, crit: None })
            }
            Kind::Hp => {
                // [varint entity][u8 mask]; bit 0: [u8 n][n x (u8 stat, u32)] (MP and others, not
                // read); bit 1: [u8 n][n x (u8 stat, u64)], stat 0 the HP and 7 the max HP
                let entity = c.varint()?;
                let mask = c.u8()?;
                if mask & !3 != 0 {
                    return None;
                }
                if mask & 1 != 0 {
                    let n = c.u8()?;
                    c.take(5 * usize::from(n))?;
                }
                let (mut hp, mut max) = (None, None);
                if mask & 2 != 0 {
                    for _ in 0..c.u8()? {
                        match (c.u8()?, c.u64()?) {
                            (0, v) => hp = Some(v),
                            (7, v) => max = Some(v),
                            _ => {}
                        }
                    }
                }
                (c.rest().is_empty() && (hp.is_some() || max.is_some())).then_some(Event::Hp { entity, hp, max })
            }
            Kind::Stagger => {
                // [varint entity][u8 03][u8 state 1][u32 max][u32 value][u8 02], or [00][u8 state 3][u8 02]
                let entity = c.varint()?;
                let (fields, state) = (c.u8()?, c.u8()?);
                let (value, max) = match fields {
                    0 => (None, None),
                    3 => {
                        let max = c.u32()?;
                        (Some(c.u32()?), Some(max))
                    }
                    _ => return None,
                };
                c.u8()?;
                c.rest().is_empty().then_some(Event::Stagger { entity, state, value, max })
            }
        }
    }
}

/// The fields of a `04 38` hit up to its amount; `None` without `flags & 4` (nothing was dealt or
/// healed; the amount is then almost always 1 or 2, 12 larger ones seen).
struct HitHead {
    target: u32,
    flags: u8,
    /// The byte after the flags: bit 2 when a drain heal ends the frame.
    extra: u8,
    attacker: u32,
    skill: u32,
    crit: bool,
    /// The effect id: the skill (without its variant) times 100 plus the effect's number.
    effect: u32,
    amount: u32,
}

fn hit_head(c: &mut Cur) -> Option<HitHead> {
    let target = c.varint()?;
    let flags = c.u8()?;
    let extra = c.u8()?;
    let attacker = c.varint()?;
    if flags & 4 == 0 {
        return None;
    }
    let skill = c.u32()?;
    c.u8()?; // serial
    let crit = c.u8()? & 1 != 0; // 2, or 3 on a critical hit
    if flags & 2 != 0 {
        // `[u8][varint][u8]`: the varint is 0 except on some hits on a player (bit 5 of the u8)
        c.u8()?;
        c.varint()?;
        c.u8()?;
    }
    let effect = c.u32()?;
    c.take(6)?; // a u32 and a 2-byte tag
    Some(HitHead { target, flags, extra, attacker, skill, crit, effect, amount: c.varint()? })
}

/// The owner of a summon in a `41 36` spawn, `rest` being the frame after the entity. The spawn
/// ends in `[varint][01 02][f32 x, y, z][07 02 06][u32 owner]`; monsters and players carry other
/// bytes there. The varint is the summon's own entity for a Ranger's, the owner's for a
/// Spiritmaster's spirits (`292xxxx` templates), so it is not compared.
/// ponytail: found by searching for that shape (a named spawn's fields before it vary in length and
/// are not decoded); decode the spawn's tail field by field if another owned kind turns up.
fn summon_owner(rest: &[u8]) -> Option<u32> {
    rest.windows(21)
        .find(|w| w[..2] == [1, 2] && w[14..17] == [7, 2, 6])
        .and_then(|w| Some(u32::from_le_bytes(w[17..].try_into().ok()?)))
}

/// The combat power in the tail of a `45 36`: a u32 at `len - 9`, or at `len - 10` when the frame has
/// one more trailing byte; the u32 before it is a small count (< 64) or the item level (800 to 2000).
/// Some frames carry a one-byte field (`01`) between that u32 and the power: then the u32 at
/// `len - 10` is the power x 256 + 1, so a value over 2^24 is rejected and the power is read at
/// `len - 9` after the u32 at `len - 14`. `from`: where the unread middle starts.
/// ponytail: found on two captures (4 values checked against the party window, 2 against another
/// meter's log); the extra bytes are not understood, and a one-byte field with a power under 65,536
/// is still misread. Re-check this rule after a game patch.
fn tail_power(frame: &[u8], from: usize) -> Option<u32> {
    let u32_at = |at: usize| Some(u32::from_le_bytes(frame.get(at..at + 4)?.try_into().ok()?));
    // (power, the u32 before it), as bytes back from the end
    [(9, 13), (10, 14), (9, 14)].into_iter().find_map(|(power, before)| {
        let before = u32_at(frame.len().checked_sub(before).filter(|&b| b >= from)?)?;
        let power = u32_at(frame.len().checked_sub(power)?)?;
        ((before < 64 || (800..=2000).contains(&before)) && power < 1 << 24).then_some(power)
    })
}

/// Bounds-checked little-endian cursor.
struct Cur<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Cur<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.i..self.i.checked_add(n)?)?;
        self.i += n;
        Some(s)
    }

    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    /// A position component: an f32 on the wire, returned as an integer (hundredths of a unit).
    fn cm(&mut self) -> Option<i32> {
        let v = f32::from_le_bytes(self.take(4)?.try_into().ok()?);
        (v.is_finite() && v.abs() < 1e8).then(|| v.round() as i32)
    }

    /// LEB128 that fits a u32: at most 5 bytes, and the 5th may only carry the top 4 bits.
    fn varint(&mut self) -> Option<u32> {
        let mut v = 0u32;
        for n in 0..5u32 {
            let c = self.u8()?;
            if n == 4 && c & 0x7f > 0x0f {
                return None;
            }
            v |= u32::from(c & 0x7f) << (7 * n);
            if c & 0x80 == 0 {
                return Some(v);
            }
        }
        None
    }

    fn rest(&self) -> &'a [u8] {
        &self.b[self.i.min(self.b.len())..]
    }
}

/// Walk the records of a `0b 61` frame (after its count byte). Each is self-describing:
/// `[u8 mask][u32 id][fields]`, fields in bit order and only when the bit is set: bit0 and bit1 a
/// u64 ms each (time left; refill period), bit2 a varint `left`, bit3 a varint `charged`. Ids
/// ascend below `MAX_ID` and the records tile `buf` exactly, else `None`.
/// Evidence: docs/protocol.md ("`0b 61` records").
fn counter_records(buf: &[u8], count: usize) -> Option<Vec<CounterRecord>> {
    if count == 0 || buf.len() > MAX_SNAPSHOT || count > buf.len() / 5 {
        return None;
    }
    let mut c = Cur { b: buf, i: 0 };
    let mut out = Vec::with_capacity(count);
    let mut prev = None;
    for _ in 0..count {
        let mask = c.u8()?;
        let id = c.u32()?;
        if mask > 0x0f || id >= MAX_ID || prev.is_some_and(|p| p >= id) {
            return None;
        }
        prev = Some(id);
        let ms = if mask & 1 != 0 { Some(c.u64()?) } else { None };
        if mask & 2 != 0 {
            // ponytail: the refill period is dropped; carry it when a UI shows time to next refill
            c.u64()?;
        }
        let left = if mask & 4 != 0 { Some(c.varint()?) } else { None };
        if mask & 8 != 0 {
            // ponytail: the wire `charged` varint is carried by no UI yet, game meaning unknown; carry
            // it when a UI needs it
            c.varint()?;
        }
        out.push(match (left, ms) {
            (Some(v), _) => CounterRecord { id, value: u64::from(v), ms: false },
            (None, Some(t)) => CounterRecord { id, value: t, ms: true },
            (None, None) => CounterRecord { id, value: 0, ms: false },
        });
    }
    c.rest().is_empty().then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ops() -> Opcodes {
        Opcodes::embedded("default").unwrap()
    }

    fn u32s(vals: &[u32]) -> Vec<u8> {
        vals.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    fn f32s(vals: &[f32]) -> Vec<u8> {
        vals.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    #[test]
    fn identity() {
        let mut f = vec![0x33, 0x36, 0xf2, 0x7f]; // entity 16370, then 5 bytes
        f.extend([0u8; 5]);
        f.push(8);
        f.extend(b"Testname");
        f.extend(7u16.to_le_bytes());
        f.extend(u32s(&[3]));
        f.push(1);
        f.extend(u32s(&[10, 9, 9, 10]));
        assert_eq!(
            ops().parse(&f),
            Some(Event::Identity {
                name: "Testname".into(), server: 7, class_code: 3, level: 10, item_level: 9, entity: 16370,
            })
        );
        assert_eq!(ops().parse(&f[..14]), None); // truncated inside the name
    }

    /// Entity 60651 as a 3-byte varint.
    const ENT: [u8; 3] = [0xeb, 0xd9, 0x03];

    #[test]
    fn monster_spawn() {
        let mut f = vec![0x41, 0x36];
        f.extend(ENT);
        f.extend([0x04, 0x20, 0x00]);
        f.extend(u32s(&[2_100_025]));
        f.extend([0x00, 0x02]);
        f.extend(f32s(&[-9942.6, -79079.4, 5000.0]));
        let want = Event::Spawn { entity: 60651, template: 2_100_025, x: -9943, y: -79079, max_hp: None };
        assert_eq!(ops().parse(&f), Some(want));
        for n in 0..f.len() - 4 {
            assert_eq!(ops().parse(&f[..n]), None, "prefix {n}"); // z is not needed
        }
    }

    #[test]
    fn monster_spawn_carries_hp_and_max_hp_after_the_position() {
        // [3 bytes][template][2 bytes][x y z][4 bytes][12 bytes when bit 3][3 bytes][varint hp][varint max]
        let spawn = |head: [u8; 3], flags: u8, hp: &[u8], max: &[u8]| {
            let mut f = vec![0x41, 0x36];
            f.extend(ENT);
            f.extend(head);
            f.extend(u32s(&[2_980_257]));
            f.extend([flags, 0x02]);
            f.extend(f32s(&[100.0, 200.0, 72.0, 90.0]));
            if flags & 8 != 0 {
                f.extend(f32s(&[1.0, 0.5, 0.0]));
            }
            f.extend([0x00, 0x40, 0x01]);
            f.extend(hp);
            f.extend(max);
            f.extend(u32s(&[100, 100])); // more, not read
            f
        };
        let with = |max_hp| Some(Event::Spawn { entity: 60651, template: 2_980_257, x: 100, y: 200, max_hp });
        let full = [0x80, 0xeb, 0x83, 0x01]; // 2,160,000
        let f = spawn([0x04, 0x20, 0x00], 0x00, &full, &full);
        assert_eq!(ops().parse(&f), with(Some(2_160_000)));
        let cut = f.len() - 9; // inside the max
        assert_eq!(ops().parse(&f[..cut]), with(None));
        // bit 3: 12 more bytes; a damaged monster (hp below max)
        assert_eq!(ops().parse(&spawn([0x1d, 0x10, 0x00], 0x08, &[0xe8, 0x07], &[0xdc, 0x0b])), with(Some(1500)));
        // hp above max is not this layout; a named spawn's HP is not read
        assert_eq!(ops().parse(&spawn([0x04, 0x20, 0x00], 0x00, &[0xdd, 0x0b], &[0xdc, 0x0b])), with(None));
        assert_eq!(ops().parse(&spawn([0x1c, 0x00, 0x01], 0x00, &full, &full)), with(None));
        assert_eq!(with(Some(1500)).unwrap().to_json()["max_hp"], 1500);
    }

    #[test]
    fn stagger_frames_carry_the_gauge_or_its_break() {
        let mut f = vec![0x05, 0xe0];
        f.extend(ENT);
        f.extend([0x03, 0x01]);
        f.extend(u32s(&[240, 213]));
        f.push(0x02);
        let want = Event::Stagger { entity: 60651, state: 1, value: Some(213), max: Some(240) };
        assert_eq!(ops().parse(&f), Some(want.clone()));
        for n in 0..f.len() {
            assert_eq!(ops().parse(&f[..n]), None, "prefix {n}");
        }
        let mut t = f.clone();
        t.push(0);
        assert_eq!(ops().parse(&t), None); // trailing byte
        let mut broken = vec![0x05, 0xe0];
        broken.extend(ENT);
        broken.extend([0x00, 0x03, 0x02]);
        assert_eq!(ops().parse(&broken), Some(Event::Stagger { entity: 60651, state: 3, value: None, max: None }));
        broken[5] = 0x01; // fields not seen
        assert_eq!(ops().parse(&broken), None);
        assert_eq!(want.to_json(), json!({"type": "stagger", "entity": 60651, "state": 1, "value": 213, "max": 240}));
    }

    #[test]
    fn monster_moves_read_the_position_after_the_flag_byte_and_its_optional_extra_byte() {
        let pos = f32s(&[81_000.4, 73_200.0, 1.0]);
        let want = Some(Event::Pos { entity: 60651, x: 81_000, y: 73_200 });
        for (op, head, extra) in [
            (0x1c, vec![0x04], 0),
            (0x1b, vec![0x04], 0),
            (0x1c, vec![0x05, 0x03], 1), // bit 0 set: one more byte
            (0x1b, vec![0x01, 0x03], 1),
        ] {
            let mut f = vec![op, 0x37];
            f.extend(ENT);
            f.extend(&head);
            f.extend(&pos);
            assert_eq!(head.len(), 1 + extra);
            assert_eq!(ops().parse(&f), want, "{op:#x} {head:?}");
            assert_eq!(ops().parse(&f[..f.len() - 8]), None); // y missing
        }
        let mut short = vec![0x1a, 0x37];
        short.extend(ENT);
        short.extend([0x00, 0x00]); // always two bytes before the position
        short.extend(&pos);
        assert_eq!(ops().parse(&short), want);
        let mut jump = vec![0x3c, 0x37];
        jump.extend(ENT);
        jump.push(0x03); // one flag byte, never an extra one
        jump.extend(&pos);
        assert_eq!(ops().parse(&jump), want);
    }

    #[test]
    fn monster_gone_and_zone_load() {
        let mut f = vec![0x42, 0x36];
        f.extend(ENT);
        f.extend([0x00, 0x03]);
        assert_eq!(ops().parse(&f), Some(Event::Gone { entity: 60651 }));
        assert_eq!(ops().parse(&f[..2]), None);
        let mut z = vec![0x23, 0x36];
        z.extend([0u8; 6]);
        z.extend(f32s(&[-8504.4, -79175.5, 1.0]));
        z.extend([5, 0, 0, 0, 1, 2]);
        assert_eq!(ops().parse(&z), Some(Event::Teleport { x: -8504, y: -79176 }));
        assert_eq!(ops().parse(&z[..15]), None); // y is cut
    }

    /// A `04 38` direct hit by entity 4990 (`fe 26`) on `ENT`: flags, 0, attacker, skill, serial,
    /// `02` (`03` when critical), the `[u8][varint][u8]` field only when `flags & 2`, time, ten bytes, amount, trailer.
    fn hit_frame(flags: u8, amount: &[u8]) -> Vec<u8> {
        crit_frame(flags, amount, false)
    }

    fn crit_frame(flags: u8, amount: &[u8], crit: bool) -> Vec<u8> {
        let mut f = vec![0x04, 0x38];
        f.extend(ENT);
        f.extend([flags, 0x00, 0xfe, 0x26]);
        f.extend(u32s(&[14_090_130]));
        f.extend([0x40, if crit { 0x03 } else { 0x02 }]);
        if flags & 2 != 0 {
            f.extend([0x08, 0x00, 0x02]);
        }
        f.extend(u32s(&[1_400_000_000]));
        f.extend([1, 0, 0, 0, 0xca, 0x61]);
        f.extend(amount);
        f.extend([0x01, 0x00]);
        f
    }

    #[test]
    fn direct_hits_carry_target_attacker_skill_and_amount() {
        let want = Some(Event::Hit { target: 60651, attacker: 4990, skill: 14_090_130, amount: 15_102, crit: Some(false) });
        let crit = Some(Event::Hit { target: 60651, attacker: 4990, skill: 14_090_130, amount: 15_102, crit: Some(true) });
        for flags in [0x26, 0x06, 0x04, 0x24] {
            assert_eq!(ops().parse(&hit_frame(flags, &[0xfe, 0x75])), want, "flags {flags:#x}");
            assert_eq!(ops().parse(&crit_frame(flags, &[0xfe, 0x75], true)), crit, "flags {flags:#x}");
        }
        // without bit 2 nothing was dealt (the amount is almost always 1 or 2, 12 larger ones seen)
        for flags in [0x00, 0x02] {
            assert_eq!(ops().parse(&hit_frame(flags, &[0x01])), None, "flags {flags:#x}");
        }
        // a target that is its own attacker is a self effect: a heal, unless it restores another resource
        let mut f = hit_frame(0x04, &[0xfe, 0x75]);
        f.splice(2..5, [0xfe, 0x26]);
        assert_eq!(ops().parse(&f), Some(Event::Heal { target: 4990, healer: 4990, skill: 14_090_130, amount: 15_102 }));
        f.splice(14..18, u32s(&[19_009_112])); // the effect id: the MP part of an HP and MP restore
        assert_eq!(ops().parse(&f), None);
        // a multi-hit list after the amount is not read
        let mut f = hit_frame(0x26, &[0xfe, 0x75]);
        f.truncate(f.len() - 2);
        f.extend([4, 0xe6, 0x0b, 0xe6, 0x0b, 0xe6, 0x0b, 0xe6, 0x0b, 2, 0]);
        assert_eq!(ops().parse(&f), want);
        // the field after the crit byte is `[u8][varint][u8]`: two bytes in its middle on some hits
        let mut f = hit_frame(0x26, &[0xfe, 0x75]);
        assert_eq!(f[15..18], [0x08, 0x00, 0x02]);
        f.splice(15..18, [0x20, 0xc7, 0x01, 0x02]);
        assert_eq!(ops().parse(&f), want);
        for n in 0..f.len() - 2 {
            assert_eq!(ops().parse(&f[..n]), None, "long field, prefix {n}");
        }
        let f = hit_frame(0x26, &[0xfe, 0x75]);
        for n in 0..f.len() - 2 {
            assert_eq!(ops().parse(&f[..n]), None, "prefix {n}"); // the amount ends at len - 2
        }
    }

    #[test]
    fn damage_ticks_put_the_skill_after_the_amount() {
        let mut f = vec![0x05, 0x38];
        f.extend(ENT);
        f.push(0x0a);
        f.extend([0xfe, 0x26]);
        f.extend([0x06, 0x56, 0x99, 0xbe, 0x53]);
        f.extend([0x8b, 0x02]); // 267
        f.extend(u32s(&[14_050_340]));
        assert_eq!(ops().parse(&f), Some(Event::Hit { target: 60651, attacker: 4990, skill: 14_050_340, amount: 267, crit: None }));
        assert_eq!(ops().parse(&f[..f.len() - 1]), None); // the skill is cut
        // a serial past 127 takes two bytes
        let mut long = f.clone();
        long.splice(8..9, [0xf7, 0x05]);
        assert_eq!(ops().parse(&long), ops().parse(&f));
        // bit 2: a varint and a u32 between the amount and the skill
        let mut more = f.clone();
        more[5] = 0x0e;
        more.splice(15..15, [0x01, 0xfb, 0xcc, 0x54, 0x07]);
        assert_eq!(ops().parse(&more), ops().parse(&f));
        f.splice(2..5, [0xfe, 0x26]); // the attacker ticks itself: a heal over time
        assert_eq!(ops().parse(&f), Some(Event::Heal { target: 4990, healer: 4990, skill: 14_050_340, amount: 267 }));
    }

    /// A `05 38` tick of 4990 on itself with `fields`, effect id `effect` and `amount`; bit 0 adds
    /// what is left of the heal over time, bit 3 a skill.
    fn self_tick(fields: u8, effect: u32, amount: &[u8]) -> Vec<u8> {
        let mut f = vec![0x05, 0x38, 0xfe, 0x26, fields, 0xfe, 0x26, 0x9f, 0x01];
        f.extend(u32s(&[effect]));
        if fields & 1 != 0 {
            f.extend([0x92, 0x0d]);
        }
        f.extend(amount);
        if fields & 8 != 0 {
            f.extend(u32s(&[2_011_101]));
        }
        f
    }

    #[test]
    fn self_ticks_are_heals_after_what_is_left_and_take_the_effects_skill_without_one() {
        let heal = |skill: u32| Some(Event::Heal { target: 4990, healer: 4990, skill, amount: 420 });
        assert_eq!(ops().parse(&self_tick(0x0b, 201_110_111, &[0xa4, 0x03])), heal(2_011_101));
        assert_eq!(ops().parse(&self_tick(0x0a, 201_110_112, &[0xa4, 0x03])), heal(2_011_101));
        assert_eq!(ops().parse(&self_tick(0x02, 190_000_131, &[0xa4, 0x03])), heal(1_900_001));
        // the restore's other effects are not HP; no amount (bit 1 clear), or unknown fields: nothing
        assert_eq!(ops().parse(&self_tick(0x02, 190_000_161, &[0xa4, 0x03])), None);
        assert_eq!(ops().parse(&self_tick(0x09, 201_110_111, &[])), None);
        assert_eq!(ops().parse(&self_tick(0x30, 201_110_111, &[0xa4, 0x03])), None);
        let f = self_tick(0x0b, 201_110_111, &[0xa4, 0x03]);
        for n in 0..f.len() {
            assert_eq!(ops().parse(&f[..n]), None, "prefix {n}");
        }
    }

    #[test]
    fn a_draining_hit_also_heals_its_attacker_by_the_last_varint() {
        let hit = Event::Hit { target: 60651, attacker: 4990, skill: 14_090_130, amount: 15_102, crit: Some(false) };
        let heal = Event::Heal { target: 4990, healer: 4990, skill: 14_090_130, amount: 294 };
        let drain = |flags: u8, list: &[u8]| {
            let mut f = hit_frame(flags, &[0xfe, 0x75]);
            f[6] = 0x04; // the byte after the flags: bit 2
            f.truncate(f.len() - 2);
            f.extend(list);
            f.extend([0x01, 0x00, 0xa6, 0x02]);
            f
        };
        for (flags, list) in [(0x06, &[][..]), (0x26, &[2, 0xe6, 0x0b, 0xe6, 0x0b][..])] {
            let f = drain(flags, list);
            assert_eq!(ops().parse_all(&f).collect::<Vec<_>>(), [hit.clone(), heal.clone()], "flags {flags:#x}");
            assert_eq!(ops().parse_all(&f[..f.len() - 1]).collect::<Vec<_>>(), std::slice::from_ref(&hit)); // the heal is cut
        }
        // without bit 2, or with fields not decoded (0x40), the hit alone
        assert_eq!(ops().parse_all(&hit_frame(0x06, &[0xfe, 0x75])).collect::<Vec<_>>(), std::slice::from_ref(&hit));
        assert_eq!(ops().parse_all(&drain(0x46, &[])).collect::<Vec<_>>(), std::slice::from_ref(&hit));
        assert_eq!(heal.to_json(), json!({"type": "heal", "target": 4990, "healer": 4990, "skill": 14_090_130, "amount": 294}));
    }

    #[test]
    fn hp_frames_carry_hp_and_max_after_the_other_stats() {
        let mut f = vec![0x00, 0x8d, 0xfe, 0x26, 0x03, 0x01, 0x01];
        f.extend(u32s(&[3759])); // MP, not read
        f.extend([0x02, 0x00]);
        f.extend(18_372u64.to_le_bytes());
        f.push(0x07);
        f.extend(18_400u64.to_le_bytes());
        let want = Event::Hp { entity: 4990, hp: Some(18_372), max: Some(18_400) };
        assert_eq!(ops().parse(&f), Some(want.clone()));
        for n in 0..f.len() {
            assert_eq!(ops().parse(&f[..n]), None, "prefix {n}");
        }
        let mut t = f.clone();
        t.push(0);
        assert_eq!(ops().parse(&t), None); // trailing byte
        // HP alone; MP alone is no event; an unknown mask bit is not read
        let mut hp = vec![0x00, 0x8d, 0xfe, 0x26, 0x02, 0x01, 0x00];
        hp.extend(17_501u64.to_le_bytes());
        assert_eq!(ops().parse(&hp), Some(Event::Hp { entity: 4990, hp: Some(17_501), max: None }));
        assert_eq!(ops().parse(&f[..11]), None);
        let mut mp = f[..11].to_vec();
        mp[4] = 0x01;
        assert_eq!(ops().parse(&mp), None);
        hp[4] = 0x06;
        assert_eq!(ops().parse(&hp), None);
        assert_eq!(want.to_json(), json!({"type": "hp", "entity": 4990, "hp": 18_372, "max": 18_400}));
    }

    /// A `41 36` spawn of entity `ENT` named `n` bytes long, whose tail repeats the entity as
    /// `[01 02][x, y, z][07 02 06][u32 owner]` (a summon) or with monster bytes in place of `07 02 06`.
    fn named_spawn(n: usize, tag: [u8; 3]) -> Vec<u8> {
        let mut f = vec![0x41, 0x36];
        f.extend(ENT);
        f.extend([0x5f, 0x00, 0x01, n as u8]);
        f.extend(std::iter::repeat_n(b'a', n));
        f.extend(u32s(&[2_920_626]));
        f.extend([0x00, 0x02]);
        f.extend(f32s(&[225.0, 425.0, 72.0]));
        f.extend([0x07, 0x38, 0x6c, 0x43, 0xc4, 0x8f, 0x01, 0x00, 0x00]);
        f.extend(ENT);
        f.extend([0x01, 0x02]);
        f.extend(f32s(&[225.0, 425.0, 72.0]));
        f.extend(tag);
        f.extend(u32s(&[4990]));
        f.extend([0x02, 0x00, 0x00, 0x00]);
        f
    }

    #[test]
    fn a_spawn_that_names_an_owner_is_a_summon() {
        for n in [3, 12] {
            let f = named_spawn(n, [0x07, 0x02, 0x06]);
            assert_eq!(ops().parse(&f), Some(Event::Summon { entity: 60651, owner: 4990 }), "name {n}");
            // owner cut: not a summon
            assert!(!matches!(ops().parse(&f[..f.len() - 5]), Some(Event::Summon { .. })));
        }
        // another tag there is not an owner
        assert!(!matches!(ops().parse(&named_spawn(3, [0x11, 0x02, 0x84])), Some(Event::Summon { .. })));
        // a spirit's tail repeats the owner's entity, not its own: still a summon
        let mut spirit = named_spawn(3, [0x07, 0x02, 0x06]);
        let at = spirit.len() - 25 - ENT.len();
        assert_eq!(spirit[at..at + ENT.len()], ENT);
        spirit[at..at + ENT.len()].copy_from_slice(&[0xce, 0x4a, 0x00][..ENT.len()]);
        assert_eq!(ops().parse(&spirit), Some(Event::Summon { entity: 60651, owner: 4990 }));
        assert_eq!(Event::Summon { entity: 1, owner: 2 }.to_json(), json!({"type": "summon", "entity": 1, "owner": 2}));
    }

    #[test]
    fn a_player_spawn_carries_entity_name_and_class_and_47_36_is_gone() {
        let mut f = vec![0x45, 0x36, 0xfe, 0x26]; // entity 4990
        f.extend([0x01, 0x20, 0xa4, 0x00, 0x07, 8]);
        f.extend(b"Testname");
        f.extend(u32s(&[25]));
        f.extend([0x01, 0x02, 0x80, 0x12]); // the rest is not read
        let want = Event::PlayerSpawn { entity: 4990, name: "Testname".into(), class_code: 25, combat_power: None };
        assert_eq!(ops().parse(&f), Some(want.clone()));
        for n in 0..f.len() - 4 {
            assert_eq!(ops().parse(&f[..n]), None, "prefix {n}"); // the class ends 4 bytes early
        }
        assert_eq!(
            want.to_json(),
            json!({"type": "player_spawn", "entity": 4990, "name": "Testname", "class_code": 25, "combat_power": null})
        );
        assert_eq!(ops().parse(&[0x47, 0x36, 0xfe, 0x26, 0x01, 0x00]), Some(Event::Gone { entity: 4990 }));
    }

    #[test]
    fn a_player_spawn_tail_carries_the_combat_power() {
        let head = |tail: &[u8]| {
            let mut f = vec![0x45, 0x36, 0xfe, 0x26, 0x01, 0x20, 0xa4, 0x00, 0x07, 8];
            f.extend(b"Testname");
            f.extend(u32s(&[25]));
            f.extend([0xffu8; 20]); // the unread middle
            f.extend(tail);
            f
        };
        let power = |f: &[u8]| match ops().parse(f) {
            Some(Event::PlayerSpawn { combat_power, .. }) => combat_power,
            e => panic!("{e:?}"),
        };
        // [item level][count][power][5 bytes]
        let mut tail = u32s(&[1204, 19, 56_070]);
        tail.extend([0; 5]);
        assert_eq!(power(&head(&tail)), Some(56_070));
        // one more trailing byte: the power moves to len - 10
        tail.push(1);
        assert_eq!(power(&head(&tail)), Some(56_070));
        // no count field (a low level): [item level][power][5 bytes]
        let mut low = u32s(&[1034, 8_124]);
        low.extend([0; 5]);
        assert_eq!(power(&head(&low)), Some(8_124));
        // a one-byte field (`01`) between the count and the power: not the power x 256 + 1
        let mut flagged = u32s(&[1204, 19]);
        flagged.push(1);
        flagged.extend(u32s(&[157_450]));
        flagged.extend([0; 5]);
        assert_eq!(power(&head(&flagged)), Some(157_450));
        // a tail that does not fit
        assert_eq!(power(&head(&[7; 9])), None);
    }

    #[test]
    fn hit_json_is_raw_ids() {
        let e = Event::Hit { target: 1, attacker: 2, skill: 3, amount: 4, crit: Some(true) };
        assert_eq!(e.to_json(), json!({"type": "hit", "target": 1, "attacker": 2, "skill": 3, "amount": 4, "crit": true}));
        let e = Event::Hit { target: 1, attacker: 2, skill: 3, amount: 4, crit: None };
        assert_eq!(e.to_json()["crit"], Value::Null);
    }

    #[test]
    fn map_enter_carries_the_map_id_after_a_load_counter() {
        let mut f = vec![0x21, 0x36];
        f.extend(u32s(&[3, 1010, 14_691, 0]));
        f.extend(f32s(&[-8504.4, -79175.5, 1.0]));
        assert_eq!(ops().parse(&f), Some(Event::MapEnter { map: 1010 }));
        assert_eq!(ops().parse(&f[..9]), None); // the id is cut
    }

    #[test]
    fn skill_frames_end_in_a_position() {
        for op in [0x2a, 0x2b] {
            let mut f = vec![op, 0x38];
            f.extend(ENT);
            f.extend([7u8; 20]); // varied middle, including a second entity
            f.extend(f32s(&[81_000.0, 73_200.0, 1.0]));
            assert_eq!(ops().parse(&f), Some(Event::Cast { entity: 60651, x: 81_000, y: 73_200 }));
        }
        let mut f = vec![0x2a, 0x38];
        f.extend(ENT);
        f.extend(f32s(&[1.0, 2.0])); // too short to hold the entity and a position
        assert_eq!(ops().parse(&f), None);
        let mut nan = vec![0x2a, 0x38];
        nan.extend(ENT);
        nan.extend(f32s(&[f32::NAN, 2.0, 1.0]));
        assert_eq!(ops().parse(&nan), None);
    }

    #[test]
    fn combat_power() {
        let mut f = vec![0x56, 0x36, 1, 0, 0, 0, 0, 0, 0, 0];
        f.extend(u32s(&[123_456, 0]));
        assert_eq!(ops().parse(&f), Some(Event::CombatPower(123_456)));
        assert_eq!(ops().parse(&f[..12]), None);
    }

    #[test]
    fn item_level() {
        let mut f = vec![0x1d, 0x56];
        f.extend(u32s(&[1500, 1500]));
        assert_eq!(ops().parse(&f), Some(Event::ItemLevel(1500)));
        assert_eq!(ops().parse(&f[..9]), None);
        f.push(0);
        assert_eq!(ops().parse(&f), None); // trailing byte
    }

    /// `boards`: `(group, slot, done flags)`; entries get ids 1.. and the flags are packed into one
    /// LSB-first pool across the whole frame, a byte inserted whenever the pool runs dry.
    fn boards_frame(boards: &[(u32, u8, &[bool])]) -> Vec<u8> {
        let mut f = vec![0x56, 0xe2, boards.len() as u8];
        let (mut at, mut left) = (0, 0u8);
        for (group, slot, flags) in boards {
            f.extend(group.to_le_bytes());
            f.push(*slot);
            f.extend([0u8; 16]);
            f.push(flags.len() as u8);
            for (n, flag) in flags.iter().enumerate() {
                f.extend(u32s(&[*group, n as u32 + 1, 7]));
                if left == 0 {
                    f.push(0);
                    (at, left) = (f.len() - 1, 8);
                }
                f[at] |= u8::from(*flag) << (8 - left);
                left -= 1;
            }
        }
        f
    }

    #[test]
    fn quest_boards() {
        // 12 entries in slot 1 (flags straddle two bytes), 3 in slot 2 (the pool carries over)
        let a = [true, false, false, true, true, false, false, false, false, true, false, true];
        let b = [false, true, true];
        let f = boards_frame(&[(10, 1, &a), (20, 2, &b), (30, 3, &[])]);
        assert_eq!(
            ops().parse(&f),
            Some(Event::Boards(vec![
                Board { slot: 1, group: 10, done: 5 },
                Board { slot: 2, group: 20, done: 2 },
                Board { slot: 3, group: 30, done: 0 },
            ]))
        );
        assert_eq!(f.len(), 2 + 1 + 3 * 22 + 15 * 12 + 2); // plus two flag bytes: after entries 1 and 9
        for n in 0..f.len() {
            assert_eq!(ops().parse(&f[..n]), None, "prefix {n}");
        }
        let mut t = f.clone();
        t.push(0);
        assert_eq!(ops().parse(&t), None); // trailing byte
    }

    #[test]
    fn quest_entry() {
        let mut f = vec![0x3e, 0xe2, 0, 0];
        f.extend(u32s(&[10, 4, 7]));
        f.push(0b11); // bit 0 = done
        f.extend([0u8; 18]);
        assert_eq!(ops().parse(&f), Some(Event::BoardEntry { group: 10, id: 4, value: 7, done: true }));
        f[16] = 0b10;
        assert_eq!(ops().parse(&f), Some(Event::BoardEntry { group: 10, id: 4, value: 7, done: false }));
        assert_eq!(ops().parse(&f[..f.len() - 1]), None);
        assert_eq!(ops().parse(&f[..16]), None);
        f.push(0);
        assert_eq!(ops().parse(&f), None); // trailing byte
    }

    #[test]
    fn kinah_today_from_the_snapshot_and_the_update() {
        for op in [[0xa5, 0xff], [0xa6, 0xff]] {
            let mut f = op.to_vec();
            f.extend(4_321u64.to_le_bytes());
            f.extend(1_700_000_000_000u64.to_le_bytes());
            assert_eq!(ops().parse(&f), Some(Event::KinahToday(4_321)));
            assert_eq!(ops().parse(&f[..17]), None);
            f.push(0);
            assert_eq!(ops().parse(&f), None); // trailing byte
        }
    }

    #[test]
    fn duty_missions_takes_the_second_u32_of_an_update_whatever_follows() {
        let mut f = vec![0x1c, 0x8d];
        f.extend(u32s(&[0, 3]));
        assert_eq!(ops().parse(&f), Some(Event::DutyMissions(3))); // header only
        f.extend([7u8; 1400]); // the list that follows is not read
        assert_eq!(ops().parse(&f), Some(Event::DutyMissions(3)));
        assert_eq!(ops().parse(&f[..9]), None);
        assert_eq!(ops().parse(&f[..5]), None);
        f[2] = 2; // the world-entry list: its `b` is not a done count
        assert_eq!(ops().parse(&f), None);
    }

    /// One `24 e3` entry body after `used`: ms, i64, then the three sub-lists with `k` entries.
    fn pass_entry(id: u32, a: u32, used: u32, k: [u8; 3]) -> Vec<u8> {
        let mut e = u32s(&[id, a, used]);
        e.extend(1_700_000_000_000u64.to_le_bytes());
        e.extend((-1i64).to_le_bytes());
        e.push(k[0]);
        e.extend(vec![7u8; usize::from(k[0]) * 4]);
        e.push(k[1]);
        e.extend(vec![7u8; usize::from(k[1]) * 5]);
        e.push(k[2]);
        e.extend(vec![7u8; usize::from(k[2]) * 8]);
        e
    }

    fn pass_frame(entries: &[Vec<u8>]) -> Vec<u8> {
        let mut f = vec![0x24, 0xe3, entries.len() as u8];
        for (i, e) in entries.iter().enumerate() {
            f.extend(e);
            if entries.len() == 1 || i + 1 < entries.len() {
                f.push(0x0a); // tag
            }
            f.extend(u32s(&[1]));
        }
        f
    }

    #[test]
    fn pass_used_takes_the_third_u32_of_each_entry() {
        let one = pass_frame(&[pass_entry(10044, 777, 12_000, [1, 0, 1])]);
        assert_eq!(ops().parse(&one), Some(Event::PassUsed(vec![(10044, 12_000)])));
        let two = pass_frame(&[pass_entry(10044, 60_000, 50_000, [2, 2, 2]), pass_entry(10045, 5, 5, [0, 0, 0])]);
        assert_eq!(ops().parse(&two), Some(Event::PassUsed(vec![(10044, 50_000), (10045, 5)])));
        for f in [one, two] {
            for n in 0..f.len() {
                assert_eq!(ops().parse(&f[..n]), None, "prefix {n}");
            }
            let mut t = f.clone();
            t.push(0);
            assert_eq!(ops().parse(&t), None); // trailing byte
        }
    }

    #[test]
    fn pass_used_rejects_a_bad_tail_count_and_hostile_lists() {
        let mut f = pass_frame(&[pass_entry(10044, 1, 1, [1, 0, 1])]);
        let n = f.len();
        f[n - 4] = 2; // trailing u32 != 1
        assert_eq!(ops().parse(&f), None);
        for count in [0u8, 9] {
            let mut f = pass_frame(&[pass_entry(10044, 1, 1, [0, 0, 0])]);
            f[2] = count;
            assert_eq!(ops().parse(&f), None, "count {count}");
        }
        // a sub-list claiming 255 entries in a frame that holds none
        let mut big = pass_frame(&[pass_entry(10044, 1, 1, [0, 0, 0])]);
        big[2 + 1 + 12 + 16] = 255;
        assert_eq!(ops().parse(&big), None);
    }

    #[test]
    fn pets_snapshot() {
        let mut f = vec![0x00, 0x90];
        f.extend(u32s(&[501])); // active
        f.push(2); // two level records [id][id][level]
        f.extend(u32s(&[501, 501, 3, 502, 502, 1]));
        f.push(1); // one soul pair [id][souls]
        f.extend(u32s(&[502, 12]));
        assert_eq!(
            ops().parse(&f),
            Some(Event::Pets { active: 501, levels: vec![(501, 3), (502, 1)], souls: vec![(502, 12)] })
        );
        assert_eq!(ops().parse(&f[..f.len() - 1]), None);
    }

    #[test]
    fn pet_soul_events() {
        let mut d = vec![0x0d, 0x90, 1];
        d.extend(u32s(&[502, 1]));
        assert_eq!(ops().parse(&d), Some(Event::PetSoulsDelta(vec![(502, 1)])));
        let mut t = vec![0x0e, 0x90, 1];
        t.extend(u32s(&[502, 25]));
        assert_eq!(ops().parse(&t), Some(Event::PetSoulsTotal(vec![(502, 25)])));
        let mut l = vec![0x14, 0x90, 0, 0];
        l.extend(u32s(&[502, 2]));
        l.extend([0x04, 0x06, 0, 0, 0, 1, 2, 0, 0, 0, 0, 0, 0]);
        assert_eq!(ops().parse(&l), Some(Event::PetLevel { id: 502, level: 2 }));
    }

    #[test]
    fn counter_update_single_record() {
        let mut f = vec![0x0c, 0x61, 0x00, 0x0c];
        f.extend(u32s(&[4]));
        f.extend([0x01, 0x0c, 0x03]); // left 1, two trailer bytes
        assert_eq!(ops().parse(&f), Some(Event::Counter(CounterRecord { id: 4, value: 1, ms: false })));
    }

    #[test]
    fn counter_update_checked_values() {
        for (body, value) in [
            (vec![0x00, 0x0c, 0x03], 0),
            (vec![0xb4, 0x01, 0x0c, 0x03], 180),
            (vec![0xff, 0xff, 0xff, 0xff, 0x0f, 0x0c, 0x03], u32::MAX),
        ] {
            let mut f = vec![0x0c, 0x61, 0x00, 0x0c];
            f.extend(u32s(&[4]));
            f.extend(body);
            assert_eq!(
                ops().parse(&f),
                Some(Event::Counter(CounterRecord {
                    id: 4,
                    value: u64::from(value),
                    ms: false
                }))
            );
        }
    }

    #[test]
    fn counter_update_rejects_malformed_bodies() {
        for body in [
            vec![],                                         // no value or trailers
            vec![0x00],                                     // no trailers
            vec![0x00, 0x0c],                               // only one trailer
            vec![0x81, 0x80],                               // unterminated varint
            vec![0x80, 0x80, 0x80, 0x80, 0x80, 0x0c, 0x03], // sixth byte needed
            vec![0xff, 0xff, 0xff, 0xff, 0x7f, 0x0c, 0x03], // exceeds u32
            vec![0x00, 0x0c, 0x03, 0x00],                   // extra byte
        ] {
            let mut f = vec![0x0c, 0x61, 0x00, 0x0c];
            f.extend(u32s(&[4]));
            f.extend(&body);
            assert_eq!(ops().parse(&f), None, "malformed body: {body:?}");
        }
    }

    #[test]
    fn counter_update_masks() {
        // mask 04 with one trailer byte: the shape that used to be dropped
        let mut f = vec![0x0c, 0x61, 0x00, 0x04];
        f.extend(u32s(&[10]));
        f.extend([0x0e, 0x02]);
        assert_eq!(ops().parse(&f), Some(Event::Counter(CounterRecord { id: 10, value: 14, ms: false })));
        // mask 01: a time counter keeps the wire's ms flag
        let mut t = vec![0x0c, 0x61, 0x00, 0x01];
        t.extend(u32s(&[3]));
        t.extend(25_200_000u64.to_le_bytes());
        t.push(0x02);
        assert_eq!(ops().parse(&t), Some(Event::Counter(CounterRecord { id: 3, value: 25_200_000, ms: true })));
        for (mask, body) in [
            (0x14, vec![0x0e, 0x02]),       // unknown mask bit
            (0x04, vec![0x0e]),             // no trailer byte
            (0x04, vec![0x0e, 0x02, 0x02]), // two trailer bytes
        ] {
            let mut f = vec![0x0c, 0x61, 0x00, mask];
            f.extend(u32s(&[10]));
            f.extend(&body);
            assert_eq!(ops().parse(&f), None, "mask {mask:#x} body {body:?}");
        }
    }

    /// One `0b 61` record: `[mask][u32 id][fields]`.
    fn rec(mask: u8, id: u32, fields: &[u8]) -> Vec<u8> {
        let mut v = vec![mask];
        v.extend(id.to_le_bytes());
        v.extend_from_slice(fields);
        v
    }

    fn snapshot(records: &[Vec<u8>]) -> Vec<u8> {
        let mut f = vec![0x0b, 0x61, records.len() as u8];
        for r in records {
            f.extend_from_slice(r);
        }
        f
    }

    fn counters(f: &[u8]) -> Vec<(u32, u64, bool)> {
        match ops().parse(f) {
            Some(Event::Counters(rs)) => rs.into_iter().map(|r| (r.id, r.value, r.ms)).collect(),
            other => panic!("not a counter snapshot: {other:?}"),
        }
    }

    fn one(mask: u8, fields: &[u8]) -> (u32, u64, bool) {
        counters(&snapshot(&[rec(mask, 7, fields)]))[0]
    }

    #[test]
    fn counter_snapshot_one_record_per_mask_kind() {
        assert_eq!(one(0x00, &[]), (7, 0, false));
        assert_eq!(one(0x04, &[0x02]), (7, 2, false));
        assert_eq!(one(0x04, &[0xb4, 0x01]), (7, 180, false)); // two-byte varint
        // only `charged` present: left is absent, so 0 even though a varint follows
        assert_eq!(one(0x08, &[0x02]), (7, 0, false));
        // left read, charged skipped
        assert_eq!(one(0x0c, &[0x03, 0x01]), (7, 3, false));
        // time counter: u64 ms
        assert_eq!(one(0x01, &25_200_000u64.to_le_bytes()), (7, 25_200_000, true));
        // refilling time counter: ms left, then the refill period (dropped)
        let both = [&9_000_000u64.to_le_bytes()[..], &3_600_000u64.to_le_bytes()[..]].concat();
        assert_eq!(one(0x03, &both), (7, 9_000_000, true));
    }

    #[test]
    fn counter_snapshot_multi_record_frame_tiles_exactly() {
        let f = snapshot(&[
            rec(0x0c, 1, &[0x0c, 0x01]),
            rec(0x01, 3, &25_200_000u64.to_le_bytes()),
            rec(0x00, 4, &[]),
            rec(0x08, 80_000_001, &[0x03]),
            rec(0x04, 90_000_006, &[0x02]), // last record has no following mask
        ]);
        assert_eq!(
            counters(&f),
            vec![
                (1, 12, false),
                (3, 25_200_000, true),
                (4, 0, false),
                (80_000_001, 0, false),
                (90_000_006, 2, false),
            ]
        );
    }

    #[test]
    fn counter_snapshot_that_does_not_tile_is_rejected() {
        let mut f = snapshot(&[rec(0x04, 1, &[0x02]), rec(0x04, 2, &[0x03])]);
        assert!(ops().parse(&f).is_some());
        f[2] = 3; // claims three records
        assert_eq!(ops().parse(&f), None);
        f[2] = 2;
        f.push(0x00); // leftover byte after the last record
        assert_eq!(ops().parse(&f), None);
        // unknown mask bit
        assert_eq!(ops().parse(&snapshot(&[rec(0x10, 1, &[])])), None);
        // ids must strictly ascend
        assert_eq!(ops().parse(&snapshot(&[rec(0x00, 2, &[]), rec(0x00, 1, &[])])), None);
        assert_eq!(ops().parse(&snapshot(&[rec(0x00, 2, &[]), rec(0x00, 2, &[])])), None);
        // truncated varint, and a u64 cut short
        assert_eq!(ops().parse(&snapshot(&[rec(0x04, 1, &[0x80])])), None);
        assert_eq!(ops().parse(&snapshot(&[rec(0x01, 1, &[0; 7])])), None);
    }

    #[test]
    fn varint_rejects_a_fifth_byte_that_overflows_u32() {
        let v = |b: &[u8]| Cur { b, i: 0 }.varint();
        assert_eq!(v(&[0xff, 0xff, 0xff, 0xff, 0x0f]), Some(u32::MAX));
        assert_eq!(v(&[0x80, 0x80, 0x80, 0x80, 0x10]), None); // 2^32 would truncate to 0
        assert_eq!(v(&[0xff, 0xff, 0xff, 0xff, 0x7f]), None);
        assert_eq!(v(&[0x80, 0x80, 0x80, 0x80, 0x80]), None); // sixth byte needed
    }

    #[test]
    fn pets_snapshot_with_hostile_varints_is_rejected() {
        let head = |varint: &[u8]| {
            let mut f = vec![0x00, 0x90];
            f.extend(u32s(&[501]));
            f.extend(varint);
            f
        };
        // claims 2^28 level records in a frame that holds one
        let mut big = head(&[0x80, 0x80, 0x80, 0x80, 0x01]);
        big.extend(u32s(&[501, 501, 3]));
        assert_eq!(ops().parse(&big), None);
        // overlong 5-byte varint: the high bits used to be dropped, reading 2^32 as 0 records
        let mut overlong = head(&[0x80, 0x80, 0x80, 0x80, 0x10]);
        overlong.push(0); // and no souls
        assert_eq!(ops().parse(&overlong), None);
    }

    #[test]
    fn counter_records_rejects_hostile_headers() {
        let ok = [rec(0x04, 1, &[0x02]), rec(0x04, 2, &[0x03])].concat();
        assert_eq!(counter_records(&ok, 2).map(|r| r.len()), Some(2));
        assert!(counter_records(&ok, 0).is_none());
        assert!(counter_records(&ok, 3).is_none()); // 3 records need 15 bytes, there are 12
        assert!(counter_records(&vec![0u8; MAX_SNAPSHOT + 1], 1).is_none());
        // a record is at least 5 bytes, so 255 of them cannot fit in a short frame
        let mut hostile = vec![0x0b, 0x61, 255];
        hostile.extend(&ok);
        assert_eq!(ops().parse(&hostile), None);
        // ids stay below MAX_ID
        assert!(counter_records(&rec(0x00, MAX_ID - 1, &[]), 1).is_some());
        assert!(counter_records(&rec(0x00, MAX_ID, &[]), 1).is_none());
        assert_eq!(ops().parse(&snapshot(&[rec(0x00, u32::MAX, &[])])), None);
    }

    #[test]
    fn truncated_frames_never_panic() {
        let mut ident = vec![0x33, 0x36];
        ident.extend([0u8; 7]);
        ident.push(8);
        ident.extend(b"Testname");
        ident.extend(7u16.to_le_bytes());
        ident.extend(u32s(&[3]));
        ident.push(1);
        ident.extend(u32s(&[10, 9, 9, 10]));
        let mut pets = vec![0x00, 0x90];
        pets.extend(u32s(&[501]));
        pets.push(2);
        pets.extend(u32s(&[501, 501, 3, 502, 502, 1]));
        pets.push(1);
        pets.extend(u32s(&[502, 12]));
        let time = [&9_000_000u64.to_le_bytes()[..], &3_600_000u64.to_le_bytes()[..]].concat();
        let snap = snapshot(&[rec(0x0c, 1, &[0x0c, 0x01]), rec(0x03, 3, &time), rec(0x08, 70_000_001, &[0x03])]);
        let mut upd = vec![0x0c, 0x61, 0x00, 0x0c];
        upd.extend(u32s(&[4]));
        upd.extend([0x01, 0x0c, 0x03]);
        let mut lvl = vec![0x1d, 0x56];
        lvl.extend(u32s(&[1500, 1500]));
        let boards = boards_frame(&[(10, 1, &[true; 10]), (20, 2, &[false, true])]);
        let mut entry = vec![0x3e, 0xe2, 0, 0];
        entry.extend(u32s(&[10, 4, 7]));
        entry.push(1);
        entry.extend([0u8; 18]);
        let mut kinah = vec![0xa5, 0xff];
        kinah.extend([1u8; 16]);
        let pass = pass_frame(&[pass_entry(10044, 1, 1, [1, 1, 1]), pass_entry(10045, 2, 2, [0, 1, 0])]);
        let radar = |op: u8, mid: &[u8]| {
            let mut f = vec![op, if op == 0x2a { 0x38 } else { 0x37 }];
            f.extend(ENT);
            f.extend(mid);
            f.extend(f32s(&[81_000.0, 73_200.0, 1.0]));
            f
        };
        let mut spawn = vec![0x41, 0x36];
        spawn.extend(ENT);
        spawn.extend([4, 0x20, 0]);
        spawn.extend(u32s(&[2_100_025]));
        spawn.extend([0, 2]);
        spawn.extend(f32s(&[1.0, 2.0, 3.0]));
        let mut zone = vec![0x23, 0x36, 0, 0, 0, 0, 0, 0];
        zone.extend(f32s(&[1.0, 2.0, 3.0]));
        let ops = ops();
        for f in [
            ident, pets, snap, upd, lvl, boards, entry, kinah, pass,
            spawn, zone, radar(0x1c, &[5, 3]), radar(0x1a, &[0, 0]), radar(0x3c, &[3]), radar(0x2a, &[9; 8]),
        ] {
            assert!(ops.parse(&f).is_some()); // the whole frame is valid
            for n in 0..=f.len() {
                let _ = ops.parse(&f[..n]); // must return, whatever it returns
            }
        }
    }

    #[test]
    fn unknown_opcode_and_short_frames_are_none() {
        assert_eq!(ops().parse(&[0xaa, 0xbb, 1, 2, 3]), None);
        assert_eq!(ops().parse(&[0x33]), None);
        assert_eq!(ops().parse(&[]), None);
    }

    #[test]
    fn table_for_an_unknown_build_is_an_error() {
        assert!(Opcodes::embedded("no-such-build").is_err());
    }

    #[test]
    fn json_is_ids_and_raw_values() {
        let e = Event::Counter(CounterRecord { id: 3, value: 25_200_000, ms: true });
        assert_eq!(
            e.to_json(),
            serde_json::json!({"type": "counter", "record": {"id": 3, "value": 25_200_000u64, "ms": true}})
        );
        assert_eq!(CounterRecord { id: 3, value: 25_200_000, ms: true }.normalized(), 25_200);
    }
}
