//! Live radar: the monsters in view and the player's last exact position. Transient, never saved.
//! Positions are hundredths of a map unit (see `Event::Spawn`); layouts and evidence in
//! docs/protocol.md ("Radar").

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::decode::Decoded;
use crate::events::Event;

/// Monsters kept at once. The server despawns what leaves the view, so real counts stay small; this
/// only bounds a run that lost despawn frames.
/// ponytail: the radar is dropped whole when full; evict by age if that ever shows.
const MAX_MONSTERS: usize = 4096;

#[derive(Default)]
pub struct Radar {
    /// The player's own entity, from the last identity.
    me: Option<u32>,
    /// `(x, y, ts_us)` of the last exact fix: a zone load, or a skill frame led by `me`.
    player: Option<(i32, i32, u64)>,
    /// The map the player is in (`Event::MapEnter`), kept across identities: the map load comes
    /// before the identity it belongs to at world entry.
    map: Option<u32>,
    /// `entity -> (template, x, y)`.
    monsters: BTreeMap<u32, (u32, i32, i32)>,
}

impl Radar {
    /// Fold one decoded event in; `true` when something the radar shows changed.
    /// ponytail: one radar for the whole capture, not per connection; a second game client on the
    /// same machine would mix them.
    pub fn apply(&mut self, d: &Decoded) -> bool {
        match d.event {
            Event::Identity { entity, .. } if self.me != Some(entity) => {
                *self = Self { me: Some(entity), map: self.map, ..Self::default() };
                true
            }
            Event::Spawn { entity, template, x, y, .. } => {
                if self.monsters.len() >= MAX_MONSTERS {
                    self.monsters.clear();
                }
                self.monsters.insert(entity, (template, x, y)) != Some((template, x, y))
            }
            Event::Pos { entity, x, y } => {
                if let Some(m) = self.monsters.get_mut(&entity) {
                    let moved = (m.1, m.2) != (x, y);
                    (m.1, m.2) = (x, y);
                    moved
                } else if self.me == Some(entity) {
                    self.player = Some((x, y, d.ts_us));
                    true
                } else {
                    false
                }
            }
            Event::Cast { entity, x, y } if self.me == Some(entity) => {
                self.player = Some((x, y, d.ts_us));
                true
            }
            Event::MapEnter { map } if self.map != Some(map) => {
                // what was in view and where the player stood belong to the old map
                *self = Self { me: self.me, map: Some(map), ..Self::default() };
                true
            }
            Event::Gone { entity } => self.monsters.remove(&entity).is_some(),
            Event::Teleport { x, y } => {
                self.player = Some((x, y, d.ts_us));
                true
            }
            _ => false,
        }
    }

    /// The radar as ids and raw values; the frontend divides positions by 100 and resolves names.
    pub fn to_json(&self) -> Value {
        let monsters: Vec<Value> = self
            .monsters
            .iter()
            .map(|(e, &(t, x, y))| json!({"entity": e, "template": t, "x": x, "y": y}))
            .collect();
        let player = self.player.map(|(x, y, t_us)| json!({"x": x, "y": y, "t_us": t_us}));
        json!({"map": self.map, "player": player, "monsters": monsters})
    }
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

    #[test]
    fn monsters_spawn_move_and_leave() {
        let mut r = Radar::default();
        assert!(r.apply(&at(1, Event::Spawn { entity: 5, template: 2_100_025, x: 100, y: 200, max_hp: None })));
        assert!(!r.apply(&at(2, Event::Spawn { entity: 5, template: 2_100_025, x: 100, y: 200, max_hp: None }))); // repeated
        assert!(r.apply(&at(3, Event::Pos { entity: 5, x: 110, y: 200 })));
        assert!(!r.apply(&at(4, Event::Pos { entity: 5, x: 110, y: 200 })));
        assert!(!r.apply(&at(5, Event::Pos { entity: 9, x: 1, y: 1 }))); // nobody we know
        assert_eq!(r.to_json()["monsters"], json!([{"entity": 5, "template": 2_100_025, "x": 110, "y": 200}]));
        assert!(r.apply(&at(6, Event::Gone { entity: 5 })));
        assert!(!r.apply(&at(7, Event::Gone { entity: 5 })));
        assert_eq!(r.to_json()["monsters"], json!([]));
    }

    #[test]
    fn the_player_is_known_from_a_zone_load_or_a_skill_frame_led_by_their_own_entity() {
        let mut r = Radar::default();
        assert_eq!(r.to_json()["player"], Value::Null);
        assert!(r.apply(&at(1, Event::Teleport { x: -8500, y: -79_100 })));
        assert_eq!(r.to_json()["player"], json!({"x": -8500, "y": -79_100, "t_us": 1}));
        r.apply(&at(2, identity(77)));
        assert_eq!(r.to_json()["player"], Value::Null); // a new entity is a new session
        assert!(!r.apply(&at(3, Event::Cast { entity: 8, x: 5, y: 5 }))); // someone else
        assert!(r.apply(&at(4, Event::Cast { entity: 77, x: 5, y: 6 })));
        assert_eq!(r.to_json()["player"], json!({"x": 5, "y": 6, "t_us": 4}));
        assert!(r.apply(&at(5, Event::Pos { entity: 77, x: 7, y: 8 }))); // the player moving
        assert_eq!(r.to_json()["player"], json!({"x": 7, "y": 8, "t_us": 5}));
    }

    #[test]
    fn the_same_identity_again_keeps_the_radar_and_a_new_one_clears_it() {
        let mut r = Radar::default();
        r.apply(&at(1, identity(77)));
        r.apply(&at(2, Event::Spawn { entity: 5, template: 1, x: 1, y: 1, max_hp: None }));
        assert!(!r.apply(&at(3, identity(77)))); // sent several times at world entry
        assert_eq!(r.to_json()["monsters"].as_array().unwrap().len(), 1);
        assert!(r.apply(&at(4, identity(78))));
        assert_eq!(r.to_json()["monsters"], json!([]));
    }

    #[test]
    fn the_map_comes_before_the_identity_and_a_new_map_clears_what_was_in_view() {
        let mut r = Radar::default();
        assert_eq!(r.to_json()["map"], Value::Null);
        assert!(r.apply(&at(1, Event::MapEnter { map: 1010 })));
        assert!(!r.apply(&at(2, Event::MapEnter { map: 1010 }))); // sent again at a teleport inside the map
        r.apply(&at(3, identity(77)));
        assert_eq!(r.to_json()["map"], 1010); // the load preceded the identity it belongs to
        r.apply(&at(4, Event::Spawn { entity: 5, template: 1, x: 1, y: 1, max_hp: None }));
        r.apply(&at(5, Event::Teleport { x: 9, y: 9 }));
        assert!(r.apply(&at(6, Event::MapEnter { map: 200_003 })));
        let j = r.to_json();
        assert_eq!((j["map"].clone(), j["monsters"].clone(), j["player"].clone()), (json!(200_003), json!([]), Value::Null));
        assert!(r.apply(&at(7, Event::Cast { entity: 77, x: 1, y: 1 }))); // still the same player
    }

    #[test]
    fn a_full_radar_starts_over() {
        let mut r = Radar::default();
        for e in 0..=MAX_MONSTERS as u32 {
            r.apply(&at(1, Event::Spawn { entity: e, template: 1, x: 0, y: 0, max_hp: None }));
        }
        assert_eq!(r.to_json()["monsters"].as_array().unwrap().len(), 1);
    }
}
