//! Firmware identity only; no arbitrary vendor writes or persistent changes.
//! Protocol reference: OpenRazer 6820f9d, linked in docs/orbweaver-research.md.
use anyhow::{Result, ensure};
use hidapi::HidDevice;
use serde::Serialize;

const SIZE: usize = 90;
fn checksum(bytes: &[u8]) -> u8 {
    bytes[2..88].iter().fold(0, |a, b| a ^ b)
}

fn firmware_request() -> [u8; SIZE] {
    let mut report = [0; SIZE];
    report[1] = 0xff;
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

fn parse_response(bytes: &[u8]) -> Result<Firmware> {
    ensure!(
        bytes.len() == SIZE,
        "expected 90 response bytes, received {}",
        bytes.len()
    );
    ensure!(bytes[88] == checksum(bytes), "response checksum mismatch");
    ensure!(
        bytes[1] == 0xff,
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

pub fn query_firmware(device: &HidDevice) -> Result<Firmware> {
    // HIDAPI requires a leading zero report ID for this unnumbered feature report.
    let mut framed = [0; SIZE + 1];
    framed[1..].copy_from_slice(&firmware_request());
    device.send_feature_report(&framed)?;
    std::thread::sleep(std::time::Duration::from_micros(600));
    let mut response = [0; SIZE + 1];
    let n = device.get_feature_report(&mut response)?;
    ensure!(
        n == SIZE + 1 && response[0] == 0,
        "unexpected HIDAPI framing: {} bytes",
        n
    );
    parse_response(&response[1..])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn valid() -> [u8; 90] {
        let mut r = firmware_request();
        r[0] = 2;
        r[8] = 1;
        r[9] = 3;
        r[88] = checksum(&r);
        r
    }
    #[test]
    fn request_wire_format() {
        let r = firmware_request();
        assert_eq!(&r[..8], &[0, 255, 0, 0, 0, 2, 0, 129]);
        assert_eq!(r[88], 0x83);
    }
    #[test]
    fn accepts_only_valid_identity() {
        let r = valid();
        let f = parse_response(&r).unwrap();
        assert_eq!((f.major, f.minor), (1, 3));
        assert!(parse_response(&r[..89]).is_err());
        for index in [7, 8, 88] {
            let mut bad = r;
            bad[index] ^= 1;
            assert!(parse_response(&bad).is_err());
        }
        let mut bad = r;
        bad[7] = 0x82;
        bad[88] = checksum(&bad);
        assert!(parse_response(&bad).is_err());
        for status in [0, 1, 3, 4, 5] {
            let mut bad = r;
            bad[0] = status;
            assert!(parse_response(&bad).is_err());
        }
    }
}

/// Restricted lighting commands for 1532:0207. Protocol encoded independently.
pub fn apply_lighting(device: &HidDevice, lighting: &crate::app::model::Lighting) -> Result<()> {
    use crate::app::model::Effect;
    ensure!(
        lighting.brightness <= 100 && lighting.colors.len() == 20,
        "invalid lighting settings"
    );
    let [r, g, b] = lighting.color;
    match lighting.effect {
        Effect::Static => {
            lighting_command(device, 0x3f, 0x0a, &[6, r, g, b])?;
        }
        Effect::Spectrum => {
            lighting_command(device, 0xff, 0x0a, &[4])?;
        }
        Effect::Breathing => {
            lighting_command(device, 0xff, 0x0a, &[3, 1, r, g, b, 0, 0, 0])?;
        }
        Effect::Wave => {
            lighting_command(
                device,
                0xff,
                0x0a,
                &[1, if lighting.reverse { 2 } else { 1 }],
            )?;
        }
        Effect::Off => {
            lighting_command(device, 0xff, 0x0a, &[0])?;
        }
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
                lighting_command(device, 0x3f, 0x0b, &args)?;
            }
            lighting_command(device, 0x3f, 0x0a, &[5, 0])?;
        }
    }
    // Volatile brightness; profiles are owned by this app, not written to flash.
    lighting_command(
        device,
        0xff,
        0x03,
        &[0, 5, (u16::from(lighting.brightness) * 255 / 100) as u8],
    )?;
    Ok(())
}
fn lighting_command(device: &HidDevice, transaction: u8, command: u8, args: &[u8]) -> Result<()> {
    ensure!(args.len() <= 80, "lighting payload too large");
    let mut report = [0; SIZE + 1];
    report[2] = transaction;
    report[6] = args.len() as u8;
    report[7] = 3;
    report[8] = command;
    report[9..9 + args.len()].copy_from_slice(args);
    report[89] = checksum(&report[1..]);
    device.send_feature_report(&report)?;
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
        r[1] == transaction && r[6] == 3 && r[7] == command,
        "lighting response does not match request"
    );
    ensure!(
        r[0] == 2,
        "lighting command {command:02x} returned status {:02x}",
        r[0]
    );
    Ok(())
}

#[cfg(test)]
mod hardware_tests {
    #[test]
    #[ignore = "changes the connected Orbweaver lighting; run explicitly"]
    fn lighting_acknowledgments() {
        use crate::app::model::{Effect, Profile};
        let api = hidapi::HidApi::new().unwrap();
        assert!(
            crate::device::connected_model(&api).unwrap().supports_rgb(),
            "RGB test requires Chroma"
        );
        let handle = crate::device::select(&api, 2)
            .unwrap()
            .open_device(&api)
            .unwrap();
        let mut lighting = Profile::default().lighting;
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
            match super::apply_lighting(&handle, &lighting) {
                Ok(()) => println!("{}: acknowledged", effect.label()),
                Err(e) => failures.push(format!("{}: {e}", effect.label())),
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
