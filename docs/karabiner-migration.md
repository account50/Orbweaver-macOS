# World of Warcraft profile migration

Source: the selected `Default profile` in `~/.config/karabiner/karabiner.json`.

Rule: `Razer Orbweaver: World of Warcraft bindings`. All 15 assignments translate to native Orbweaver key actions. Karabiner is read only during this migration; the app does not depend on it.

The source device condition is `1532:0113` (older Orbweaver). The new profile uses the app's connected-device target `1532:0207` (Chroma). The source application condition matches exactly `com.blizzard.worldofwarcraft`; the user explicitly chose to apply these mappings in every application while remapping is active. The WoW-only application condition is therefore intentionally omitted. Other controls keep factory behavior. Current user lighting is preserved; Karabiner contains no lighting configuration.

| Control | Factory input | Assigned output |
| --- | --- | --- |
| 06 | tab | 1 |
| 07 | q | 2 |
| 08 | w | 3 |
| 09 | e | 4 |
| 10 | r | 5 |
| 11 | caps_lock | 6 |
| 12 | a | 7 |
| 13 | s | 8 |
| 14 | d | 9 |
| 15 | f | 0 |
| 16 | shift | minus |
| 17 | z | equal |
| 19 | c | a |
| 20 | v | space |
| Space | space | shift |

Source SHA-256: `08d49a3724207ab873f826ec4b5a50da1ddbc771a2353d0018e26bbc48e34479`

The migrated profile is selected in the local library. The existing Everyday profile remains intact. A backup of the previous library was saved at `/Users/josh/Library/Application Support/Orbweaver/profiles.before-wow-20260905-143124.json`.
