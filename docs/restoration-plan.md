# Rust restoration plan

September 5, 2026. Implementation roadmap, grounded in the [hardware and feature research](orbweaver-research.md).

Current progress: device diagnostics and a native Rust GUI are implemented. The GUI includes independent capture/remapping, eight keymaps, shortcut and sequence editing, profile management, and six lighting modes. Automated input/profile tests pass, and hardware lighting commands are acknowledged. End-to-end mapped output, full control calibration, visual LED placement, game compatibility, and disconnect/sleep acceptance still require physical verification. This is a working implementation milestone, not full legacy feature parity. See [usage and access requirements](../README.md).

The user explicitly requires independence from Karabiner: no configuration integration, event source, framework, virtual device, or installed component is used. Its competing capture services were stopped for this session. Our temporary privileged helper uses HIDAPI/IOKit; the normal-user app owns CoreGraphics output, profiles, and lighting.

## Product boundary

Support the observed original `1532:0113` and Chroma `1532:0207` on Apple Silicon macOS, one connected unit at a time. October 13, 2026: the Tartarus V2 `1532:022b` was added as a second layout family; see [its research notes](tartarus-v2-research.md). Build a reusable Rust input/action engine and a macOS adapter; add other platforms after this device works reliably. Keep configurations local, versioned, and exportable. No Razer account or online service should be required.

Separate physical identity from emitted keys: use internal control IDs `key_01` through `key_20`, `thumb_upper`, `thumb_lower`, and eight named directions. Populate their HID mappings by measurement. Do not assume diagonals produce independent reports; derive logical directions from observed contact combinations when necessary.

## Architecture and remaining targets

| Component | Responsibility |
| --- | --- |
| Device discovery/transport | Match the exact device; distinguish its interfaces; reconnect; serialize vendor requests |
| Input decoder | Convert measured reports into physical press/release transitions |
| Mapping engine | Resolve profile/layer actions; track held inputs and outputs; schedule cancellable macros |
| macOS adapter | Capture only this device; emit keyboard, pointer, and media actions; track active application |
| Lighting engine | Map 20 physical key LEDs to protocol coordinates; manage effects and indicator state |
| Configuration store | Versioned local profiles, macro definitions, atomic saves, import/export |
| Diagnostic CLI / native UI | Explain connection/access failures; visualize inputs; edit and apply configuration |

Evaluate the Rust [`hidapi` crate](https://docs.rs/hidapi/2.6.7/hidapi/) for enumeration and report access. Its API documents that opening by VID/PID alone chooses an unspecified first match when several exist, so select an enumerated path. Direct IOHID integration may be needed for precise macOS capture behavior. [HidApi reference](https://docs.rs/hidapi/2.6.7/hidapi/struct.HidApi.html)

Apple provides exclusive device opening through `kIOHIDOptionsTypeSeizeDevice`; validate it on this device before choosing the capture design. [Apple IOHIDDeviceOpen](https://developer.apple.com/documentation/iokit/1588670-iohiddeviceopen) Check Input Monitoring for capture and the permissions required by the chosen output mechanism. A global keyboard remapper that cannot identify the originating device is not sufficient.

## Milestones and acceptance criteria

### 1. Establish device access

Build a small Rust diagnostic CLI with proposed commands `devices`, `inspect`, and `monitor`. Enumeration must report VID/PID, interface/path, usages, descriptor sizes, and connection state. Monitoring should be explicitly enabled and limited to the Orbweaver.

Acceptance: identify this unit consistently; unplug/replug cleanly; capture presses and releases for all controls with guided user testing; save a measured mapping fixture. Confirm a regular keyboard remains usable throughout. Distinguish access denial from device absence.

### 2. Restore useful input

Implement one local profile and a single layer first. Verify key replacement, modifiers, disabled controls, and mouse actions. Then introduce layer changes with an explicit rule for releasing outputs according to the mapping active when each input was pressed.

Acceptance: one Orbweaver press yields exactly the configured output, without an extra factory keystroke. Regular keyboard input is unchanged. Holding two controls that share an output does not release it early. Disconnect, cancellation, and shutdown release all synthesized held keys. No event feedback loop.

### 3. Add profiles and automation

Expand to the historical profile/layer and macro inventory documented in the research. Store application associations by stable application identity. Add deterministic conflict handling and a visible active profile/layer.

Proposed macro playback modes are once, fixed repetitions, repeat while held, and toggle-until-stopped; these are implementation targets, with exact legacy mode parity still to be checked. Use a monotonic scheduler, bounded resource use, and a cancel action. A zero-delay loop must not monopolize input processing.

Acceptance: configuration round-trips through export/import; automatic and manual profile changes behave predictably; nested momentary layers restore correctly; switching profiles during a macro never leaves a modifier held. Test ordering and cancellation, then measure real timing on the host instead of promising hard real-time precision.

### 4. Restore illumination

First resolve the interface-index discrepancy described in the research using a documented identity query. Keep request/response transactions serialized and validate size, command identity, and status. Treat any uncertain response as unverified rather than announcing success.

Then test one reversible lighting operation and restore the previous observable state where readable. Measure LED coordinates one physical key at a time. Implement hardware effects that work, and synthesize missing effects with custom frames when feasible. Keep layer indicators independent from decorative lighting.

Acceptance: every physical key can show a selected RGB value; no matrix padding appears in the editor; all eight layer states have distinguishable feedback. Exercise effect changes, reconnect, and sleep/wake. Determine whether brightness can be controlled natively or requires scaling custom RGB frames.

### 5. Finish the desktop application

Add a layout-based editor, macro timeline, local profile management, connection status, launch-at-login option, and input diagnostics. Keep the mapping service alive independently of editor windows. Present a usable recovery action if capture or output access is lost.

Acceptance: a fresh local configuration works without internet access; relaunch restores settings; a disconnected keypad is handled cleanly; the user can stop remapping immediately. Verify input in at least a text editor and a target game/application before declaring restoration complete.

## Later compatibility work

| Original area | Proposed handling |
| --- | --- |
| Joystick output | Separate platform feasibility experiment; macOS game compatibility is unproven |
| Windows-specific actions | Preserve in portable config; implement on Windows or offer explicit macOS equivalents |
| Cross-device actions / synchronized lighting | Optional integration layer after one-device reliability |
| Game add-ons / SDK integrations | Rebuild only against identified applications and available APIs |
| Statistics / heatmaps | Optional local counters; disabled initially; avoid collecting typed text |
| Legacy cloud / social features | Local export and optional image export cover practical needs initially |
| Old Synapse file import | Inspect real export samples before claiming compatibility |

Hardware repair, firmware replacement, and persistent onboard programming are separate scopes. The current evidence supports attempting restoration through host software first.

## Implemented architecture decision

The native eframe/egui application and temporary capture helper are two modes of the same Rust executable. Only capture is elevated. A private socket carries device state and a heartbeat, with a three-second capture timeout. CoreGraphics output runs as the normal user with Accessibility consent; no third-party virtual HID device is used. This output path still needs application-specific acceptance, especially for games.

HID interface 2 supports both verified firmware queries and direct lighting writes while keyboard interfaces 0 and 1 are captured. Profile persistence belongs to this app, not the hardware. The GUI is now the primary workflow; the CLI remains available for diagnostics. Future milestones above describe remaining parity and acceptance work, not features claimed by this build.
