use super::model::Lighting;
use crate::device::Model;
use eframe::egui;
use std::{
    collections::BTreeSet,
    sync::mpsc::{self, Receiver, Sender},
    thread,
};
pub enum Request {
    Inspect,
    Apply(Lighting),
}
#[derive(Clone)]
pub struct DeviceStatus {
    pub description: String,
    pub model: Model,
}
pub enum Reply {
    Device(Result<DeviceStatus, String>),
    Applied(Result<String, String>),
}
pub struct Worker {
    pub tx: Sender<Request>,
    pub rx: Receiver<Reply>,
}
/// Device access kept between requests, so the periodic inspection only re-enumerates
/// instead of re-initialising HID access and opening the keypad each time.
#[derive(Default)]
struct Poller {
    api: Option<hidapi::HidApi>,
    /// Paths of the supported keypad interfaces seen by the last inspection.
    paths: BTreeSet<Vec<u8>>,
    /// Result of the last inspection, resent while the interface set is unchanged.
    status: Option<Result<DeviceStatus, String>>,
}
impl Poller {
    /// Make `self.api` available with a fresh device list; retried from scratch after a failure.
    fn refresh(&mut self) -> anyhow::Result<()> {
        if let Some(api) = &mut self.api
            && let Err(e) = api.refresh_devices()
        {
            self.api = None;
            return Err(e.into());
        }
        if self.api.is_none() {
            self.api = Some(hidapi::HidApi::new()?);
        }
        Ok(())
    }
    fn inspect(&mut self) -> Result<DeviceStatus, String> {
        let result = self.describe().map_err(|e| format!("{e:#}"));
        self.status = Some(result.clone());
        result
    }
    fn describe(&mut self) -> anyhow::Result<DeviceStatus> {
        self.refresh()?;
        let api = self.api.as_ref().expect("initialised by refresh");
        let paths: BTreeSet<Vec<u8>> = crate::device::matching(api)
            .iter()
            .map(|d| d.path().to_bytes().to_vec())
            .collect();
        if paths == self.paths
            && let Some(Ok(status)) = &self.status
        {
            return Ok(status.clone());
        }
        self.paths = paths;
        let model = crate::device::connected_model(api)?;
        if !model.supports_rgb() {
            return Ok(DeviceStatus {
                description: format!("{} · USB 1532:{:04x}", model.name(), model.product_id()),
                model,
            });
        }
        let handle = crate::device::select(api, 2)?.open_device(api)?;
        let f = crate::protocol::query_firmware_for(&handle, model)?;
        Ok(DeviceStatus {
            description: format!("{} · Firmware {}.{}", model.name(), f.major, f.minor),
            model,
        })
    }
    fn apply(&mut self, l: &Lighting) -> anyhow::Result<String> {
        self.refresh()?;
        let api = self.api.as_ref().expect("initialised by refresh");
        let model = crate::device::connected_model(api)?;
        if !model.supports_rgb() {
            return Ok("Mappings ready. RGB settings are kept for an RGB keypad.".into());
        }
        let handle = crate::device::select(api, 2)?.open_device(api)?;
        // A profile made for the other layout still lights the keypad,
        // but its per-key colors cannot be placed.
        let (lighting, adapted) = l.adapted(model.family());
        crate::protocol::apply_lighting(&handle, model, &lighting)?;
        Ok(if adapted {
            format!(
                "Lighting acknowledged by the {}. This profile's per-key colors belong to another layout, so the solid color was used.",
                model.name()
            )
        } else {
            format!(
                "Lighting acknowledged by the {}. Profile is ready.",
                model.name()
            )
        })
    }
}
impl Worker {
    /// `ctx` is woken after every reply so the GUI redraws without polling.
    pub fn new(ctx: egui::Context) -> Self {
        let (tx, requests) = mpsc::channel();
        let (events, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut poller = Poller::default();
            while let Ok(request) = requests.recv() {
                let reply = match request {
                    Request::Inspect => Reply::Device(poller.inspect()),
                    Request::Apply(l) => {
                        Reply::Applied(poller.apply(&l).map_err(|e| format!("{e:#}")))
                    }
                };
                if events.send(reply).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        });
        let worker = Self { tx, rx };
        let _ = worker.tx.send(Request::Inspect);
        worker
    }
}
