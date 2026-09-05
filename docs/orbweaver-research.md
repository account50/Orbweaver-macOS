# Orbweaver Chroma: hardware and capability research

Research date: September 5, 2026. Initial target: this Mac, running macOS 26.6.1 on arm64.

## Finding

The connected unit identifies itself as **Razer Orbweaver Chroma**, USB **`1532:0207`**, corresponding to Razer's **RZ07-01440** product family. This is the Chroma generation introduced in August 2015. The original non-Chroma Orbweaver uses `1532:0113`; its lighting implementation should not be substituted for this device's. Model family is corroborated by [Razer support][support] and [OpenRazer's device list][openrazer]; the underside label, exact retail suffix, and switch variant were not inspected.

It is not electrically absent: macOS has enumerated it and attached its HID drivers. This does not establish that every switch, directional contact, LED, or programmable feature works.

## Direct observations

Evidence: [filtered I/O Registry snapshot](evidence/device-inventory-2026-09-05.json), captured at 13:16 America/Chicago. Only the matching Orbweaver's relevant properties were retained.

| Property | Observed value | Interpretation |
| --- | --- | --- |
| USB vendor/product | 5426 / 519 = `0x1532 / 0x0207` | Exact software matching target |
| Product string | Razer Orbweaver Chroma | Device-reported identity |
| USB specification / link | `bcdUSB=0x0200`; 12,000,000 bits/s | USB 2.0 descriptor; full-speed link |
| Device release field | `bcdDevice=0x0200` | USB release field, **not a verified firmware version** |
| USB serial-string index | `0` | No standard USB serial string advertised; vendor-specific serial query remains untested |
| Location ID | `0x00100000` | Connection-specific, not a permanent identity |
| Input interval property | `1000` on each HID interface | Raw macOS registry value; not a measured latency result |
| Privacy authorization property | `RequiresTCCAuthorization=true` | Enumeration does not prove our future app can open/read the device |
| Keyboard report counter | 20 on interface 0 | OS recorded input reports; content and physical origin were not captured |

Three USB interfaces were observed; `hidutil list` independently showed their keyboard/keyboard/mouse services. Its device and service rows describe the same composite unit, not six Orbweavers.

| Interface | HID role from descriptors | Max input | Max output | Max feature |
| --- | --- | ---: | ---: | ---: |
| 0 | Boot keyboard | 8 bytes | 1 byte | 0 bytes |
| 1 | Keyboard, consumer/media, system control, additional reports | 16 bytes | 1 byte | 1 byte |
| 2 | Mouse/pointer plus vendor-defined feature report | 8 bytes | 0 bytes | 90 bytes |

Descriptor interpretation: interface 0 has modifier bits plus six ordinary key slots. Interface 1 includes report ID 1 with modifier bits and fourteen key slots, and report IDs 2–5 for other collections. Interface 2 advertises pointer, wheel, button fields, and a 90-byte vendor feature report. These are report formats, not evidence of a physical mouse sensor, wheel, analog stick, or tested rollover performance. The full descriptor hex is preserved in the evidence file.

`system_profiler SPUSBDataType -json` returned an empty list during this inspection. `ioreg` and `hidutil` supplied the positive identification; an empty System Profiler result alone must not be treated as disconnection.

## Original physical capabilities

Razer specified 20 mechanical keys, an eight-direction thumb pad, an upper thumb button, and a lower space actuator. The advertised 30 assignments reconcile as **20 + 8 + 2**, rather than 30 mechanical switches. The upper and lower thumb buttons defaulted to Alt and Space; the pad defaulted to navigation. All were customizable. [Razer support][support]

The standard Chroma launched with 50 g green mechanical switches, individually customizable RGB lighting with approximately 16.8 million colors, a braided cable, and adjustable hand/thumb/palm components. Nominal weight was 395 g; dimensions approximately 202 × 154 × 55 mm. Razer labels the dimension axes inconsistently across publications. These are model specifications, not measurements of this unit. [2015 launch announcement][launch]

## Original software inventory

This compact inventory summarizes the [Chroma master guide][manual]. Page numbers below are the guide's printed numbers, starting at zero; PDF viewer page numbers are one higher.

| Area | Historical capabilities | Pages |
| --- | --- | --- |
| Profiles | Create, rename, copy, delete, import/export; application linking; cloud autosave | 12 |
| Layers | Eight keymaps; indicator feedback; profile switching with onscreen feedback | 12, 17 |
| Assignments | Default, keyboard/modifier combinations, disable, macros, application/URL launch, inter-device actions | 15–18 |
| Mouse | Left/right/middle, double-click, back/forward, four scroll directions | 16 |
| Media/system | Playback, output/microphone volume and mute; Windows shortcuts/Charms | 18 |
| Joystick | Button and stick-direction emulation | 17 |
| Lighting | Spectrum default, directional wave, static, one/two-color breathing, reactive fade, ripple; regional effect layers; compatible-device synchronization; indicators separate | 19–21 |
| Macro editor | Keyboard/mouse recording; recorded/fixed/no delays; millisecond entry precision; insert/edit/delete/reorder; copy/import/export | 22–26 |
| Extras | Game add-ons/account association; opt-in usage statistics, heatmaps, social sharing | 27–34 |

Razer advertised unrestricted profile counts and macro lengths, subject in practice to host resources. The launch also described synchronized Chroma devices and game-driven lighting through its SDK. [Launch announcement][launch]

Keymap switching could be momentary while held or advance on presses. [Razer keymap definition][keymaps] The original joystick function was Windows-only; mapping digital controls to stick directions should not be mistaken for physical analog sensing. [Chroma FAQ][support]

## Legacy software context

This is a Synapse 2 device. Razer lists the old macOS support range only through Mojave (10.14), far behind this Mac. Synapse 2 cloud features ended October 28, 2025; Razer says existing PC installations can retain local settings. This does not mean every Orbweaver stopped sending input on that date. [Razer Mac support notice][mac-support]

We should keep profiles and macros locally and make account-free operation the default. That is a proposed product decision, not a claim about onboard storage. Profile persistence in the device itself has not been established.

## Existing implementation evidence

OpenRazer is a Linux implementation, not a macOS backend. Its public guidance directs key rebinding to a separate application. [OpenRazer][openrazer-site]

Source inspection was pinned to commit `6820f9da169d354bc7e6e93a0aa8683a6bb75792`:

- [Chroma device class][device-code]: exact USB ID, custom frames, off/static/spectrum/wave/breathing, three indicator channels, and software ripple. It declares a **5 × 22 protocol matrix**, which must not become a 110-key UI. The dedicated Orbweaver key manager is commented out. Native reactive and brightness setters are absent from this class's method list; inherited behavior and actual hardware support require investigation.
- [Keyboard driver][driver-code]: the Orbweaver Chroma branch exposes those core lighting operations and indicator controls. Native reactive/brightness controls are not in that branch. Porting the driver is insufficient to recreate Synapse's full input behavior.
- [Common report structure][report-code] and [transport/checksum][transport-code]: 90-byte messages with an 8-byte header, 80 argument bytes, XOR checksum at byte 88 over bytes 2–87, and a final reserved byte. Requests use HID SET_REPORT (`0x21/0x09`, value `0x0300`) followed by GET_REPORT (`0xA1/0x01`, value `0x0300`).

**Transport uncertainty:** the pinned Linux driver selects interface index `1` and a 600 μs request/response delay for this product via its default path. Locally, the 90-byte feature descriptor is on interface `2`. Do not silently equate descriptor location, control-transfer interface index, HID report ID, and HIDAPI path. Resolve this with an explicit, non-mutating identity-query experiment before lighting writes. HIDAPI's report-ID framing also needs separate handling.

The inspected driver files carry GPL-2.0-or-later headers. Record provenance and choose the application's licensing approach before incorporating source code.

## Still unverified

- Physical key-to-usage mapping, diagonals, simultaneous presses, repeat behavior, and worn contacts.
- Application-level read access, exclusive capture, and suppression of default keystrokes on this macOS version.
- Working control transport; firmware identity; actual lighting/indicator behavior and physical LED coordinates.
- What survives disconnect, power loss, sleep, and application exit.
- Native brightness/reactive support versus effects synthesized by the host.
- Original macro playback options beyond the documented editor; compatibility of old Synapse export files.

No device was seized, remapped, reset, flashed, or sent vendor control reports during this research.

## Follow-up: first Rust CLI, September 5, 2026

The initial research above describes the state before implementation. Subsequent live checks with the CLI established:

- Shared handles open on all three interfaces after temporarily excluding this Orbweaver from Karabiner. See [CLI descriptor inspection](evidence/cli-inspect-2026-09-05.json).
- A firmware identity request through HIDAPI **interface 2** returned **1.2**, with valid response command, transaction, length, status, and checksum. See [firmware response](evidence/firmware-query-2026-09-05.json). This is a working macOS transport route; interface 1 was not tested with a vendor request. It does not invalidate the Linux driver's different routing.
- Karabiner logs showed both keyboard interfaces seized. The device-specific exclusion used for testing was removed and the original configuration restored after each trial.
- Input Monitoring was granted. Exclusive keyboard opening still failed as an ordinary user with `0xE00002C1` (privilege violation), and succeeded under an elevated process. [Karabiner's implementation documentation](https://github.com/pqrs-org/Karabiner-Elements/blob/main/DEVELOPMENT.md) independently describes its root capture service.
- All three interfaces opened exclusively in the [successful access test](evidence/exclusive-access-2026-09-05.jsonl). The first [longer capture](evidence/input-capture-2026-09-05.jsonl) recorded zero input reports; a subsequent [user-operated capture](evidence/input-capture-verified-2026-09-05.jsonl) recorded **73 reports**, all on interface 0, decoding to **40 presses and 40 releases** with no errors. The final report was empty. Fourteen distinct key usages appeared: D, E, Q, R, S, W, 4, Tab, Space, four arrow keys, and left Alt. Arrow combinations included northeast, northwest, and southwest diagonals. This establishes real keyboard/combination delivery, but does not map every physical key label or measure all rollover limits. The user confirmed completing the trial; application-level suppression was not separately instrumented.

The failed unelevated capture is retained as [diagnostic evidence](evidence/input-trial-2026-09-05.jsonl). The CLI now rejects unelevated exclusive keyboard monitoring before opening handles, and stops with all handles released if any required interface fails. Lighting, mappings, and firmware remain unchanged.

[support]: https://mysupport.razer.com/app/answers/detail/a_id/3619/kw/orbweaver
[launch]: https://www.razer.com/newsroom/product-news/infinite-control-and-16-8-million-color-options-with-the-razer-orbweaver-chroma-mechanical-gaming-keypad
[manual]: https://dl.razerzone.com/master-guides/OrbweaverChroma/OrbweaverChromaOMG-ENG.pdf
[keymaps]: https://mysupport.razer.com/app/answers/detail/a_id/939/~/what-is-a-keymap
[mac-support]: https://mysupport.razer.com/app/answers/detail/a_id/1500/kw/Synapse%202.0.
[openrazer]: https://github.com/openrazer/openrazer/tree/6820f9da169d354bc7e6e93a0aa8683a6bb75792
[openrazer-site]: https://openrazer.github.io/
[device-code]: https://github.com/openrazer/openrazer/blob/6820f9da169d354bc7e6e93a0aa8683a6bb75792/daemon/openrazer_daemon/hardware/keyboards.py#L265-L302
[driver-code]: https://github.com/openrazer/openrazer/blob/6820f9da169d354bc7e6e93a0aa8683a6bb75792/driver/razerkbd_driver.c
[report-code]: https://github.com/openrazer/openrazer/blob/6820f9da169d354bc7e6e93a0aa8683a6bb75792/driver/razercommon.h#L124-L136
[transport-code]: https://github.com/openrazer/openrazer/blob/6820f9da169d354bc7e6e93a0aa8683a6bb75792/driver/razercommon.c

## Independent GUI implementation follow-up

The user selected a standalone native Rust design, with no Karabiner component. Its competing capture services were stopped; the installed third-party system extension is not used by this app. Our helper captures only the Orbweaver keyboard interfaces and releases them if the GUI disconnects or its heartbeat expires. CoreGraphics supplies normal-user keyboard output.

[OpenRGB's Orbweaver mapping](https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/223ec45a75b59dc8aacf1be0374d7cc66cc2a0da/Controllers/RazerController/RazerController.cpp) identifies the RGB coordinates: rows 1–3 use columns 1–5, while row 4 uses columns 1, 3, 4, 5, 6. Row 0 and other positions are padding. Our independently encoded custom frames use transaction 0x3f and the standard matrix command. The same source describes standard brightness support; the connected unit acknowledged the volatile backlight-brightness command (storage 0, LED 5).

Direct device tests returned checksum-valid, matching success responses for off, wave, spectrum, single-color breathing, per-key frames, static, and brightness. Testing ended at solid mint and 80% brightness. Visual verification of every LED and physical key label remains distinct from protocol acknowledgment.


## Original Orbweaver compatibility verification

The user's second unit reports **Razer Orbweaver, 1532:0113**, with keyboard interfaces 0 and 1 and pointer/vendor interface 2. All three shared handles opened. Each report descriptor matches the Chroma unit byte-for-byte; see [original device capture](evidence/original-inspect-2026-09-05.json). No new decoder or profile conversion is needed for these two observed revisions. Exclusive access to all three interfaces was also obtained, but the timed physical-input trial returned zero reports, so live key testing remains pending.

The original model's 20 keys, thumb buttons, and eight-way pad are consistent with [Razer's original Orbweaver specification](https://mysupport.razer.com/app/answers/detail/a_id/3615). Profiles remain model-independent. The app accepts exactly one supported unit at a time and prevents Chroma RGB commands from reaching the original. Original lighting and firmware protocol changes are outside this key-mapping update.

The rebuilt GUI identified USB 1532:0113 and reached ACTIVE with the World of Warcraft profile selected. The same executable's capture helper was running, and the saved two-profile library remained byte-for-byte unchanged (SHA-256 `66352e6f99e9364e81560edc8274f80efc596f1b99f69816caa99373aaa4c6f1`). macOS Accessibility had to be refreshed through System Settings because the local build's signature changed. During a requested physical test of controls 06–10, the GUI's initially empty local test field was observed displaying `12345` followed by additional numbers and punctuation while remapping remained ACTIVE. This establishes the visible requested output sequence; no new raw input recording was collected, and exhaustive original-unit control testing remains pending.

The user subsequently confirmed the original-unit mapping test: “Works now.”
