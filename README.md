# Orbweaver 2.0

An independent native Rust configuration app for the **Razer Orbweaver (`1532:0113`) and Orbweaver Chroma (`1532:0207`)** on Apple Silicon macOS. No Razer account, Karabiner, virtual keyboard driver, browser runtime, or network service is required.

## Open the app

```sh
./scripts/build-app.sh
open dist/Orbweaver.app
```

You can move `dist/Orbweaver.app` into Applications. Running `cargo run --locked` also opens the GUI, but the app bundle provides a clearer identity in macOS permission settings.

1. Select a profile and click any of the 20 keys, two thumb buttons, or eight thumb-pad directions.
2. Assign a key/shortcut, a once-per-press sequence, a keymap switch, factory input, or no output. Eight keymaps are available per profile; switches can be momentary or stay selected.
3. On Chroma, use **Lighting** to choose solid RGB, individual key colors, spectrum, breathing, wave, or off, and adjust brightness. The original model retains those settings for your next Chroma connection.
4. **Save & apply** writes the profile locally and sends its lighting to the device. A failed hardware operation is displayed; saving a profile does not mean the hardware accepted it.
5. **Start remapping** enables capture. Allow Orbweaver in macOS **Accessibility** for mapped output. If macOS denies HID access, use **Set up Input Monitoring** on **Device & test**. Both setup buttons are provided there. macOS may ask for administrator access to start the temporary capture helper. Other remappers must release this keypad first.
6. Use **Device & test** to try mappings in a local text field. **Stop remapping** restores factory input. Keep the app running for mappings to work; quitting stops capture.

Profiles support create, rename, duplicate, delete, import, and export. Saves are validated and replaced atomically at `~/Library/Application Support/Orbweaver/profiles.json`. Import/export files contain one profile. A damaged existing library is protected from overwrite; the app allows exporting a recovery profile instead.

Locally built app signatures change when rebuilding. An enabled Settings switch may still refer to an old signature. If the app is denied access after rebuilding, remove only Orbweaver's stale entry, restart the app, use its setup button to request access again, and enable that new entry. Keep other applications' permissions unchanged. A stable signing identity is a future distribution requirement.

The editor starts from a factory-layout mapping. **Learn from device** captures one physical control while suppressing output; press and release that control, then save. Default diagonals combine cardinal keys; assigning a diagonal overrides those cardinals. Direction changes settle for 25 ms before mapping. Key names represent US physical key positions; the active macOS keyboard layout determines resulting characters. Held keyboard outputs repeat after 500 ms at approximately 33 ms intervals. Sequences accept names such as `command+c, command+v`, with configurable spacing. They finish after release, and are cancelled by a profile change or stopping remapping.

## Switching between Orbweavers

Connect one Orbweaver at a time. Both models use the same saved key profiles, shortcuts, layers, and sequences—including the imported World of Warcraft profile. The app detects device changes every two seconds. Stop remapping before swapping, then click Start remapping after the replacement is detected. Simultaneously connected units are rejected rather than mixing their interfaces.

Both physical units were opened successfully and their three HID descriptors match byte-for-byte. A regression test feeds the same report format through each model's verified descriptor and checks all 15 WoW assignments and releases. The updated GUI reached ACTIVE on the original unit with the World of Warcraft profile selected after refreshing its existing Accessibility entry. During the requested physical-key trial, the local test field displayed `12345` followed by additional mapped numbers. This verifies the visible requested sequence; exhaustive testing of every control on the original unit remains pending. The earlier timed capture received zero reports.

RGB controls apply only to Chroma. The original Orbweaver skips RGB writes and keeps the profile's colors intact for the next Chroma connection. Original-model backlight control is not implemented in this key-mapping compatibility update. Firmware probing remains Chroma-only and is not needed for original-model remapping.

## Independent implementation

The normal-user GUI uses eframe/egui, owns profiles and action scheduling, sends keyboard events through Apple's CoreGraphics, and controls LEDs through HIDAPI. The same executable has a restricted helper mode that exclusively opens only the two recognized Orbweaver keyboard interfaces. It sends decoded states through a private local socket; it cannot execute profile commands. The lighting interface stays available to the GUI. No permanent privileged service is installed.

The helper releases the keypad on socket close, missing heartbeat (three seconds), device removal, or capture failure. Live lifecycle tests measured exit after socket close at 5 ms and after heartbeat loss at 3.006 seconds; both had acquired the real keyboard interfaces. The mapping worker releases its held outputs on ordinary stop, profile changes, and connection failure. A force-killed or crashed GUI cannot guarantee synthesized key-up delivery. Remapping must be started again after disconnect; there is no automatic restart or launch-at-login yet.

## Verified and still pending

The connected unit reported firmware **1.2**. Earlier physical input testing captured **73 reports, 40 presses, and 40 releases**, covering 14 distinct usages, multi-key input, and diagonals. That recording is a decoder regression fixture. Firmware and all six GUI lighting modes, including brightness commands, returned valid success responses on this device. Hardware acknowledgment does not establish visual color accuracy or every LED's physical placement. The user saw a different solid color during GUI testing; a stale macOS permission signature also denied GUI HID access. Replacing the stale entries through System Settings and restarting restored the GUI firmware query. Visual acceptance of the selected RGB value remains pending.

Automated checks cover profile validation, held-output sharing, layer release ordering, macro cancellation, diagonal overrides, recorded HID reports, firmware checksums, and CLI error handling. Complete physical-control calibration, mapped output in target games, unplug/replug, and sleep/wake acceptance remain hands-on checks. CoreGraphics output is suitable for regular Mac applications; games that require raw hardware input may need a future native virtual HID implementation.

This first GUI does not yet implement mouse/media/joystick output, automatic application profiles, macro recording or looping, reactive/ripple lighting, keymap indicator LEDs, or full Synapse import. The [restoration roadmap](docs/restoration-plan.md) retains those targets.

## Diagnostics and development

```sh
cargo run -- devices
cargo run -- inspect
cargo run -- permissions
cargo run -- probe --interface 2
cargo run -- monitor --seconds 30 --output trial.jsonl --label key_01
sudo ./target/release/orbweaver monitor --exclusive --seconds 60
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

Shared diagnostic monitoring leaves factory keystrokes active. Exclusive monitoring requires administrator access; wait for Capture ready. Recordings refuse to overwrite existing files. Ordinary automated tests do not change lighting. This explicit hardware test does change lighting, ending at solid mint / 80% brightness:

```sh
cargo test --locked lighting_acknowledgments -- --ignored --nocapture
```

Research: [hardware and original capabilities](docs/orbweaver-research.md), [roadmap](docs/restoration-plan.md), [observed USB/HID evidence](docs/evidence/device-inventory-2026-09-05.json).
