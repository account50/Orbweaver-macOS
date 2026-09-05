//! Minimal HID size parser. Unknown input layouts remain raw in the monitor.
use anyhow::{Result, bail, ensure};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Default)]
struct Globals {
    size: u32,
    count: u32,
    id: u8,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ReportSizes {
    pub report_id: u8,
    pub input_bytes: u32,
    pub output_bytes: u32,
    pub feature_bytes: u32,
}

/// Sizes include numbered report IDs, but exclude HIDAPI's synthetic zero ID.
pub fn sizes(data: &[u8]) -> Result<Vec<ReportSizes>> {
    let mut globals = Globals::default();
    let mut stack = Vec::new();
    let mut bits: BTreeMap<u8, [u32; 3]> = BTreeMap::new();
    let mut i = 0;
    while i < data.len() {
        let prefix = data[i];
        i += 1;
        if prefix == 0xfe {
            ensure!(i + 2 <= data.len(), "truncated long item");
            let count = data[i] as usize;
            i += 2;
            ensure!(i + count <= data.len(), "truncated long payload");
            i += count;
            continue;
        }
        let len = match prefix & 3 {
            3 => 4,
            n => n as usize,
        };
        ensure!(i + len <= data.len(), "truncated HID item");
        let value = data[i..i + len]
            .iter()
            .enumerate()
            .fold(0u32, |v, (j, b)| v | ((*b as u32) << (j * 8)));
        i += len;
        match (prefix & 0x0c, prefix >> 4) {
            (4, 7) => globals.size = value,
            (4, 8) => {
                ensure!((1..=255).contains(&value), "invalid report ID");
                globals.id = value as u8;
            }
            (4, 9) => globals.count = value,
            (4, 10) => {
                ensure!(stack.len() < 32, "HID global stack too deep");
                stack.push(globals.clone());
            }
            (4, 11) => {
                globals = stack
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("HID global stack underflow"))?
            }
            (0, tag @ (8 | 9 | 11)) => {
                let index = match tag {
                    8 => 0,
                    9 => 1,
                    _ => 2,
                };
                let n = globals
                    .size
                    .checked_mul(globals.count)
                    .ok_or_else(|| anyhow::anyhow!("HID size overflow"))?;
                let total = &mut bits.entry(globals.id).or_default()[index];
                *total = total
                    .checked_add(n)
                    .ok_or_else(|| anyhow::anyhow!("HID size overflow"))?;
                ensure!(*total <= 8 * 65536, "HID report too large");
            }
            _ => {}
        }
    }
    if !stack.is_empty() {
        bail!("unbalanced HID global stack");
    }
    Ok(bits
        .into_iter()
        .map(|(id, b)| {
            let bytes = |n: u32| {
                if n == 0 {
                    0
                } else {
                    n.div_ceil(8) + u32::from(id != 0)
                }
            };
            ReportSizes {
                report_id: id,
                input_bytes: bytes(b[0]),
                output_bytes: bytes(b[1]),
                feature_bytes: bytes(b[2]),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_captured_hardware() {
        let evidence: serde_json::Value = serde_json::from_str(include_str!(
            "../docs/evidence/device-inventory-2026-09-05.json"
        ))
        .unwrap();
        let nodes: Vec<_> = evidence["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|n| n["IOObjectClass"] == "AppleUserHIDDevice")
            .collect();
        let parsed: Vec<_> = nodes
            .iter()
            .map(|n| {
                let s = n["ReportDescriptor"].as_str().unwrap();
                sizes(
                    &(0..s.len())
                        .step_by(2)
                        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
                        .collect::<Vec<_>>(),
                )
                .unwrap()
            })
            .collect();
        assert_eq!(
            parsed[0][0],
            ReportSizes {
                report_id: 0,
                input_bytes: 8,
                output_bytes: 1,
                feature_bytes: 0
            }
        );
        assert_eq!(parsed[1].len(), 5);
        assert_eq!(parsed[1][0].input_bytes, 16);
        assert_eq!(parsed[2][0].feature_bytes, 90);
        assert_eq!(parsed[2][0].input_bytes, 8);
    }
    #[test]
    fn rejects_bad_descriptors() {
        for data in [
            &[0x76, 1][..],
            &[0xfe, 3, 0, 1],
            &[0xb4],
            &[0x85, 0],
            &[0x77, 255, 255, 255, 255, 0x95, 2, 0x81, 0],
        ] {
            assert!(sizes(data).is_err());
        }
    }
}
