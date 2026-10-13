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
    SpamKey {
        chord: String,
        interval_ms: u64,
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
            Self::SpamKey { chord, .. } => format!("Spam {chord}"),
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
/// Physical layout a profile is built for. Profiles written before this field existed are Orbweaver profiles.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    #[default]
    Orbweaver,
    TartarusV2,
}
impl Family {
    pub fn name(self) -> &'static str {
        match self {
            Self::Orbweaver => "Orbweaver",
            Self::TartarusV2 => "Tartarus V2",
        }
    }
    pub fn control_count(self) -> usize {
        match self {
            Self::Orbweaver => 30,
            Self::TartarusV2 => 29,
        }
    }
    /// RGB slots in `Lighting::colors`. Tartarus V2: keys 01–19, key 20, then the scroll wheel.
    pub fn led_count(self) -> usize {
        match self {
            Self::Orbweaver => 20,
            Self::TartarusV2 => 21,
        }
    }
    /// Color slot for a control, if that control has its own key LED.
    pub fn led_for_control(self, control: usize) -> Option<usize> {
        (control < 20).then_some(control)
    }
    /// Color slot of the scroll-wheel LED, which is not a remappable control.
    pub fn wheel_led(self) -> Option<usize> {
        match self {
            Self::Orbweaver => None,
            Self::TartarusV2 => Some(20),
        }
    }
    /// Index of the first thumb-pad direction ("N"); the eight directions follow in order.
    pub fn thumb_pad_first(self) -> usize {
        match self {
            Self::Orbweaver => 22,
            Self::TartarusV2 => 21,
        }
    }
    pub fn export_file_name(self) -> &'static str {
        match self {
            Self::Orbweaver => "orbweaver-profile.json",
            Self::TartarusV2 => "tartarus-v2-profile.json",
        }
    }
    /// Factory key usages per control. Names describe the printed key labels.
    pub fn default_controls(self) -> Vec<Control> {
        let (keys, named): (&[u16], &[(&str, &[u16])]) = match self {
            Self::Orbweaver => (
                &[
                    53, 30, 31, 32, 33, 43, 20, 26, 8, 21, 57, 4, 22, 7, 9, 225, 29, 27, 6, 25,
                ],
                &[("Thumb", &[226]), ("Space", &[44])],
            ),
            // Tartarus V2 factory layout: 1–5, Tab Q W E R, Caps A S D F, Shift Z X C,
            // key 20 (thumb paddle) Space, thumb button Alt, thumb pad arrows.
            Self::TartarusV2 => (
                &[
                    30, 31, 32, 33, 34, 43, 20, 26, 8, 21, 57, 4, 22, 7, 9, 225, 29, 27, 6,
                ],
                &[("20", &[44]), ("Thumb", &[226])],
            ),
        };
        let mut controls: Vec<_> = keys
            .iter()
            .enumerate()
            .map(|(i, k)| Control {
                name: format!("{:02}", i + 1),
                inputs: vec![*k],
            })
            .collect();
        controls.extend(named.iter().map(|(n, inputs)| Control {
            name: (*n).into(),
            inputs: inputs.to_vec(),
        }));
        controls.extend(
            [
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
        debug_assert_eq!(controls.len(), self.control_count());
        controls
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    #[serde(default)]
    pub family: Family,
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
        Self::new(Family::Orbweaver)
    }
}
impl Profile {
    /// Factory layout for one keypad family with every control at its default.
    pub fn new(family: Family) -> Self {
        Self {
            name: "Everyday".into(),
            family,
            controls: family.default_controls(),
            maps: (0..8)
                .map(|i| Keymap {
                    name: format!("Keymap {}", i + 1),
                    bindings: vec![Action::Default; family.control_count()],
                })
                .collect(),
            lighting: Lighting {
                effect: Effect::Static,
                color: [66, 224, 176],
                colors: vec![[66, 224, 176]; family.led_count()],
                brightness: 80,
                reverse: false,
            },
        }
    }
}
impl Lighting {
    /// Lighting usable on `family`. Per-key colors from another layout fall back to the solid color.
    pub fn adapted(&self, family: Family) -> (Self, bool) {
        if self.colors.len() == family.led_count() {
            return (self.clone(), false);
        }
        let mut l = self.clone();
        l.colors = vec![self.color; family.led_count()];
        if l.effect == Effect::PerKey {
            l.effect = Effect::Static;
        }
        (l, true)
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
        let (controls, leds) = (self.family.control_count(), self.family.led_count());
        ensure!(
            self.controls.len() == controls
                && self.maps.len() == 8
                && self.lighting.colors.len() == leds,
            "{} profile must contain {controls} controls, 8 maps, and {leds} LED colors",
            self.family.name()
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
            ensure!(
                map.bindings.len() == controls,
                "keymap must contain {controls} bindings"
            );
            for action in &map.bindings {
                match action {
                    Action::Key { chord } => {
                        parse_chord(chord)?;
                    }
                    Action::SpamKey { chord, interval_ms } => {
                        ensure!(
                            (20..=5000).contains(interval_ms),
                            "spam interval must be 20–5000 ms"
                        );
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
    fn spam_profiles_roundtrip_and_reject_invalid_settings() {
        let mut l = Library::default();
        l.profiles[0].maps[0].bindings[0] = Action::SpamKey {
            chord: "shift+1".into(),
            interval_ms: 100,
        };
        l.validate().unwrap();
        let json = serde_json::to_string(&l).unwrap();
        assert!(json.contains("spam_key"));
        assert_eq!(serde_json::from_str::<Library>(&json).unwrap(), l);
        for (chord, interval_ms) in [
            ("a", 0),
            ("a", 19),
            ("a", 5001),
            ("", 100),
            ("a,b", 100),
            ("a+b", 100),
        ] {
            l.profiles[0].maps[0].bindings[0] = Action::SpamKey {
                chord: chord.into(),
                interval_ms,
            };
            assert!(l.validate().is_err(), "accepted {chord} at {interval_ms}");
        }
    }
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
    fn tartarus_v2_profiles_validate_and_roundtrip() {
        let p = Profile::new(Family::TartarusV2);
        p.validate().unwrap();
        assert_eq!(p.controls.len(), 29);
        assert_eq!(p.lighting.colors.len(), 21);
        assert_eq!(p.controls[0].inputs, vec![30]);
        assert_eq!(p.controls[18].inputs, vec![6]);
        assert_eq!(
            (p.controls[19].name.as_str(), &p.controls[19].inputs[..]),
            ("20", &[44][..])
        );
        assert_eq!(p.controls[20].inputs, vec![226]);
        assert_eq!(p.controls[Family::TartarusV2.thumb_pad_first()].name, "N");
        assert_eq!(p.controls[28].name, "NW");
        assert!(p.maps.iter().all(|m| m.bindings.len() == 29));
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"family\":\"tartarus_v2\""));
        assert_eq!(serde_json::from_str::<Profile>(&json).unwrap(), p);
        // A Tartarus profile cannot borrow the Orbweaver control table, and vice versa.
        let mut wrong = p.clone();
        wrong.family = Family::Orbweaver;
        assert!(wrong.validate().is_err());
        let wrong = Profile {
            family: Family::TartarusV2,
            ..Profile::default()
        };
        assert!(wrong.validate().is_err());
    }
    #[test]
    fn profiles_without_family_are_orbweaver() {
        let wow: Profile =
            serde_json::from_str(include_str!("../../profiles/world-of-warcraft.json")).unwrap();
        assert_eq!(wow.family, Family::Orbweaver);
        wow.validate().unwrap();
        let mut json: serde_json::Value = serde_json::to_value(Profile::default()).unwrap();
        json.as_object_mut().unwrap().remove("family");
        let p: Profile = serde_json::from_value(json).unwrap();
        assert_eq!(p, Profile::default());
    }
    #[test]
    fn bundled_tartarus_wow_profile_mirrors_the_orbweaver_one() {
        let orb: Profile =
            serde_json::from_str(include_str!("../../profiles/world-of-warcraft.json")).unwrap();
        let tart: Profile = serde_json::from_str(include_str!(
            "../../profiles/world-of-warcraft-tartarus-v2.json"
        ))
        .unwrap();
        tart.validate().unwrap();
        assert_eq!(tart.family, Family::TartarusV2);
        assert_eq!(tart.controls, Family::TartarusV2.default_controls());
        for map in 0..8 {
            let (o, t) = (&orb.maps[map].bindings, &tart.maps[map].bindings);
            // Keys 01–19 keep their numbers.
            assert_eq!(&o[..19], &t[..19]);
            // Orbweaver key 20 moves to the thumb button; the lower Space thumb becomes key 20.
            assert_eq!(o[19], t[20]);
            assert_eq!(o[21], t[19]);
            assert_eq!(&o[22..30], &t[21..29]);
            assert_eq!(o[20], Action::Default);
        }
        assert_eq!(tart.lighting.colors[..20], orb.lighting.colors[..20]);
        assert_eq!(tart.lighting.colors[20], orb.lighting.colors[0]);
    }
    #[test]
    fn lighting_adapts_between_layouts() {
        let mut l = Profile::default().lighting;
        l.effect = Effect::PerKey;
        l.colors[3] = [1, 2, 3];
        let (same, changed) = l.adapted(Family::Orbweaver);
        assert!(!changed);
        assert_eq!(same, l);
        let (other, changed) = l.adapted(Family::TartarusV2);
        assert!(changed);
        assert_eq!(other.effect, Effect::Static);
        assert_eq!(other.colors, vec![l.color; 21]);
        l.effect = Effect::Wave;
        assert_eq!(l.adapted(Family::TartarusV2).0.effect, Effect::Wave);
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
