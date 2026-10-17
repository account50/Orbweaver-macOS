mod engine;
mod lighting;
pub mod model;
mod native;
pub mod session;
use eframe::egui::{self, Color32, RichText, Vec2};
use model::{Action, Effect, Family, Library, Profile};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};
const ACCENT: Color32 = Color32::from_rgb(66, 224, 176);
const MUTED: Color32 = Color32::from_rgb(145, 157, 169);
#[derive(PartialEq)]
enum Page {
    Keys,
    Lighting,
    Device,
}
struct App {
    library: Library,
    saved: Library,
    /// `library` differs from `saved`; maintained at edit time so the status bar need not compare per frame.
    dirty: bool,
    /// Handed to worker threads so they can wake the UI when they have news.
    ctx: egui::Context,
    load_error: bool,
    page: Page,
    selected: usize,
    map: usize,
    status: String,
    device: String,
    connected: bool,
    last_device_refresh: Instant,
    device_model: Option<crate::device::Model>,
    lights: lighting::Worker,
    session: Option<session::Session>,
    ready: bool,
    raw: BTreeSet<u16>,
    live_map: usize,
    learning: Option<usize>,
    test_text: String,
    delete_confirm: bool,
    /// Lighting page: the Tartarus V2 scroll-wheel LED is selected instead of a control.
    wheel_selected: bool,
}
pub fn run() -> anyhow::Result<()> {
    let icon =
        eframe::icon_data::from_png_bytes(include_bytes!("../../assets/orbweaver-icon-256.png"))?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_icon(icon)
            .with_inner_size([1180., 820.])
            .with_min_inner_size([1040., 740.]),
        ..Default::default()
    };
    eframe::run_native(
        "Orbweaver",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))
}
impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut style = (*cc.egui_ctx.style_of(egui::Theme::Dark)).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = Color32::from_rgb(19, 24, 31);
        style.visuals.window_fill = Color32::from_rgb(25, 31, 39);
        style.visuals.selection.bg_fill = Color32::from_rgb(30, 102, 84);
        style.visuals.selection.stroke = egui::Stroke::new(1., ACCENT);
        style.spacing.item_spacing = Vec2::new(10., 10.);
        style.spacing.button_padding = Vec2::new(12., 8.);
        cc.egui_ctx.set_style_of(egui::Theme::Dark, style);
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        let (library, status, load_error) = match model::load() {
            Ok(l) => (
                l,
                "Select a key to configure it. Save & apply activates your changes.".into(),
                false,
            ),
            Err(e) => (
                Library::default(),
                format!(
                    "Could not load profiles: {e}. Existing file is protected; export your edits to recover."
                ),
                true,
            ),
        };
        Self {
            saved: library.clone(),
            library,
            dirty: false,
            ctx: cc.egui_ctx.clone(),
            load_error,
            page: Page::Keys,
            selected: 0,
            map: 0,
            status,
            device: "Looking for a Razer keypad…".into(),
            connected: false,
            last_device_refresh: Instant::now(),
            device_model: None,
            lights: lighting::Worker::new(cc.egui_ctx.clone()),
            session: None,
            ready: false,
            raw: BTreeSet::new(),
            live_map: 0,
            learning: None,
            test_text: String::new(),
            delete_confirm: false,
            wheel_selected: false,
        }
    }
    fn profile(&self) -> &Profile {
        &self.library.profiles[self.library.selected]
    }
    /// Layout of the connected keypad; Orbweaver when nothing is connected.
    fn connected_family(&self) -> Family {
        self.device_model
            .map(crate::device::Model::family)
            .unwrap_or_default()
    }
    /// Add a factory profile for `family` and select it.
    fn new_profile(&mut self, family: Family) {
        if self.library.profiles.len() >= 128 {
            self.status = "Profile limit reached (128).".into();
            return;
        }
        self.cancel_learn();
        let p = Profile {
            name: format!("Profile {}", self.library.profiles.len() + 1),
            ..Profile::new(family)
        };
        self.library.profiles.push(p);
        self.library.selected = self.library.profiles.len() - 1;
        self.mark_edited();
        self.map = 0;
        self.selected = 0;
        self.wheel_selected = false;
    }
    fn profile_mut(&mut self) -> &mut Profile {
        &mut self.library.profiles[self.library.selected]
    }
    /// Call after any change to `library`. Compares once here so reverted edits still read as saved.
    fn mark_edited(&mut self) {
        self.dirty = self.library != self.saved;
    }
    fn apply(&mut self, save: bool) {
        if let Err(e) = self.library.validate() {
            self.status = e.to_string();
            return;
        }
        if save {
            if let Err(e) = model::save(&self.library) {
                self.status = format!("Could not save: {e}");
                return;
            }
            self.saved = self.library.clone();
            self.dirty = false;
        }
        if let Some(session) = &self.session {
            let _ = session
                .tx
                .send(session::Control::Profile(Box::new(self.profile().clone())));
        }
        let _ = self
            .lights
            .tx
            .send(lighting::Request::Apply(self.profile().lighting.clone()));
        self.status = if save {
            "Profile saved. Applying lighting…"
        } else {
            "Profile selected. Applying lighting…"
        }
        .into();
    }
    fn poll(&mut self) {
        while let Ok(reply) = self.lights.rx.try_recv() {
            match reply {
                lighting::Reply::Device(result) => match result {
                    Ok(s) => {
                        self.device = s.description;
                        self.device_model = Some(s.model);
                        self.connected = true;
                    }
                    Err(e) => {
                        self.device = "Keypad unavailable".into();
                        self.connected = false;
                        self.device_model = None;
                        self.status = if e.contains("not permitted") {
                            "macOS blocked HID access. Open Device & test and request Input Monitoring, then refresh the device.".into()
                        } else {
                            e
                        };
                    }
                },
                lighting::Reply::Applied(result) => match result {
                    Ok(message) => {
                        self.status = message;
                    }
                    Err(e) => self.status = format!("Lighting could not be applied: {e}"),
                },
            }
        }
        let events: Vec<_> = self
            .session
            .as_ref()
            .map(|s| s.rx.try_iter().collect())
            .unwrap_or_default();
        for event in events {
            match event {
                session::Event::Ready => {
                    self.ready = true;
                    self.status = "Remapping is active. Only the keypad is captured.".into();
                }
                session::Event::Keys(keys, map) => {
                    self.raw = keys.into_iter().collect();
                    self.live_map = map;
                }
                session::Event::Learned(keys) => {
                    if let Some(i) = self.learning.take() {
                        if keys.len() <= 2
                            && keys.iter().all(|u| model::key_for_usage(*u).is_some())
                        {
                            self.profile_mut().controls[i].inputs = keys;
                            self.mark_edited();
                            self.status =
                                "Input learned. Save & apply to use the new source.".into();
                        } else {
                            self.status="That input contains too many or unsupported keys. Try one control.".into();
                        }
                    }
                }
                session::Event::Ended(message) => {
                    self.session = None;
                    self.ready = false;
                    self.raw.clear();
                    self.learning = None;
                    self.status = message;
                }
            }
        }
    }
    fn start(&mut self) {
        if let Err(e) = self.profile().validate() {
            self.status = e.to_string();
            return;
        }
        if !native::trusted() {
            native::request_access();
            self.page = Page::Device;
            self.status = "Allow Orbweaver in Accessibility, then click Start remapping.".into();
            return;
        }
        self.session = Some(session::Session::start(
            self.profile().clone(),
            self.ctx.clone(),
        ));
        self.status = "Starting capture. macOS may ask for administrator access…".into();
    }
    fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("ORBWEAVER").size(25.).strong().color(ACCENT));
                ui.label(RichText::new(&self.device).size(13.).color(MUTED));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.session.is_some() {
                    if ui
                        .add(
                            egui::Button::new(if self.ready {
                                "Stop remapping"
                            } else {
                                "Cancel startup"
                            })
                            .min_size(Vec2::new(160., 38.)),
                        )
                        .clicked()
                    {
                        self.session.take();
                        self.ready = false;
                        self.raw.clear();
                        self.learning = None;
                        self.status = "Stopping capture…".into();
                    }
                } else if ui
                    .add_enabled(
                        self.connected,
                        egui::Button::new("Start remapping")
                            .fill(Color32::from_rgb(31, 96, 78))
                            .min_size(Vec2::new(160., 38.)),
                    )
                    .clicked()
                {
                    self.start();
                }
                ui.label(
                    RichText::new(if self.ready {
                        "● ACTIVE"
                    } else {
                        "○ PAUSED"
                    })
                    .color(if self.ready { ACCENT } else { MUTED })
                    .size(12.),
                );
            });
        });
    }
    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.);
        ui.label(RichText::new("PROFILES").color(MUTED).size(11.));
        ui.add_space(8.);
        let mut switch = None;
        for (i, p) in self.library.profiles.iter().enumerate() {
            if ui
                .add_sized(
                    [180., 38.],
                    egui::Button::selectable(self.library.selected == i, &p.name),
                )
                .clicked()
            {
                switch = Some(i);
            }
        }
        if let Some(i) = switch {
            self.cancel_learn();
            self.library.selected = i;
            self.mark_edited();
            self.map = 0;
            self.selected = 0;
            self.wheel_selected = false;
            self.delete_confirm = false;
            self.apply(false);
        }
        ui.add_space(8.);
        ui.label(RichText::new("NEW PROFILE FOR").color(MUTED).size(11.));
        ui.horizontal(|ui| {
            let connected = self.connected_family();
            for family in [Family::Orbweaver, Family::TartarusV2] {
                let label = if family == connected && self.connected {
                    format!("+ {} (connected)", family.name())
                } else {
                    format!("+ {}", family.name())
                };
                if ui
                    .button(label)
                    .on_hover_text(format!(
                        "Creates a profile with the {} control layout.",
                        family.name()
                    ))
                    .clicked()
                {
                    self.new_profile(family);
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Duplicate").clicked() && self.library.profiles.len() < 128 {
                self.cancel_learn();
                let mut p = self.profile().clone();
                p.name = format!("{} copy", p.name);
                self.library.profiles.push(p);
                self.library.selected = self.library.profiles.len() - 1;
                self.mark_edited();
            }
        });
        ui.add_space(16.);
        ui.separator();
        ui.add_space(12.);
        ui.label(RichText::new("EDIT PROFILE").color(MUTED).size(11.));
        if ui
            .text_edit_singleline(&mut self.profile_mut().name)
            .changed()
        {
            self.mark_edited();
        }
        ui.label(
            RichText::new(format!("{} layout", self.profile().family.name()))
                .size(11.)
                .color(MUTED),
        );
        ui.horizontal(|ui| {
            if ui.button("Import").clicked() {
                self.import();
            }
            if ui.button("Export").clicked() {
                self.export();
            }
        });
        if ui
            .add_enabled(
                self.library.profiles.len() > 1,
                egui::Button::new("Delete profile"),
            )
            .clicked()
        {
            self.delete_confirm = true;
        }
        if self.delete_confirm {
            ui.label("Delete this profile?");
            ui.horizontal(|ui| {
                if ui.button("Delete").clicked() {
                    self.cancel_learn();
                    self.library.profiles.remove(self.library.selected);
                    self.library.selected = 0;
                    self.mark_edited();
                    self.delete_confirm = false;
                    self.apply(false);
                }
                if ui.button("Keep").clicked() {
                    self.delete_confirm = false;
                }
            });
        }
        ui.add_space(24.);
        ui.separator();
        ui.label(RichText::new("LOCAL BY DESIGN").size(11.).color(ACCENT));
        ui.label(
            RichText::new("Native Rust. Your profiles stay on this Mac.")
                .color(MUTED)
                .size(12.),
        );
    }
    fn cancel_learn(&mut self) {
        if self.learning.take().is_some()
            && let Some(s) = &self.session
        {
            let _ = s.tx.send(session::Control::Learn(false));
        }
    }
    fn import(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Orbweaver profile", &["json"])
            .pick_file()
        {
            let result = (|| -> anyhow::Result<Profile> {
                let bytes = std::fs::read(path)?;
                anyhow::ensure!(bytes.len() <= 2_000_000, "profile file is too large");
                let p: Profile = serde_json::from_slice(&bytes)?;
                p.validate()?;
                Ok(p)
            })();
            match result {
                Ok(p) if self.library.profiles.len() < 128 => {
                    self.cancel_learn();
                    self.library.profiles.push(p);
                    self.library.selected = self.library.profiles.len() - 1;
                    self.mark_edited();
                    self.map = 0;
                    self.selected = 0;
                    self.wheel_selected = false;
                    self.status = "Profile imported. Save & apply to keep it.".into();
                }
                Ok(_) => self.status = "Profile limit reached (128).".into(),
                Err(e) => self.status = format!("Import failed: {e}"),
            }
        }
    }
    fn export(&mut self) {
        if let Err(e) = self.profile().validate() {
            self.status = e.to_string();
            return;
        }
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Orbweaver profile", &["json"])
            .set_file_name(self.profile().family.export_file_name())
            .save_file()
        {
            let result = serde_json::to_vec_pretty(self.profile())
                .map_err(anyhow::Error::from)
                .and_then(|b| model::atomic_save(&path, &b));
            self.status = match result {
                Ok(()) => "Profile exported.".into(),
                Err(e) => format!("Export failed: {e}"),
            };
        }
    }
    /// Lighting-page color shown on a tile for color slot `led`.
    fn led_color(&self, led: usize) -> [u8; 3] {
        let l = &self.profile().lighting;
        if l.effect == Effect::PerKey {
            l.colors[led]
        } else {
            l.color
        }
    }
    fn tile_button(
        label: String,
        rgb: [u8; 3],
        active: bool,
        selected: bool,
    ) -> egui::Button<'static> {
        let color = if active {
            ACCENT
        } else {
            Color32::from_rgb(rgb[0] / 3 + 15, rgb[1] / 3 + 15, rgb[2] / 3 + 15)
        };
        egui::Button::new(RichText::new(label).size(13.).color(if active {
            Color32::BLACK
        } else {
            Color32::WHITE
        }))
        .fill(color)
        .stroke(egui::Stroke::new(
            if selected { 2. } else { 1. },
            if selected {
                ACCENT
            } else {
                Color32::from_rgb(64, 75, 88)
            },
        ))
    }
    fn tile(&mut self, ui: &mut egui::Ui, i: usize, size: Vec2) {
        let p = self.profile();
        let control = &p.controls[i];
        let active = control.inputs.iter().all(|u| self.raw.contains(u));
        let rgb = match p.family.led_for_control(i) {
            Some(led) if self.page == Page::Lighting => self.led_color(led),
            _ => [41, 51, 62],
        };
        let selected = self.selected == i && !self.wheel_selected;
        let label = if self.page == Page::Lighting {
            control.name.clone()
        } else {
            if p.maps[self.map].bindings[i] == Action::Default {
                if i >= p.family.thumb_pad_first() {
                    control.name.clone()
                } else {
                    format!("{}\n{}", control.name, model::input_label(&control.inputs))
                }
            } else {
                format!("{}\n{}", control.name, p.maps[self.map].bindings[i].label())
            }
        };
        let button = Self::tile_button(label, rgb, active, selected);
        if ui
            .add_sized(size, button)
            .on_hover_text(format!(
                "Physical input: {}",
                model::input_label(&control.inputs)
            ))
            .clicked()
        {
            self.selected = i;
            self.wheel_selected = false;
        }
    }
    /// Lighting-only tile for the Tartarus V2 scroll-wheel LED, which has no key mapping.
    fn wheel_tile(&mut self, ui: &mut egui::Ui, led: usize, size: Vec2) {
        let button = Self::tile_button(
            "Wheel".into(),
            self.led_color(led),
            false,
            self.wheel_selected,
        );
        if ui
            .add_sized(size, button)
            .on_hover_text(
                "Scroll-wheel LED. The wheel keeps its factory scrolling and is not remappable.",
            )
            .clicked()
        {
            self.wheel_selected = true;
        }
    }
    fn thumb_pad(&mut self, ui: &mut egui::Ui) {
        let first = self.profile().family.thumb_pad_first();
        // Directions are stored N, NE, E, SE, S, SW, W, NW.
        let at = |d: usize| first + d;
        ui.vertical(|ui| {
            ui.label(RichText::new("8-WAY THUMB PAD").size(11.).color(MUTED));
            egui::Grid::new("pad").spacing([5., 5.]).show(ui, |ui| {
                for row in [
                    [at(7), at(0), at(1)],
                    [at(6), usize::MAX, at(2)],
                    [at(5), at(4), at(3)],
                ] {
                    for i in row {
                        if i == usize::MAX {
                            ui.add_sized([58., 32.], egui::Label::new("+"));
                        } else {
                            self.tile(ui, i, Vec2::new(58., 32.));
                        }
                    }
                    ui.end_row();
                }
            });
        });
    }
    fn keypad(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new(if self.page == Page::Lighting {
                "Choose a key to edit its light"
            } else {
                "Choose a control to assign"
            })
            .size(18.)
            .strong(),
        );
        ui.add_space(14.);
        match self.profile().family {
            Family::Orbweaver => self.orbweaver_keypad(ui),
            Family::TartarusV2 => self.tartarus_keypad(ui),
        }
    }
    fn orbweaver_keypad(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("keys").spacing([10., 10.]).show(ui, |ui| {
            for row in 0..4 {
                for col in 0..5 {
                    self.tile(ui, row * 5 + col, Vec2::new(72., 50.));
                }
                ui.end_row();
            }
        });
        if self.page == Page::Keys {
            ui.add_space(20.);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("THUMB BUTTONS").size(11.).color(MUTED));
                    self.tile(ui, 20, Vec2::new(122., 40.));
                    self.tile(ui, 21, Vec2::new(122., 40.));
                });
                ui.add_space(22.);
                self.thumb_pad(ui);
            });
            ui.add_space(14.);
            ui.label(
                RichText::new("Default diagonals combine their two direction keys.")
                    .size(12.)
                    .color(MUTED),
            );
        }
    }
    /// Tartarus V2: 19 keys in rows of 5, 5, 5, 4; key 20 and the thumb button on the thumb
    /// module; eight pad directions. The scroll wheel only appears as an LED.
    fn tartarus_keypad(&mut self, ui: &mut egui::Ui) {
        let wheel = self.profile().family.wheel_led();
        ui.horizontal_top(|ui| {
            egui::Grid::new("keys").spacing([10., 10.]).show(ui, |ui| {
                for row in 0..4 {
                    for col in 0..5 {
                        let i = row * 5 + col;
                        if i < 19 {
                            self.tile(ui, i, Vec2::new(72., 50.));
                        }
                    }
                    ui.end_row();
                }
            });
            if self.page == Page::Lighting
                && let Some(led) = wheel
            {
                ui.add_space(12.);
                ui.vertical(|ui| {
                    ui.label(RichText::new("SCROLL WHEEL").size(11.).color(MUTED));
                    self.wheel_tile(ui, led, Vec2::new(72., 50.));
                });
            }
        });
        ui.add_space(20.);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("THUMB MODULE").size(11.).color(MUTED));
                self.tile(ui, 19, Vec2::new(122., 40.));
                if self.page == Page::Keys {
                    self.tile(ui, 20, Vec2::new(122., 40.));
                }
            });
            if self.page == Page::Keys {
                ui.add_space(22.);
                self.thumb_pad(ui);
            }
        });
        ui.add_space(14.);
        ui.label(
            RichText::new(if self.page == Page::Keys {
                "Default diagonals combine their two direction keys. The scroll wheel keeps its factory scrolling."
            } else {
                "Key 20 is the thumb paddle. The thumb button and pad have no LEDs."
            })
            .size(12.)
            .color(MUTED),
        );
    }
    fn inspector(&mut self, ui: &mut egui::Ui) {
        let i = self.selected;
        let map = self.map;
        let family = self.profile().family;
        if self.page == Page::Lighting && self.wheel_selected {
            if let Some(led) = family.wheel_led() {
                ui.label(RichText::new("SCROLL WHEEL LED").color(ACCENT).size(12.));
                ui.add_space(8.);
                if self.profile().lighting.effect == Effect::PerKey {
                    if ui
                        .color_edit_button_srgb(&mut self.profile_mut().lighting.colors[led])
                        .changed()
                    {
                        self.mark_edited();
                    }
                } else {
                    ui.label(
                        RichText::new("Choose Per-key colors to paint the wheel separately.")
                            .color(MUTED),
                    );
                }
                return;
            }
            self.wheel_selected = false;
        }
        ui.label(
            RichText::new(format!("CONTROL {}", self.profile().controls[i].name))
                .color(ACCENT)
                .size(12.),
        );
        ui.add_space(8.);
        if self.page == Page::Lighting {
            let Some(led) = family.led_for_control(i) else {
                ui.label("Thumb controls have no RGB key LEDs.");
                return;
            };
            ui.label("Key color");
            if self.profile().lighting.effect == Effect::PerKey {
                if ui
                    .color_edit_button_srgb(&mut self.profile_mut().lighting.colors[led])
                    .changed()
                {
                    self.mark_edited();
                }
            } else {
                ui.label(
                    RichText::new("Choose Per-key colors to paint this key separately.")
                        .color(MUTED),
                );
            }
            return;
        }
        ui.label("Assignment");
        let action = &self.profile().maps[map].bindings[i];
        let mut kind = match action {
            Action::Default => 0,
            Action::Key { .. } => 1,
            Action::Sequence { .. } => 2,
            Action::Layer { .. } => 3,
            Action::Disabled => 4,
            Action::SpamKey { .. } => 5,
        };
        let before = kind;
        egui::ComboBox::from_id_salt("action-kind")
            .height(300.)
            .selected_text(
                [
                    "Factory key",
                    "Key / shortcut",
                    "Sequence",
                    "Switch keymap",
                    "Disabled",
                    "Spam Key",
                ][kind],
            )
            .show_ui(ui, |ui| {
                for (index, label) in [
                    "Factory key",
                    "Key / shortcut",
                    "Sequence",
                    "Switch keymap",
                    "Disabled",
                    "Spam Key",
                ]
                .iter()
                .enumerate()
                {
                    ui.selectable_value(&mut kind, index, *label);
                }
            });
        if before != kind {
            self.profile_mut().maps[map].bindings[i] = match kind {
                1 => Action::Key {
                    chord: "space".into(),
                },
                2 => Action::Sequence {
                    keys: "command+c, command+v".into(),
                    interval_ms: 100,
                },
                3 => Action::Layer {
                    map: 1,
                    momentary: true,
                },
                4 => Action::Disabled,
                5 => Action::SpamKey {
                    chord: "space".into(),
                    interval_ms: 100,
                },
                _ => Action::Default,
            };
            self.mark_edited();
        }
        let mut changed = false;
        match &mut self.profile_mut().maps[map].bindings[i] {
            Action::SpamKey { chord, interval_ms } => {
                ui.label("Key to spam");
                changed |= ui.text_edit_singleline(chord).changed();
                ui.label(
                    RichText::new("Examples: 1, space, f5, shift+tab")
                        .size(12.)
                        .color(MUTED),
                );
                changed |= ui
                    .add(
                        egui::DragValue::new(interval_ms)
                            .range(20..=5000)
                            .suffix(" ms between presses"),
                    )
                    .changed();
                ui.label(
                    RichText::new("Repeatedly presses and releases while held. Stops on release.")
                        .size(12.)
                        .color(MUTED),
                );
            }
            Action::Key { chord } => {
                changed |= ui.text_edit_singleline(chord).changed();
                ui.label(
                    RichText::new("Examples: command+c, shift+tab, f5, space")
                        .size(12.)
                        .color(MUTED),
                );
            }
            Action::Sequence { keys, interval_ms } => {
                changed |= ui
                    .add(
                        egui::TextEdit::multiline(keys)
                            .desired_rows(3)
                            .desired_width(250.),
                    )
                    .changed();
                ui.label(
                    RichText::new("Comma-separated shortcuts, played once per press.")
                        .size(12.)
                        .color(MUTED),
                );
                changed |= ui
                    .add(
                        egui::DragValue::new(interval_ms)
                            .range(20..=5000)
                            .suffix(" ms between steps"),
                    )
                    .changed();
            }
            Action::Layer { map, momentary } => {
                changed |= egui::ComboBox::from_id_salt("target-map")
                    .selected_text(format!("Keymap {}", *map + 1))
                    .show_ui(ui, |ui| {
                        let mut picked = false;
                        for n in 0..8 {
                            picked |= ui
                                .selectable_value(map, n, format!("Keymap {}", n + 1))
                                .changed();
                        }
                        picked
                    })
                    .inner
                    .unwrap_or(false);
                changed |= ui.checkbox(momentary, "Only while held").changed();
            }
            Action::Default => {
                ui.label(
                    RichText::new("Use the physical input shown below.")
                        .size(12.)
                        .color(MUTED),
                );
            }
            Action::Disabled => {
                ui.label("This control will produce no output.");
            }
        }
        if changed {
            self.mark_edited();
        }
        ui.add_space(20.);
        ui.separator();
        ui.label("Physical input");
        ui.label(
            RichText::new(model::input_label(&self.profile().controls[i].inputs)).color(MUTED),
        );
        if self.learning.is_some() {
            ui.label("Press and release one control…");
            if ui.button("Cancel learning").clicked() {
                self.cancel_learn();
            }
        } else if ui
            .add_enabled(self.ready, egui::Button::new("Learn from device"))
            .clicked()
        {
            self.learning = Some(i);
            if let Some(s) = &self.session {
                let _ = s.tx.send(session::Control::Learn(true));
            }
        }
        if !self.ready {
            ui.label(
                RichText::new("Start remapping to learn a physical control.")
                    .size(12.)
                    .color(MUTED),
            );
        }
        ui.add_space(16.);
        egui::CollapsingHeader::new("Supported key names").show(ui,|ui|{ui.label(RichText::new("a–z, 0–9, f1–f15, command, option, shift, control, enter, escape, space, tab, backspace, delete, caps_lock, up, down, left, right, home, end, page_up, page_down, grave, minus, equal, comma, period, slash, backslash, quote, semicolon, left_bracket, right_bracket").size(12.));});
    }
    fn lighting_controls(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        let l = &mut self.profile_mut().lighting;
        ui.horizontal(|ui| {
            changed |= egui::ComboBox::from_id_salt("effect")
                .selected_text(l.effect.label())
                .show_ui(ui, |ui| {
                    let mut picked = false;
                    for e in [
                        Effect::Static,
                        Effect::PerKey,
                        Effect::Spectrum,
                        Effect::Breathing,
                        Effect::Wave,
                        Effect::Off,
                    ] {
                        picked |= ui.selectable_value(&mut l.effect, e, e.label()).changed();
                    }
                    picked
                })
                .inner
                .unwrap_or(false);
            if matches!(l.effect, Effect::Static | Effect::Breathing) {
                changed |= ui.color_edit_button_srgb(&mut l.color).changed();
            }
            if l.effect == Effect::Wave {
                changed |= ui.checkbox(&mut l.reverse, "Reverse").changed();
            }
        });
        changed |= ui
            .add(egui::Slider::new(&mut l.brightness, 0..=100).text("Brightness %"))
            .changed();
        if l.effect == Effect::PerKey {
            ui.horizontal(|ui| {
                changed |= ui.color_edit_button_srgb(&mut l.color).changed();
                if ui.button("Fill all keys").clicked() {
                    l.colors.fill(l.color);
                    changed = true;
                }
            });
        }
        if changed {
            self.mark_edited();
        }
        ui.add_space(12.);
    }
    fn device_page(&mut self, ui: &mut egui::Ui) {
        ui.heading("Your keypad");
        ui.label(&self.device);
        ui.label("Supported: Orbweaver 1532:0113 and Orbweaver Chroma 1532:0207 (shared profile layout), Tartarus V2 1532:022b (its own profile layout).");
        ui.add_space(20.);
        ui.heading("macOS access");
        ui.label(if native::trusted() {
            "Accessibility is granted."
        } else {
            "Accessibility is needed to send your mapped keys to other apps."
        });
        if ui.button("Open Accessibility settings").clicked() {
            native::request_access();
            let _ = std::process::Command::new("/usr/bin/open")
                .arg(
                    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
                )
                .spawn();
        }
        ui.add_space(12.);
        ui.label("Input Monitoring allows direct access to the keypad's HID interfaces.");
        if ui.button("Set up Input Monitoring").clicked() {
            let _ = crate::permissions::status(true);
            let _ = std::process::Command::new("/usr/bin/open")
                .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent")
                .spawn();
        }
        ui.add_space(8.);
        ui.label("Start remapping requests administrator access for the temporary device capture helper. Your other keyboards remain available. Stop remapping or quit the app to restore factory input.");
        ui.add_space(16.);
        if ui.button("Refresh device").clicked() {
            let _ = self.lights.tx.send(lighting::Request::Inspect);
        }
        ui.add_space(20.);
        ui.heading("Try your mappings");
        ui.label("Click below, then press your keypad. This text stays in memory and is cleared when you quit.");
        ui.add(
            egui::TextEdit::multiline(&mut self.test_text)
                .desired_rows(5)
                .desired_width(f32::INFINITY)
                .hint_text("Test keys here…"),
        );
        if ui.button("Clear test text").clicked() {
            self.test_text.clear();
        }
        ui.add_space(18.);
        ui.label(
            RichText::new(format!("Profiles: {}", model::path().display()))
                .size(12.)
                .color(MUTED),
        );
        ui.label(RichText::new("Keep Orbweaver running for remapping. Settings are applied with Save & apply; they are not stored in the keypad's firmware.").color(MUTED));
    }
}
impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.poll();
        if self.last_device_refresh.elapsed() >= Duration::from_secs(2) {
            let _ = self.lights.tx.send(lighting::Request::Inspect);
            self.last_device_refresh = Instant::now();
        }
        // Worker threads wake the UI as soon as they have news; this only keeps the device
        // check above running while nothing else happens.
        ctx.request_repaint_after(Duration::from_millis(500));
    }
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        egui::Panel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(15, 19, 25))
                    .inner_margin(20),
            )
            .show(ui, |ui| self.header(ui));
        egui::Panel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(15, 19, 25))
                    .inner_margin(16),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&self.status).size(12.));
                        ui.label(
                            RichText::new(if self.dirty {
                                "Unsaved changes"
                            } else {
                                "All profile changes saved"
                            })
                            .size(11.)
                            .color(MUTED),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                !self.load_error,
                                egui::Button::new("Save & apply")
                                    .fill(Color32::from_rgb(31, 96, 78)),
                            )
                            .clicked()
                        {
                            self.cancel_learn();
                            self.apply(true);
                        }
                    });
                });
            });
        egui::Panel::left("profiles")
            .exact_size(210.)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(22, 28, 36))
                    .inner_margin(14),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.sidebar(ui));
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(19, 24, 31))
                    .inner_margin(24),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.page, Page::Keys, "Key mapping");
                    ui.selectable_value(&mut self.page, Page::Lighting, "Lighting");
                    ui.selectable_value(&mut self.page, Page::Device, "Device & test");
                });
                ui.add_space(14.);
                ui.separator();
                ui.add_space(14.);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if self.page == Page::Device {
                        self.device_page(ui);
                        return;
                    }
                    if self.page == Page::Lighting && self.device_model == Some(crate::device::Model::Original) {
                        ui.heading("Key mappings work on both Orbweavers");
                        ui.label("This Orbweaver has no Chroma RGB lighting. Your saved RGB colors are preserved for when you reconnect the Chroma.");
                        ui.label("Use the same profile on either device. Original-model backlight controls are not implemented yet.");
                        return;
                    }
                    if let Some(model) = self.device_model
                        && model.family() != self.profile().family
                    {
                        let wanted = model.family();
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new(format!(
                                    "This profile uses the {} layout. The connected {} needs a {} profile for its own keys and LEDs.",
                                    self.profile().family.name(),
                                    model.name(),
                                    wanted.name()
                                ))
                                .color(Color32::from_rgb(242, 201, 76)),
                            );
                            if ui
                                .button(format!("Create {} profile", wanted.name()))
                                .clicked()
                            {
                                self.new_profile(wanted);
                            }
                        });
                        ui.add_space(10.);
                    }
                    if self.page == Page::Keys {
                        ui.horizontal(|ui| {
                            egui::ComboBox::from_id_salt("keymap")
                                .selected_text(&self.profile().maps[self.map].name)
                                .show_ui(ui, |ui| {
                                    for n in 0..8 {
                                        ui.selectable_value(
                                            &mut self.map,
                                            n,
                                            format!("Keymap {}", n + 1),
                                        );
                                    }
                                });
                            if self.ready {
                                ui.label(
                                    RichText::new(format!("Live: keymap {}", self.live_map + 1))
                                        .color(ACCENT),
                                );
                            }
                        });
                        ui.add_space(12.);
                    } else {
                        self.lighting_controls(ui);
                    }
                    ui.horizontal_top(|ui| {
                        ui.vertical(|ui| {
                            ui.set_width(420.);
                            self.keypad(ui);
                        });
                        ui.add_space(8.);
                        ui.separator();
                        ui.vertical(|ui| {
                            ui.set_width(250.);
                            self.inspector(ui);
                        });
                    });
                });
            });
    }
}
