use super::model::Lighting;
use crate::device::Model;
use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
};
pub enum Request {
    Inspect,
    Apply(Lighting),
}
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
impl Worker {
    pub fn new() -> Self {
        let (tx, requests) = mpsc::channel();
        let (events, rx) = mpsc::channel();
        thread::spawn(move || {
            while let Ok(request) = requests.recv() {
                let result = (|| -> anyhow::Result<(Model, String)> {
                    let api = hidapi::HidApi::new()?;
                    let model = crate::device::connected_model(&api)?;
                    if !model.supports_rgb() {
                        return Ok((
                            model,
                            match request {
                                Request::Inspect => format!(
                                    "{} · USB 1532:{:04x}",
                                    model.name(),
                                    model.product_id()
                                ),
                                Request::Apply(_) => {
                                    "Mappings ready. RGB settings retained for your Chroma.".into()
                                }
                            },
                        ));
                    }
                    let info = crate::device::select(&api, 2)?;
                    let handle = info.open_device(&api)?;
                    let message = match &request {
                        Request::Inspect => {
                            let f = crate::protocol::query_firmware(&handle)?;
                            format!("{} · Firmware {}.{}", model.name(), f.major, f.minor)
                        }
                        Request::Apply(l) => {
                            crate::protocol::apply_lighting(&handle, l)?;
                            "Lighting acknowledged by the Orbweaver. Profile is ready.".into()
                        }
                    };
                    Ok((model, message))
                })();
                let reply = match request {
                    Request::Inspect => Reply::Device(
                        result
                            .map(|(model, description)| DeviceStatus { model, description })
                            .map_err(|e| format!("{e:#}")),
                    ),
                    Request::Apply(_) => {
                        Reply::Applied(result.map(|(_, s)| s).map_err(|e| format!("{e:#}")))
                    }
                };
                if events.send(reply).is_err() {
                    break;
                }
            }
        });
        let worker = Self { tx, rx };
        let _ = worker.tx.send(Request::Inspect);
        worker
    }
}
