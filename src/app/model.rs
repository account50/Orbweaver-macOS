use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    #[default]
    Default,
    Key {
        chord: String,
    },
    Sequence {
        keys: String,
        interval_ms: u64,
    },
    Layer {
        map: usize,
        momentary: bool,
    },
    Disabled,
}
impl Action {
    pub fn label(&self) -> String {
        match self {
            Self::Default => "Default".into(),
            Self::Key { chord } => chord.clone(),
            Self::Sequence { .. } => "Sequence".into(),
            Self::Layer { map, momentary } => format!(
                "{} {}",
                if *momentary { "Hold map" } else { "Map" },
                map + 1
            ),
            Self::Disabled => "Disabled".into(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Control {
    pub name: String,
    pub inputs: Vec<u16>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Keymap {
    pub name: String,
    pub bindings: Vec<Action>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Static,
    PerKey,
    Spectrum,
    Breathing,
    Wave,
    Off,
}
impl Effect {
    pub fn label(self) -> &'static str {
        match self {
            Self::Static => "Solid color",
            Self::PerKey => "Per-key colors",
            Self::Spectrum => "Spectrum",
            Self::Breathing => "Breathing",
            Self::Wave => "Wave",
            Self::Off => "Off",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Lighting {
    pub effect: Effect,
    pub color: [u8; 3],
    pub colors: Vec<[u8; 3]>,
    pub brightness: u8,
    pub reverse: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    pub controls: Vec<Control>,
    pub maps: Vec<Keymap>,
    pub lighting: Lighting,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Library {
    pub schema_version: u32,
    pub selected: usize,
    pub profiles: Vec<Profile>,
}
impl Default for Profile {
    fn default() -> Self {
        let input = [
            53, 30, 31, 32, 33, 43, 20, 26, 8, 21, 57, 4, 22, 7, 9, 225, 29, 27, 6, 25,
        ];
        let mut controls: Vec<_> = input
            .iter()
            .enumerate()
            .map(|(i, k)| Control {
                name: format!("{:02}", i + 1),
                inputs: vec![*k],
            })
            .collect();
        controls.extend(
            [
                ("Thumb", vec![226]),
                ("Space", vec![44]),
                ("N", vec![82]),
                ("NE", vec![82, 79]),
                ("E", vec![79]),
                ("SE", vec![81, 79]),
                ("S", vec![81]),
                ("SW", vec![81, 80]),
                ("W", vec![80]),
                ("NW", vec![82, 80]),
            ]
            .into_iter()
            .map(|(n, inputs)| Control {
                name: n.into(),
                inputs,
            }),
        );
        Self {
            name: "Everyday".into(),
            controls,
            maps: (0..8)
                .map(|i| Keymap {
                    name: format!("Keymap {}", i + 1),
                    bindings: vec![Action::Default; 30],
                })
                .collect(),
            lighting: Lighting {
                effect: Effect::Static,
                color: [66, 224, 176],
                colors: vec![[66, 224, 176]; 20],
                brightness: 80,
                reverse: false,
            },
        }
    }
}
impl Default for Library {
    fn default() -> Self {
        Self {
            schema_version: 1,
            selected: 0,
            profiles: vec![Profile::default()],
        }
    }
}
impl Library {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema_version == 1, "unsupported profile format");
        ensure!(
            !self.profiles.is_empty()
                && self.profiles.len() <= 128
                && self.selected < self.profiles.len(),
            "invalid profile count/selection"
        );
        for p in &self.profiles {
            p.validate()?;
        }
        Ok(())
    }
}
impl Profile {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.name.trim().is_empty() && self.name.len() <= 120,
            "profile needs a name (up to 120 bytes)"
        );
        ensure!(
            self.controls.len() == 30 && self.maps.len() == 8 && self.lighting.colors.len() == 20,
            "profile must contain 30 controls, 8 maps, and 20 LED colors"
        );
        ensure!(self.lighting.brightness <= 100, "brightness must be 0–100");
        for c in &self.controls {
            ensure!(
                !c.inputs.is_empty()
                    && c.inputs.len() <= 2
                    && c.inputs.iter().all(|u| key_for_usage(*u).is_some()),
                "unsupported physical input for {}",
                c.name
            );
        }
        for map in &self.maps {
            ensure!(map.bindings.len() == 30, "keymap must contain 30 bindings");
            for action in &map.bindings {
                match action {
                    Action::Key { chord } => {
                        parse_chord(chord)?;
                    }
                    Action::Sequence { keys, interval_ms } => {
                        ensure!(
                            (20..=5000).contains(interval_ms),
                            "sequence interval must be 20–5000 ms"
                        );
                        parse_sequence(keys)?;
                    }
                    Action::Layer { map, .. } => ensure!(*map < 8, "keymap must be 1–8"),
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

// Name, USB keyboard usage, macOS virtual keycode. Names describe US physical key positions.
pub const KEYS: &[(&str, u16, u16)] = &[
    ("a", 4, 0),
    ("b", 5, 11),
    ("c", 6, 8),
    ("d", 7, 2),
    ("e", 8, 14),
    ("f", 9, 3),
    ("g", 10, 5),
    ("h", 11, 4),
    ("i", 12, 34),
    ("j", 13, 38),
    ("k", 14, 40),
    ("l", 15, 37),
    ("m", 16, 46),
    ("n", 17, 45),
    ("o", 18, 31),
    ("p", 19, 35),
    ("q", 20, 12),
    ("r", 21, 15),
    ("s", 22, 1),
    ("t", 23, 17),
    ("u", 24, 32),
    ("v", 25, 9),
    ("w", 26, 13),
    ("x", 27, 7),
    ("y", 28, 16),
    ("z", 29, 6),
    ("1", 30, 18),
    ("2", 31, 19),
    ("3", 32, 20),
    ("4", 33, 21),
    ("5", 34, 23),
    ("6", 35, 22),
    ("7", 36, 26),
    ("8", 37, 28),
    ("9", 38, 25),
    ("0", 39, 29),
    ("enter", 40, 36),
    ("escape", 41, 53),
    ("backspace", 42, 51),
    ("tab", 43, 48),
    ("space", 44, 49),
    ("minus", 45, 27),
    ("equal", 46, 24),
    ("left_bracket", 47, 33),
    ("right_bracket", 48, 30),
    ("backslash", 49, 42),
    ("semicolon", 51, 41),
    ("quote", 52, 39),
    ("grave", 53, 50),
    ("comma", 54, 43),
    ("period", 55, 47),
    ("slash", 56, 44),
    ("caps_lock", 57, 57),
    ("f1", 58, 122),
    ("f2", 59, 120),
    ("f3", 60, 99),
    ("f4", 61, 118),
    ("f5", 62, 96),
    ("f6", 63, 97),
    ("f7", 64, 98),
    ("f8", 65, 100),
    ("f9", 66, 101),
    ("f10", 67, 109),
    ("f11", 68, 103),
    ("f12", 69, 111),
    ("f13", 104, 105),
    ("f14", 105, 107),
    ("f15", 106, 113),
    ("home", 74, 115),
    ("page_up", 75, 116),
    ("delete", 76, 117),
    ("end", 77, 119),
    ("page_down", 78, 121),
    ("right", 79, 124),
    ("left", 80, 123),
    ("down", 81, 125),
    ("up", 82, 126),
    ("control", 224, 59),
    ("shift", 225, 56),
    ("option", 226, 58),
    ("command", 227, 55),
    ("right_control", 228, 62),
    ("right_shift", 229, 60),
    ("right_option", 230, 61),
    ("right_command", 231, 54),
];
pub fn key_for_usage(u: u16) -> Option<&'static (&'static str, u16, u16)> {
    KEYS.iter().find(|k| k.1 == u)
}
pub fn input_label(inputs: &[u16]) -> String {
    inputs
        .iter()
        .map(|u| key_for_usage(*u).map(|k| k.0).unwrap_or("?"))
        .collect::<Vec<_>>()
        .join(" + ")
}
pub fn parse_chord(s: &str) -> Result<Vec<u16>> {
    let mut out = vec![];
    for token in s.split('+') {
        let lower = token.trim().to_ascii_lowercase();
        let name = match lower.as_str() {
            "cmd" | "super" => "command",
            "ctrl" => "control",
            "alt" => "option",
            "return" => "enter",
            "esc" => "escape",
            n => n,
        };
        let code = KEYS
            .iter()
            .find(|k| k.0 == name)
            .map(|k| k.2)
            .with_context(|| {
                format!("Unknown key '{token}'. Use names such as command+c, space, or f5.")
            })?;
        ensure!(!out.contains(&code), "duplicate key in shortcut");
        out.push(code);
    }
    ensure!(
        !out.is_empty() && out.len() <= 5,
        "shortcut requires 1–5 keys"
    );
    ensure!(
        out[..out.len() - 1]
            .iter()
            .all(|c| matches!(c,54..=56|58..=62)),
        "only modifiers can precede the final key"
    );
    Ok(out)
}
pub fn parse_sequence(s: &str) -> Result<Vec<Vec<u16>>> {
    let parts: Vec<_> = s.split(',').collect();
    ensure!(
        !parts.is_empty() && parts.len() <= 128,
        "sequence requires 1–128 shortcuts"
    );
    parts.into_iter().map(parse_chord).collect()
}
pub fn path() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
        .join("Library/Application Support/Orbweaver/profiles.json")
}
pub fn load() -> Result<Library> {
    let p = path();
    if !p.exists() {
        return Ok(Library::default());
    }
    let l: Library = serde_json::from_slice(&fs::read(p)?)?;
    l.validate()?;
    Ok(l)
}
pub fn save(l: &Library) -> Result<()> {
    l.validate()?;
    atomic_save(&path(), &serde_json::to_vec_pretty(l)?)
}
pub fn atomic_save(p: &Path, bytes: &[u8]) -> Result<()> {
    let dir = p.parent().context("missing parent directory")?;
    fs::create_dir_all(dir)?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    use std::io::Write;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(p).map_err(|e| e.error)?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_roundtrip() {
        let l = Library::default();
        l.validate().unwrap();
        assert_eq!(
            serde_json::from_slice::<Library>(&serde_json::to_vec(&l).unwrap()).unwrap(),
            l
        );
    }
    #[test]
    fn bad_profiles_rejected() {
        let mut l = Library::default();
        l.profiles[0].maps[0].bindings[0] = Action::Key {
            chord: "shell+rm".into(),
        };
        assert!(l.validate().is_err());
        assert!(parse_chord("a+b").is_err());
        assert!(parse_sequence("cmd+c, cmd+v").is_ok());
    }
}
