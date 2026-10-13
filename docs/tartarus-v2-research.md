# Tartarus V2: hardware and protocol research

Research date: October 13, 2026, on the same Apple Silicon Mac used for the Orbweaver work. The unit was connected but its HID interfaces were not opened during research: the shell used for inspection does not hold Input Monitoring, so every observation below comes from the I/O Registry or from public driver sources.

## Identity

The connected unit reports **Razer Tartarus V2**, USB **`1532:022b`** (vendor 5426, product 555), USB device release `0x0200`. Razer's product code is RZ07-0227. [OpenRazer][openrazer-ids] and [OpenRGB][openrgb-ids] use the same product ID. Neighbouring keypads are deliberately **not** matched: Tartarus `1532:0201`, Tartarus Chroma `1532:0208`, Tartarus Pro `1532:0244`, Nostromo `1532:0111`.

## Interfaces

Evidence: [filtered I/O Registry capture](evidence/tartarus-v2-ioreg-2026-10-13.json).

| Interface | USB class/subclass/protocol | HID role | Max input | Max output | Max feature |
| --- | --- | --- | ---: | ---: | ---: |
| 0 | 3 / 1 / 1 (boot keyboard) | Keyboard | 8 bytes | 1 byte | 0 bytes |
| 1 | 3 / 0 | Keyboard, consumer, system control, two vendor collections | 16 bytes | 1 byte | 1 byte |
| 2 | 3 / 0 | Pointer plus 90-byte vendor feature report | 8 bytes | 0 bytes | 90 bytes |

The report descriptors of interfaces 0 and 1 are **byte-for-byte identical** to the Orbweaver and Orbweaver Chroma descriptors already verified for this app, so the existing boot-keyboard and extended-keyboard decoders apply unchanged. Interface 2 differs only in declaring **five** button bits instead of three (the Orbweaver declares three plus five bits of padding). The report is still eight bytes: buttons, two vendor bytes, wheel, 16-bit X and Y. The 90-byte vendor feature report is the same Razer control channel used for firmware queries and lighting on the Chroma; macOS reports it on interface 2 exactly as on the Orbweaver.

A unit test pins these three descriptors and asserts their classification.

## Physical controls and factory output

Razer specifies 32 programmable controls: 19 keys, the key-20 thumb paddle, an 8-way thumb pad, a thumb button, and a 3-way scroll wheel. [Razer's manual][manual] labels the keys `01`–`05` = `1`–`5`, `06`–`10` = Tab Q W E R, `11`–`15` = Caps A S D F, `16`–`19` = Shift Z X C, key `20` = Space. Razer's keypad FAQ describes the thumb pad as navigation (arrow keys) with the buttons beside it mapped to Alt and Space. These are the factory outputs assumed for the default Tartarus V2 profile:

| Control | Factory input |
| --- | --- |
| 01–05 | `1 2 3 4 5` |
| 06–10 | `tab q w e r` |
| 11–15 | `caps_lock a s d f` |
| 16–19 | `shift z x c` |
| 20 (thumb paddle) | `space` |
| Thumb button | `option` (left Alt) |
| Thumb pad | arrow keys; diagonals combine two arrows |
| Scroll wheel | mouse wheel on interface 2; not captured |

**Not yet measured on this unit.** No key presses were recorded during this research. If a control differs, start remapping, select the control, use **Learn from device**, and press it; the learned usage replaces the assumed one in that profile. A shared `monitor` recording also shows the raw usages.

The scroll wheel reports through the pointer interface. The capture helper seizes only interfaces 0 and 1, so the wheel keeps its factory scrolling while remapping is active and is not remappable. The lighting worker opens interface 2 in shared mode, which it could not do if the helper seized that interface, so wheel capture was deliberately left out of this change.

## Lighting protocol

OpenRazer and OpenRGB both drive the Tartarus V2 with Razer's newer **extended matrix** command class `0x0f` and transaction ID **`0x1f`** (the Chroma uses class `0x03` with `0x3f`/`0xff`). The report is the usual 90-byte structure: status, transaction, reserved, data size, command class, command ID, 80 argument bytes, XOR checksum of bytes 2–87, reserved. Sources: [OpenRazer `razerkbd_driver.c`][openrazer-driver] (Tartarus V2 branches), [OpenRazer `razerchromacommon.c`][openrazer-common], and [OpenRGB `RazerController.cpp`][openrgb-controller]. The app re-encodes the commands independently rather than copying GPL source.

| Operation | Command | Arguments |
| --- | --- | --- |
| Off | `0x0f/0x02`, size 6 | `storage, led 0x05, effect 0x00` |
| Static | `0x0f/0x02`, size 9 | `storage, 0x05, 0x01, 0, 0, 1, r, g, b` |
| Breathing (one color) | `0x0f/0x02`, size 9 | `storage, 0x05, 0x02, 1, 0, 1, r, g, b` |
| Spectrum | `0x0f/0x02`, size 6 | `storage, 0x05, 0x03` |
| Wave | `0x0f/0x02`, size 6 | `storage, 0x05, 0x04, direction 1/2, speed 0x28` |
| Custom frame row | `0x0f/0x03`, size 5 + 3·columns | `0, 0, row, first column, last column, rgb…` |
| Show custom frame | `0x0f/0x02`, size 12 | `0, 0, 0x08` |
| Brightness | `0x0f/0x04`, size 3 | `storage, led 0x00, 0–255` |

OpenRazer passes `storage = 0x01` (saved to the keypad); OpenRGB passes `0x00` (volatile) for the same commands. This app uses **`0x00`** so nothing is written to the keypad's flash, matching its Orbweaver behaviour. Brightness addresses LED `0x00` on this model in both sources, while effects address the backlight LED `0x05`.

### LED matrix

OpenRGB's [Tartarus V2 layout][openrgb-layout] declares a **4 × 6** matrix:

| Row | Columns 0–4 | Column 5 |
| --- | --- | --- |
| 0 | keys 01–05 | unused |
| 1 | keys 06–10 | unused |
| 2 | keys 11–15 | unused |
| 3 | keys 16–19, **scroll wheel** (column 4) | **key 20** |

The profile therefore stores 21 colors: keys 01–19, key 20, then the wheel. The thumb button and thumb pad have no LEDs in either source.

### Firmware query

The Chroma answers the firmware query (`0x00/0x81`) on transaction `0xff`. OpenRGB sends every request to the Tartarus V2 with `0x1f`; OpenRazer's firmware read for this model uses the default `0xff`. The app tries `0x1f` first and falls back to `0xff`.

## Verification status

| Item | Status |
| --- | --- |
| USB identity and three HID interfaces | Observed in the I/O Registry on October 13, 2026 |
| Keyboard descriptors equal to the Orbweaver | Observed; pinned by unit test |
| Factory key usages per control | **Assumed from Razer documentation; not yet recorded** |
| Exclusive capture and key outputs | Pending: blocked on Accessibility consent for the rebuilt app |
| Shared HID access and firmware query | Verified October 13, 2026: the installed app opened interface 2 and reported **Firmware 1.0** |
| Lighting acknowledgment | **Pending**: check the status line after Save & apply, or run `cargo test --locked -- --ignored lighting_acknowledgments` from a terminal with Input Monitoring |
| LED coordinates | From OpenRGB; visual confirmation pending |

No vendor report was sent to the unit and no setting on it was changed during this research.

[openrazer-ids]: https://github.com/openrazer/openrazer/blob/master/driver/razerkbd_driver.h
[openrazer-driver]: https://github.com/openrazer/openrazer/blob/master/driver/razerkbd_driver.c
[openrazer-common]: https://github.com/openrazer/openrazer/blob/master/driver/razerchromacommon.c
[openrgb-ids]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/master/Controllers/RazerController/RazerDevices.h
[openrgb-layout]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/master/Controllers/RazerController/RazerDevices.cpp
[openrgb-controller]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/master/Controllers/RazerController/RazerController/RazerController.cpp
[manual]: https://www.manua.ls/razer/tartarus-v2/manual
