//! Firmware identity and restricted lighting writes; no arbitrary vendor writes or persistent changes.
//! Protocol references: OpenRazer 6820f9d (docs/orbweaver-research.md) and the OpenRGB Razer
//! controller for the Tartarus V2 extended matrix (docs/tartarus-v2-research.md).
use crate::app::model::{Effect, Lighting};
use crate::device::Model;
use anyhow::{Result, ensure};
use hidapi::HidDevice;
use serde::Serialize;

const SIZE: usize = 90;
/// Orbweaver Chroma: "standard" lighting command class with transaction 0x3f/0xff.
const STANDARD_CLASS: u8 = 0x03;
/// Tartarus V2: "extended matrix" command class; the firmware answers transaction 0x1f.
const EXTENDED_CLASS: u8 = 0x0f;
const EXTENDED_TRANSACTION: u8 = 0x1f;
/// Volatile storage: nothing is written to the keypad's flash.
const NOSTORE: u8 = 0x00;
const BACKLIGHT_LED: u8 = 0x05;
const ZERO_LED: u8 = 0x00;

fn checksum(bytes: &[u8]) -> u8 {
    bytes[2..88].iter().fold(0, |a, b| a ^ b)
}

fn firmware_request(transaction: u8) -> [u8; SIZE] {
    let mut report = [0; SIZE];
    report[1] = transaction;
    report[5] = 2;
    report[7] = 0x81;
    report[88] = checksum(&report);
    report
}

#[derive(Debug, Serialize)]
pub struct Firmware {
    pub major: u8,
    pub minor: u8,
    pub response_hex: String,
}

fn parse_response(bytes: &[u8], transaction: u8) -> Result<Firmware> {
    ensure!(
        bytes.len() == SIZE,
        "expected 90 response bytes, received {}",
        bytes.len()
    );
    ensure!(bytes[88] == checksum(bytes), "response checksum mismatch");
    ensure!(
        bytes[1] == transaction,
        "response transaction does not match firmware query"
    );
    ensure!(
        bytes[2..5] == [0, 0, 0] && bytes[6..8] == [0, 0x81],
        "response does not match firmware query"
    );
    ensure!(
        bytes[5] >= 2 && bytes[5] <= 80,
        "invalid firmware payload length {}",
        bytes[5]
    );
    ensure!(
        bytes[0] == 2,
        "firmware query returned status 0x{:02x}; not verified (success=02, busy=01, failure=03, timeout=04, unsupported=05)",
        bytes[0]
    );
    Ok(Firmware {
        major: bytes[8],
        minor: bytes[9],
        response_hex: crate::device::hex(bytes),
    })
}

/// Transaction IDs to try for a firmware query, most likely first.
pub fn firmware_transactions(model: Model) -> &'static [u8] {
    match model {
        Model::Original | Model::Chroma => &[0xff],
        Model::TartarusV2 => &[EXTENDED_TRANSACTION, 0xff],
    }
}

pub fn query_firmware(device: &HidDevice, transaction: u8) -> Result<Firmware> {
    // HIDAPI requires a leading zero report ID for this unnumbered feature report.
    let mut framed = [0; SIZE + 1];
    framed[1..].copy_from_slice(&firmware_request(transaction));
    device.send_feature_report(&framed)?;
    std::thread::sleep(std::time::Duration::from_micros(600));
    let mut response = [0; SIZE + 1];
    let n = device.get_feature_report(&mut response)?;
    ensure!(
        n == SIZE + 1 && response[0] == 0,
        "unexpected HIDAPI framing: {} bytes",
        n
    );
    parse_response(&response[1..], transaction)
}

/// Query firmware identity with each transaction ID the model is known to answer.
pub fn query_firmware_for(device: &HidDevice, model: Model) -> Result<Firmware> {
    let mut last = None;
    for transaction in firmware_transactions(model) {
        match query_firmware(device, *transaction) {
            Ok(f) => return Ok(f),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn valid() -> [u8; 90] {
        let mut r = firmware_request(0xff);
        r[0] = 2;
        r[8] = 1;
        r[9] = 3;
        r[88] = checksum(&r);
        r
    }
    #[test]
    fn request_wire_format() {
        let r = firmware_request(0xff);
        assert_eq!(&r[..8], &[0, 255, 0, 0, 0, 2, 0, 129]);
        assert_eq!(r[88], 0x83);
        let r = firmware_request(0x1f);
        assert_eq!(&r[..8], &[0, 0x1f, 0, 0, 0, 2, 0, 129]);
        assert_eq!(r[88], 0x83);
        assert_eq!(firmware_transactions(Model::Chroma), &[0xff]);
        assert_eq!(firmware_transactions(Model::TartarusV2), &[0x1f, 0xff]);
    }
    #[test]
    fn accepts_only_valid_identity() {
        let r = valid();
        let f = parse_response(&r, 0xff).unwrap();
        assert_eq!((f.major, f.minor), (1, 3));
        assert!(parse_response(&r, 0x1f).is_err());
        assert!(parse_response(&r[..89], 0xff).is_err());
        for index in [7, 8, 88] {
            let mut bad = r;
            bad[index] ^= 1;
            assert!(parse_response(&bad, 0xff).is_err());
        }
        let mut bad = r;
        bad[7] = 0x82;
        bad[88] = checksum(&bad);
        assert!(parse_response(&bad, 0xff).is_err());
        for status in [0, 1, 3, 4, 5] {
            let mut bad = r;
            bad[0] = status;
            assert!(parse_response(&bad, 0xff).is_err());
        }
    }
}

/// One lighting request: transaction, command class, command, arguments (at most 80 bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub transaction: u8,
    pub class: u8,
    pub command: u8,
    pub args: Vec<u8>,
}
impl Command {
    fn standard(transaction: u8, command: u8, args: &[u8]) -> Self {
        Self {
            transaction,
            class: STANDARD_CLASS,
            command,
            args: args.to_vec(),
        }
    }
    fn extended(command: u8, args: &[u8]) -> Self {
        Self {
            transaction: EXTENDED_TRANSACTION,
            class: EXTENDED_CLASS,
            command,
            args: args.to_vec(),
        }
    }
    /// HIDAPI-framed 91-byte feature report: zero report ID, then the 90-byte Razer report.
    pub fn report(&self) -> [u8; SIZE + 1] {
        let mut report = [0; SIZE + 1];
        report[2] = self.transaction;
        report[6] = self.args.len() as u8;
        report[7] = self.class;
        report[8] = self.command;
        report[9..9 + self.args.len()].copy_from_slice(&self.args);
        report[89] = checksum(&report[1..]);
        report
    }
}

/// Restricted lighting commands for 1532:0207. Protocol encoded independently.
fn chroma_commands(lighting: &Lighting) -> Vec<Command> {
    let [r, g, b] = lighting.color;
    let mut out = vec![];
    match lighting.effect {
        Effect::Static => out.push(Command::standard(0x3f, 0x0a, &[6, r, g, b])),
        Effect::Spectrum => out.push(Command::standard(0xff, 0x0a, &[4])),
        Effect::Breathing => out.push(Command::standard(0xff, 0x0a, &[3, 1, r, g, b, 0, 0, 0])),
        Effect::Wave => out.push(Command::standard(
            0xff,
            0x0a,
            &[1, if lighting.reverse { 2 } else { 1 }],
        )),
        Effect::Off => out.push(Command::standard(0xff, 0x0a, &[0])),
        Effect::PerKey => {
            for row in 0..4 {
                let mut args = [0; 70];
                args[0] = 0xff;
                args[1] = row as u8 + 1;
                args[2] = 1;
                args[3] = if row == 3 { 6 } else { 5 };
                for col in 0..5 {
                    let offset = if row == 3 && col > 0 { col + 1 } else { col };
                    args[4 + offset * 3..7 + offset * 3]
                        .copy_from_slice(&lighting.colors[row * 5 + col]);
                }
                out.push(Command::standard(0x3f, 0x0b, &args));
            }
            out.push(Command::standard(0x3f, 0x0a, &[5, 0]));
        }
    }
    // Volatile brightness; profiles are owned by this app, not written to flash.
    out.push(Command::standard(
        0xff,
        0x03,
        &[0, 5, (u16::from(lighting.brightness) * 255 / 100) as u8],
    ));
    out
}

/// Tartarus V2 LED matrix: 4 rows × 6 columns. Rows 0–2 hold keys 01–15 in columns 0–4;
/// row 3 holds keys 16–19, the scroll wheel (column 4), and key 20 (column 5).
/// Coordinates follow OpenRGB's Tartarus V2 layout and await visual confirmation.
const TARTARUS_ROWS: [[Option<usize>; 6]; 4] = [
    [Some(0), Some(1), Some(2), Some(3), Some(4), None],
    [Some(5), Some(6), Some(7), Some(8), Some(9), None],
    [Some(10), Some(11), Some(12), Some(13), Some(14), None],
    [Some(15), Some(16), Some(17), Some(18), Some(20), Some(19)],
];

/// Lighting commands for 1532:022b using the extended matrix class (0x0f), volatile storage.
fn tartarus_commands(lighting: &Lighting) -> Vec<Command> {
    let [r, g, b] = lighting.color;
    let mut out = vec![];
    let effect = |id: u8, size: usize, extra: &[u8]| {
        let mut args = vec![0; size];
        args[0] = NOSTORE;
        args[1] = BACKLIGHT_LED;
        args[2] = id;
        args[3..3 + extra.len()].copy_from_slice(extra);
        Command::extended(0x02, &args)
    };
    match lighting.effect {
        Effect::Off => out.push(effect(0x00, 6, &[])),
        Effect::Static => out.push(effect(0x01, 9, &[0, 0, 1, r, g, b])),
        Effect::Breathing => out.push(effect(0x02, 9, &[1, 0, 1, r, g, b])),
        Effect::Spectrum => out.push(effect(0x03, 6, &[])),
        Effect::Wave => out.push(effect(
            0x04,
            6,
            &[if lighting.reverse { 2 } else { 1 }, 0x28],
        )),
        Effect::PerKey => {
            for (row, slots) in TARTARUS_ROWS.iter().enumerate() {
                let stop = slots.iter().rposition(Option::is_some).unwrap();
                let mut args = vec![0, 0, row as u8, 0, stop as u8];
                for slot in &slots[..=stop] {
                    args.extend_from_slice(&slot.map(|s| lighting.colors[s]).unwrap_or([0; 3]));
                }
                out.push(Command::extended(0x03, &args));
            }
            let mut args = vec![0; 12];
            args[2] = 0x08;
            out.push(Command::extended(0x02, &args));
        }
    }
    out.push(Command::extended(
        0x04,
        &[
            NOSTORE,
            ZERO_LED,
            (u16::from(lighting.brightness) * 255 / 100) as u8,
        ],
    ));
    out
}

/// Commands that realise `lighting` on `model`, or none for a keypad without RGB control.
pub fn lighting_commands(model: Model, lighting: &Lighting) -> Result<Vec<Command>> {
    ensure!(
        lighting.brightness <= 100 && lighting.colors.len() == model.family().led_count(),
        "invalid lighting settings for {}",
        model.name()
    );
    Ok(match model {
        Model::Original => vec![],
        Model::Chroma => chroma_commands(lighting),
        Model::TartarusV2 => tartarus_commands(lighting),
    })
}

pub fn apply_lighting(device: &HidDevice, model: Model, lighting: &Lighting) -> Result<()> {
    for command in lighting_commands(model, lighting)? {
        send(device, &command)?;
    }
    Ok(())
}

fn send(device: &HidDevice, command: &Command) -> Result<()> {
    ensure!(command.args.len() <= 80, "lighting payload too large");
    device.send_feature_report(&command.report())?;
    std::thread::sleep(std::time::Duration::from_millis(2));
    let mut response = [0; SIZE + 1];
    let n = device.get_feature_report(&mut response)?;
    ensure!(
        n == SIZE + 1 && response[0] == 0,
        "unexpected lighting response length {n}"
    );
    let r = &response[1..];
    ensure!(r[88] == checksum(r), "lighting response checksum mismatch");
    ensure!(
        r[1] == command.transaction && r[6] == command.class && r[7] == command.command,
        "lighting response does not match request"
    );
    ensure!(
        r[0] == 2,
        "lighting command {:02x}/{:02x} returned status {:02x}",
        command.class,
        command.command,
        r[0]
    );
    Ok(())
}

#[cfg(test)]
mod lighting_tests {
    use super::*;
    use crate::app::model::{Family, Profile};
    fn lighting(family: Family, effect: Effect) -> Lighting {
        let mut l = Profile::new(family).lighting;
        l.effect = effect;
        l.color = [10, 20, 30];
        l.brightness = 100;
        l
    }
    #[test]
    fn chroma_wire_format_is_unchanged() {
        let l = lighting(Family::Orbweaver, Effect::Static);
        let c = lighting_commands(Model::Chroma, &l).unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0], Command::standard(0x3f, 0x0a, &[6, 10, 20, 30]));
        assert_eq!(c[1], Command::standard(0xff, 0x03, &[0, 5, 255]));
        let report = c[0].report();
        assert_eq!(
            &report[..13],
            &[0, 0, 0x3f, 0, 0, 0, 4, 3, 0x0a, 6, 10, 20, 30]
        );
        assert_eq!(report[89], checksum(&report[1..]));
        assert!(lighting_commands(Model::Original, &l).unwrap().is_empty());
        let per_key =
            lighting_commands(Model::Chroma, &lighting(Family::Orbweaver, Effect::PerKey)).unwrap();
        assert_eq!(per_key.len(), 6);
        assert_eq!(per_key[3].args[..4], [0xff, 4, 1, 6]);
    }
    #[test]
    fn tartarus_uses_the_extended_matrix_class_and_volatile_storage() {
        let l = lighting(Family::TartarusV2, Effect::Static);
        let c = lighting_commands(Model::TartarusV2, &l).unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(
            c[0],
            Command::extended(0x02, &[0, 5, 1, 0, 0, 1, 10, 20, 30])
        );
        assert_eq!(c[1], Command::extended(0x04, &[0, 0, 255]));
        let report = c[0].report();
        assert_eq!(
            &report[..18],
            &[
                0, 0, 0x1f, 0, 0, 0, 9, 0x0f, 0x02, 0, 5, 1, 0, 0, 1, 10, 20, 30
            ]
        );
        assert_eq!(report[89], checksum(&report[1..]));
        for (effect, expected) in [
            (Effect::Off, vec![0, 5, 0, 0, 0, 0]),
            (Effect::Spectrum, vec![0, 5, 3, 0, 0, 0]),
            (Effect::Breathing, vec![0, 5, 2, 1, 0, 1, 10, 20, 30]),
            (Effect::Wave, vec![0, 5, 4, 1, 0x28, 0]),
        ] {
            let c = lighting_commands(Model::TartarusV2, &lighting(Family::TartarusV2, effect))
                .unwrap();
            assert_eq!(
                c[0],
                Command::extended(0x02, &expected),
                "{}",
                effect.label()
            );
        }
        let mut reverse = lighting(Family::TartarusV2, Effect::Wave);
        reverse.reverse = true;
        reverse.brightness = 50;
        let c = lighting_commands(Model::TartarusV2, &reverse).unwrap();
        assert_eq!(c[0].args[3], 2);
        assert_eq!(c[1].args[2], 127);
    }
    #[test]
    fn tartarus_per_key_frames_follow_the_four_by_six_matrix() {
        let mut l = lighting(Family::TartarusV2, Effect::PerKey);
        for (i, c) in l.colors.iter_mut().enumerate() {
            *c = [i as u8 + 1, 0, 0];
        }
        let c = lighting_commands(Model::TartarusV2, &l).unwrap();
        assert_eq!(c.len(), 6);
        for (row, cmd) in c[..3].iter().enumerate() {
            assert_eq!(cmd.class, 0x0f);
            assert_eq!(cmd.command, 0x03);
            assert_eq!(cmd.args[..5], [0, 0, row as u8, 0, 4]);
            assert_eq!(cmd.args.len(), 5 + 5 * 3);
            for col in 0..5 {
                assert_eq!(cmd.args[5 + col * 3], (row * 5 + col) as u8 + 1);
            }
        }
        let last = &c[3];
        assert_eq!(last.args[..5], [0, 0, 3, 0, 5]);
        assert_eq!(last.args.len(), 5 + 6 * 3);
        // Keys 16–19, then the scroll wheel (slot 20), then key 20 (slot 19).
        let reds: Vec<_> = (0..6).map(|col| last.args[5 + col * 3]).collect();
        assert_eq!(reds, vec![16, 17, 18, 19, 21, 20]);
        assert_eq!(
            c[4],
            Command::extended(0x02, &[0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0])
        );
        assert_eq!(c[5].command, 0x04);
    }
    #[test]
    fn lighting_must_match_the_models_layout() {
        let orb = lighting(Family::Orbweaver, Effect::Static);
        assert!(lighting_commands(Model::TartarusV2, &orb).is_err());
        let tart = lighting(Family::TartarusV2, Effect::Static);
        assert!(lighting_commands(Model::Chroma, &tart).is_err());
        assert!(lighting_commands(Model::Original, &tart).is_err());
        let (adapted, changed) = orb.adapted(Family::TartarusV2);
        assert!(changed);
        assert_eq!(
            lighting_commands(Model::TartarusV2, &adapted)
                .unwrap()
                .len(),
            2
        );
        let mut bright = tart;
        bright.brightness = 101;
        assert!(lighting_commands(Model::TartarusV2, &bright).is_err());
    }
}

#[cfg(test)]
mod hardware_tests {
    #[test]
    #[ignore = "changes the connected keypad's lighting; run explicitly"]
    fn lighting_acknowledgments() {
        use crate::app::model::{Effect, Profile};
        let api = hidapi::HidApi::new().unwrap();
        let model = crate::device::connected_model(&api).unwrap();
        assert!(
            model.supports_rgb(),
            "RGB test requires a Chroma or Tartarus V2"
        );
        let handle = crate::device::select(&api, 2)
            .unwrap()
            .open_device(&api)
            .unwrap();
        let firmware = super::query_firmware_for(&handle, model).unwrap();
        println!(
            "{}: firmware {}.{}",
            model.name(),
            firmware.major,
            firmware.minor
        );
        let mut lighting = Profile::new(model.family()).lighting;
        let mut failures = vec![];
        for effect in [
            Effect::Off,
            Effect::Wave,
            Effect::Spectrum,
            Effect::Breathing,
            Effect::PerKey,
            Effect::Static,
        ] {
            lighting.effect = effect;
            match super::apply_lighting(&handle, model, &lighting) {
                Ok(()) => println!("{}: acknowledged", effect.label()),
                Err(e) => failures.push(format!("{}: {e}", effect.label())),
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
