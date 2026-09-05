use super::model::{Action, Profile, key_for_usage, parse_chord, parse_sequence};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    pub code: u16,
    pub pressed: bool,
}
struct Held {
    outputs: Vec<u16>,
    layer: bool,
}
pub struct Engine {
    pub profile: Profile,
    pub base: usize,
    layers: Vec<(usize, usize)>,
    held: BTreeMap<usize, Held>,
    counts: BTreeMap<u16, usize>,
    queue: VecDeque<(u64, Output)>,
    pub raw: BTreeSet<u16>,
}
impl Engine {
    pub fn new(profile: Profile) -> Self {
        Self {
            profile,
            base: 0,
            layers: vec![],
            held: BTreeMap::new(),
            counts: BTreeMap::new(),
            queue: VecDeque::new(),
            raw: BTreeSet::new(),
        }
    }
    pub fn map(&self) -> usize {
        self.layers.last().map(|v| v.1).unwrap_or(self.base)
    }
    fn transition(&mut self, code: u16, pressed: bool, out: &mut Vec<Output>) {
        let count = self.counts.entry(code).or_default();
        if pressed {
            if *count == 0 {
                out.push(Output {
                    code,
                    pressed: true,
                });
            }
            *count += 1;
        } else if *count > 0 {
            *count -= 1;
            if *count == 0 {
                out.push(Output {
                    code,
                    pressed: false,
                });
            }
        }
    }
    pub fn reset(&mut self) -> Vec<Output> {
        let out = self
            .counts
            .iter()
            .filter(|(_, n)| **n > 0)
            .map(|(code, _)| Output {
                code: *code,
                pressed: false,
            })
            .collect();
        self.held.clear();
        self.layers.clear();
        self.counts.clear();
        self.queue.clear();
        self.raw.clear();
        out
    }
    pub fn state(&mut self, keys: BTreeSet<u16>, now: u64) -> Vec<Output> {
        self.raw = keys.clone();
        let mut out = vec![];
        let map = self.map();
        let candidates: Vec<_> = self
            .profile
            .controls
            .iter()
            .enumerate()
            .filter(|(i, c)| {
                c.inputs.iter().all(|u| keys.contains(u))
                    && (c.inputs.len() == 1
                        || self.profile.maps[map].bindings[*i] != Action::Default
                        || self.held.contains_key(i))
            })
            .map(|(i, _)| i)
            .collect();
        let active: BTreeSet<_> = candidates
            .iter()
            .copied()
            .filter(|i| {
                !candidates.iter().any(|j| {
                    self.profile.controls[*j].inputs.len() > self.profile.controls[*i].inputs.len()
                        && self.profile.controls[*i]
                            .inputs
                            .iter()
                            .all(|u| self.profile.controls[*j].inputs.contains(u))
                })
            })
            .collect();
        let released: Vec<_> = self
            .held
            .keys()
            .filter(|i| !active.contains(i))
            .copied()
            .collect();
        for i in released {
            let h = self.held.remove(&i).unwrap();
            for c in h.outputs.iter().rev() {
                self.transition(*c, false, &mut out);
            }
            if h.layer {
                self.layers.retain(|v| v.0 != i);
            }
        }
        let mut pressed: Vec<_> = active
            .into_iter()
            .filter(|i| !self.held.contains_key(i))
            .collect();
        pressed.sort_by_key(|i| {
            !matches!(
                self.profile.maps[self.map()].bindings[*i],
                Action::Layer { .. }
            )
        });
        for i in pressed {
            let action = self.profile.maps[self.map()].bindings[i].clone();
            let mut held = Held {
                outputs: vec![],
                layer: false,
            };
            match action {
                Action::Default => {
                    held.outputs = self.profile.controls[i]
                        .inputs
                        .iter()
                        .filter_map(|u| key_for_usage(*u).map(|k| k.2))
                        .collect();
                }
                Action::Key { chord } => {
                    held.outputs = parse_chord(&chord).unwrap_or_default();
                }
                Action::Layer { map, momentary } => {
                    if momentary {
                        self.layers.push((i, map));
                        held.layer = true;
                    } else {
                        self.base = map;
                    }
                }
                Action::Sequence { keys, interval_ms } => {
                    if let Ok(steps) = parse_sequence(&keys) {
                        let start = self
                            .queue
                            .back()
                            .map(|(t, _)| *t + interval_ms)
                            .unwrap_or(now);
                        for (step, chord) in steps.into_iter().enumerate() {
                            let t = start + step as u64 * interval_ms;
                            for code in &chord {
                                self.queue.push_back((
                                    t,
                                    Output {
                                        code: *code,
                                        pressed: true,
                                    },
                                ));
                            }
                            for code in chord.into_iter().rev() {
                                self.queue.push_back((
                                    t + interval_ms.min(40) / 2,
                                    Output {
                                        code,
                                        pressed: false,
                                    },
                                ));
                            }
                        }
                    }
                }
                Action::Disabled => {}
            }
            for c in &held.outputs {
                self.transition(*c, true, &mut out);
            }
            self.held.insert(i, held);
        }
        out.extend(self.tick(now));
        out
    }
    pub fn tick(&mut self, now: u64) -> Vec<Output> {
        let mut out = vec![];
        while self.queue.front().is_some_and(|(t, _)| *t <= now) {
            let (_, e) = self.queue.pop_front().unwrap();
            self.transition(e.code, e.pressed, &mut out);
        }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_wow_profile_has_identical_outputs_for_both_models() {
        for fixture in [
            include_str!("../../docs/evidence/original-inspect-2026-09-05.json"),
            include_str!("../../docs/evidence/cli-inspect-2026-09-05.json"),
        ] {
            let devices: serde_json::Value = serde_json::from_str(fixture).unwrap();
            let d = devices
                .as_array()
                .unwrap()
                .iter()
                .find(|d| d["interface_number"] == 0)
                .unwrap();
            let hex = d["descriptor_hex"].as_str().unwrap();
            let desc: Vec<u8> = (0..hex.len())
                .step_by(2)
                .map(|n| u8::from_str_radix(&hex[n..n + 2], 16).unwrap())
                .collect();
            let layout = crate::input::layout(&desc);
            let p: Profile =
                serde_json::from_str(include_str!("../../profiles/world-of-warcraft.json"))
                    .unwrap();
            p.validate().unwrap();
            let mut engine = Engine::new(p);
            let mut decoder = crate::input::Decoder::default();
            for (usage, output) in [
                (43, 18),
                (20, 19),
                (26, 20),
                (8, 21),
                (21, 23),
                (57, 22),
                (4, 26),
                (22, 28),
                (7, 25),
                (9, 29),
                (225, 27),
                (29, 24),
                (6, 0),
                (25, 49),
                (44, 56),
            ] {
                let mut report = [0; 8];
                if usage == 225 {
                    report[0] = 2;
                } else {
                    report[2] = usage as u8;
                }
                let keys = decoder
                    .decode(layout, &report)
                    .unwrap()
                    .into_iter()
                    .filter(|t| t.pressed)
                    .map(|t| t.usage)
                    .collect();
                assert_eq!(
                    engine.state(keys, 0),
                    vec![Output {
                        code: output,
                        pressed: true
                    }]
                );
                decoder.decode(layout, &[0; 8]).unwrap();
                assert_eq!(
                    engine.state(BTreeSet::new(), 1),
                    vec![Output {
                        code: output,
                        pressed: false
                    }]
                );
            }
        }
    }
    #[test]
    fn shared_output_not_released_early() {
        let mut p = Profile::default();
        for i in [0, 1] {
            p.maps[0].bindings[i] = Action::Key { chord: "a".into() };
        }
        let mut e = Engine::new(p);
        assert_eq!(
            e.state(BTreeSet::from([53, 30]), 0),
            vec![Output {
                code: 0,
                pressed: true
            }]
        );
        assert!(e.state(BTreeSet::from([30]), 1).is_empty());
        assert_eq!(
            e.state(BTreeSet::new(), 2),
            vec![Output {
                code: 0,
                pressed: false
            }]
        );
    }
    #[test]
    fn held_mapping_survives_layer_release() {
        let mut p = Profile::default();
        p.maps[0].bindings[20] = Action::Layer {
            map: 1,
            momentary: true,
        };
        p.maps[1].bindings[0] = Action::Key {
            chord: "command+c".into(),
        };
        let mut e = Engine::new(p);
        e.state(BTreeSet::from([226]), 0);
        assert_eq!(e.map(), 1);
        assert_eq!(e.state(BTreeSet::from([226, 53]), 1).len(), 2);
        assert!(e.state(BTreeSet::from([53]), 2).is_empty());
        assert_eq!(e.map(), 0);
        let r = e.state(BTreeSet::new(), 3);
        assert_eq!(
            r,
            vec![
                Output {
                    code: 8,
                    pressed: false
                },
                Output {
                    code: 55,
                    pressed: false
                }
            ]
        );
    }
    #[test]
    fn reset_cancels_macros_and_releases_modifiers() {
        let mut p = Profile::default();
        p.maps[0].bindings[0] = Action::Sequence {
            keys: "command+c, command+v".into(),
            interval_ms: 100,
        };
        let mut e = Engine::new(p);
        assert_eq!(e.state(BTreeSet::from([53]), 0).len(), 2);
        assert_eq!(e.reset().len(), 2);
        assert!(e.tick(500).is_empty());
    }
    #[test]
    fn diagonal_overrides_cardinal() {
        let mut p = Profile::default();
        p.maps[0].bindings[23] = Action::Key {
            chord: "space".into(),
        };
        let mut e = Engine::new(p);
        assert_eq!(
            e.state(BTreeSet::from([82, 79]), 0),
            vec![Output {
                code: 49,
                pressed: true
            }]
        );
    }
}
