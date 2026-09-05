use super::engine::Output;
use anyhow::{Result, anyhow};
use core_graphics::{
    event::{CGEvent, CGEventFlags, CGEventTapLocation, EventField},
    event_source::{CGEventSource, CGEventSourceStateID},
};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn CGRequestPostEventAccess() -> bool;
    fn CGEventSourceFlagsState(state: i32) -> u64;
}
pub fn trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}
pub fn request_access() {
    unsafe {
        CGRequestPostEventAccess();
    }
}
pub struct Emitter {
    source: CGEventSource,
    held: BTreeSet<u16>,
    repeat: Option<(u16, Instant)>,
}
impl Emitter {
    pub fn new() -> Result<Self> {
        Ok(Self {
            source: CGEventSource::new(CGEventSourceStateID::Private)
                .map_err(|_| anyhow!("cannot create macOS event source"))?,
            held: BTreeSet::new(),
            repeat: None,
        })
    }
    pub fn emit(&mut self, events: Vec<Output>) -> Result<()> {
        for e in events {
            if e.pressed {
                self.held.insert(e.code);
                if !matches!(e.code, 54..=62) {
                    self.repeat = Some((e.code, Instant::now() + Duration::from_millis(500)));
                }
            } else {
                self.held.remove(&e.code);
                if self.repeat.is_some_and(|(code, _)| code == e.code) {
                    self.repeat = None;
                }
            }
            let event = CGEvent::new_keyboard_event(self.source.clone(), e.code, e.pressed)
                .map_err(|_| anyhow!("cannot create keyboard event"))?;
            event.set_integer_value_field(EventField::KEYBOARD_EVENT_AUTOREPEAT, 0);
            event.set_flags(self.flags());
            event.post(CGEventTapLocation::Session);
        }
        Ok(())
    }
    fn flags(&self) -> CGEventFlags {
        let mut flags = unsafe { CGEventSourceFlagsState(1) }; // Physical HID state, separate from our private source.
        for code in &self.held {
            flags |= match code {
                54 | 55 => 0x100000,
                56 | 60 => 0x20000,
                58 | 61 => 0x80000,
                59 | 62 => 0x40000,
                _ => 0,
            };
        }
        CGEventFlags::from_bits_truncate(flags)
    }
    pub fn tick(&mut self) -> Result<()> {
        if let Some((code, due)) = self.repeat
            && Instant::now() >= due
        {
            let event = CGEvent::new_keyboard_event(self.source.clone(), code, true)
                .map_err(|_| anyhow!("cannot create repeat event"))?;
            event.set_flags(self.flags());
            event.set_integer_value_field(EventField::KEYBOARD_EVENT_AUTOREPEAT, 1);
            event.post(CGEventTapLocation::Session);
            self.repeat = Some((code, Instant::now() + Duration::from_millis(33)));
        }
        Ok(())
    }
    pub fn release_all(&mut self) {
        let events = self
            .held
            .iter()
            .map(|c| Output {
                code: *c,
                pressed: false,
            })
            .collect();
        let _ = self.emit(events);
    }
}
impl Drop for Emitter {
    fn drop(&mut self) {
        self.release_all();
    }
}
