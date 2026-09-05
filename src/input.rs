//! Decode only the keyboard layouts verified in the research snapshot.
use anyhow::{Result, ensure};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    BootKeyboard,
    ExtendedKeyboard,
    Pointer,
    Unknown,
}

pub fn layout(descriptor: &[u8]) -> Layout {
    // Exact descriptor matching prevents accidental interpretation of another revision.
    let hex = crate::device::hex(descriptor);
    match hex.as_str() {
        "05010906a101050719e029e715002501750195088102810119002aff00150026ff007508950681000508190129031500250175019503910295059101c0" => {
            Layout::BootKeyboard
        }
        "05010906a1018501050719e029e71500250175019508810219002aff00150026ff007508950e8100c0050c0901a101850219002a3c021500263c029501751081007508950d8101c005010980a10185031981298315002501750195038102950581017508950e8101c005010900a10185040903150026ff00350046ff007508950f8100c005010900a10185050903150026ff00350046ff007508950f8100c0" => {
            Layout::ExtendedKeyboard
        }
        "05010902a1010901a100050919012903150025019503750181027501950581030600ff0940950275081581257f8102050109381581257f7508950181060930093116008026ff7f751095028106c00600ff0902150025017508955ab101c0" => {
            Layout::Pointer
        }
        _ => Layout::Unknown,
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Transition {
    pub report_id: u8,
    pub usage_page: u16,
    pub usage: u16,
    pub pressed: bool,
}

#[derive(Default)]
pub struct Decoder {
    held: BTreeMap<(u8, u16), BTreeSet<u16>>,
}

impl Decoder {
    pub fn decode(&mut self, layout: Layout, bytes: &[u8]) -> Result<Vec<Transition>> {
        let (id, page, next) = match layout {
            Layout::BootKeyboard => {
                ensure!(bytes.len() == 8, "boot report must be 8 bytes");
                (0, 7, keyboard(bytes[0], &bytes[2..])?)
            }
            Layout::ExtendedKeyboard => {
                ensure!(bytes.len() == 16, "extended report must be 16 bytes");
                match bytes[0] {
                    1 => (1, 7, keyboard(bytes[1], &bytes[2..])?),
                    2 => {
                        let usage = u16::from_le_bytes([bytes[1], bytes[2]]);
                        (
                            2,
                            12,
                            if usage == 0 {
                                BTreeSet::new()
                            } else {
                                BTreeSet::from([usage])
                            },
                        )
                    }
                    3 => (
                        3,
                        1,
                        (0..3)
                            .filter(|i| bytes[1] & (1 << i) != 0)
                            .map(|i| 0x81 + i)
                            .collect(),
                    ),
                    _ => return Ok(vec![]),
                }
            }
            Layout::Pointer => {
                ensure!(bytes.len() == 8, "pointer report must be 8 bytes");
                (
                    0,
                    9,
                    (0..3)
                        .filter(|i| bytes[0] & (1 << i) != 0)
                        .map(|i| i + 1)
                        .collect(),
                )
            }
            Layout::Unknown => return Ok(vec![]),
        };
        let previous = self.held.entry((id, page)).or_default();
        let mut result: Vec<_> = previous
            .difference(&next)
            .map(|u| Transition {
                report_id: id,
                usage_page: page,
                usage: *u,
                pressed: false,
            })
            .collect();
        result.extend(next.difference(previous).map(|u| Transition {
            report_id: id,
            usage_page: page,
            usage: *u,
            pressed: true,
        }));
        *previous = next;
        Ok(result)
    }
    /// Diagnostic state clearing only; these are not physical release reports.
    pub fn reset(&mut self) {
        self.held.clear();
    }
}

fn keyboard(modifiers: u8, keys: &[u8]) -> Result<BTreeSet<u16>> {
    ensure!(
        !keys.iter().any(|k| (1..=3).contains(k)),
        "keyboard rollover/error: previous state retained"
    );
    let mut result: BTreeSet<_> = keys.iter().filter(|k| **k > 3).map(|k| *k as u16).collect();
    result.extend(
        (0..8)
            .filter(|i| modifiers & (1 << i) != 0)
            .map(|i| 0xe0 + i),
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_and_chroma_use_the_same_verified_layouts() {
        let original: serde_json::Value = serde_json::from_str(include_str!(
            "../docs/evidence/original-inspect-2026-09-05.json"
        ))
        .unwrap();
        let chroma: serde_json::Value =
            serde_json::from_str(include_str!("../docs/evidence/cli-inspect-2026-09-05.json"))
                .unwrap();
        for interface in 0..=2 {
            let a = original
                .as_array()
                .unwrap()
                .iter()
                .find(|d| d["interface_number"] == interface)
                .unwrap();
            let b = chroma
                .as_array()
                .unwrap()
                .iter()
                .find(|d| d["interface_number"] == interface)
                .unwrap();
            assert_eq!(a["product_id"], 275);
            assert_eq!(b["product_id"], 519);
            assert_eq!(a["descriptor_hex"], b["descriptor_hex"]);
            assert_eq!(a["layout"], b["layout"]);
            assert_ne!(a["layout"], "unknown");
        }
    }
    #[test]
    fn replays_live_orbweaver_trace_without_stuck_keys() {
        let mut decoder = Decoder::default();
        let mut reports = 0;
        let mut presses = 0;
        let mut releases = 0;
        let mut usages = BTreeSet::new();
        for line in include_str!("../docs/evidence/input-capture-verified-2026-09-05.jsonl").lines()
        {
            let event: serde_json::Value = serde_json::from_str(line).unwrap();
            if event["event"] != "report" {
                continue;
            }
            assert_eq!(event["details"]["interface"], 0);
            let raw = event["details"]["raw_hex"].as_str().unwrap();
            let bytes: Vec<_> = (0..raw.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&raw[i..i + 2], 16).unwrap())
                .collect();
            let transitions = decoder.decode(Layout::BootKeyboard, &bytes).unwrap();
            assert_eq!(
                serde_json::to_value(&transitions).unwrap(),
                event["details"]["transitions"]
            );
            for transition in transitions {
                if transition.pressed {
                    presses += 1;
                    usages.insert(transition.usage);
                } else {
                    releases += 1;
                }
            }
            reports += 1;
        }
        assert_eq!((reports, presses, releases, usages.len()), (73, 40, 40, 14));
        assert!(decoder.held.values().all(BTreeSet::is_empty));
    }
    #[test]
    fn modifiers_chords_repeat_and_release() {
        let mut d = Decoder::default();
        let b = [2, 0, 4, 5, 0, 0, 0, 0];
        let first = d.decode(Layout::BootKeyboard, &b).unwrap();
        assert_eq!(
            first.iter().map(|t| t.usage).collect::<Vec<_>>(),
            vec![4, 5, 0xe1]
        );
        assert!(d.decode(Layout::BootKeyboard, &b).unwrap().is_empty());
        let releases = d.decode(Layout::BootKeyboard, &[0; 8]).unwrap();
        assert_eq!(releases.len(), 3);
        assert!(releases.iter().all(|t| !t.pressed));
    }
    #[test]
    fn rollover_does_not_release_held_keys() {
        let mut d = Decoder::default();
        d.decode(Layout::BootKeyboard, &[0, 0, 4, 0, 0, 0, 0, 0])
            .unwrap();
        assert!(
            d.decode(Layout::BootKeyboard, &[0, 0, 1, 1, 1, 1, 1, 1])
                .is_err()
        );
        assert_eq!(d.decode(Layout::BootKeyboard, &[0; 8]).unwrap()[0].usage, 4);
    }
    #[test]
    fn collections_have_independent_state() {
        let mut d = Decoder::default();
        let mut k = [0; 16];
        k[0] = 1;
        k[2] = 4;
        d.decode(Layout::ExtendedKeyboard, &k).unwrap();
        let mut c = [0; 16];
        c[0] = 2;
        c[1] = 0xe9;
        assert_eq!(d.decode(Layout::ExtendedKeyboard, &c).unwrap().len(), 1);
        k[2] = 0;
        let r = d.decode(Layout::ExtendedKeyboard, &k).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].usage, 4);
        assert!(d.decode(Layout::ExtendedKeyboard, &c).unwrap().is_empty());
    }
    #[test]
    fn rejects_truncated_and_leaves_unknown_raw() {
        let mut d = Decoder::default();
        assert!(d.decode(Layout::BootKeyboard, &[0; 7]).is_err());
        assert!(d.decode(Layout::Unknown, &[255]).unwrap().is_empty());
        assert_eq!(layout(&[0]), Layout::Unknown);
    }
}
