use hidapi::{DeviceInfo, HidApi};
use serde::Serialize;
use std::collections::BTreeSet;

pub const VID: u16 = 0x1532;
pub const CHROMA_PID: u16 = 0x0207;
pub const ORIGINAL_PID: u16 = 0x0113;
pub const TARTARUS_V2_PID: u16 = 0x022b;
pub const SUPPORTED_PIDS: [u16; 3] = [ORIGINAL_PID, CHROMA_PID, TARTARUS_V2_PID];
pub const SUPPORTED_LIST: &str =
    "Orbweaver 1532:0113, Orbweaver Chroma 1532:0207, or Tartarus V2 1532:022b";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Model {
    Original,
    Chroma,
    TartarusV2,
}
impl Model {
    pub fn from_ids(vendor: u16, product: u16) -> Option<Self> {
        if vendor != VID {
            return None;
        }
        match product {
            ORIGINAL_PID => Some(Self::Original),
            CHROMA_PID => Some(Self::Chroma),
            TARTARUS_V2_PID => Some(Self::TartarusV2),
            _ => None,
        }
    }
    pub fn product_id(self) -> u16 {
        match self {
            Self::Original => ORIGINAL_PID,
            Self::Chroma => CHROMA_PID,
            Self::TartarusV2 => TARTARUS_V2_PID,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Original => "Orbweaver",
            Self::Chroma => "Orbweaver Chroma",
            Self::TartarusV2 => "Tartarus V2",
        }
    }
    pub fn supports_rgb(self) -> bool {
        matches!(self, Self::Chroma | Self::TartarusV2)
    }
    /// Profile layout this unit uses. Both Orbweavers share one layout.
    pub fn family(self) -> crate::app::model::Family {
        match self {
            Self::Original | Self::Chroma => crate::app::model::Family::Orbweaver,
            Self::TartarusV2 => crate::app::model::Family::TartarusV2,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Interface {
    pub vendor_id: u16,
    pub product_id: u16,
    pub product: Option<String>,
    pub path: String,
    pub interface_number: i32,
    pub usage_page: u16,
    pub usage: u16,
    pub release_number: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub descriptor_hex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reports: Option<Vec<crate::descriptor::ReportSizes>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<crate::input::Layout>,
}

pub fn matching(api: &HidApi) -> Vec<&DeviceInfo> {
    let mut paths = BTreeSet::new();
    let mut devices: Vec<_> = api
        .device_list()
        .filter(|d| Model::from_ids(d.vendor_id(), d.product_id()).is_some())
        .filter(|d| paths.insert(d.path().to_bytes().to_vec()))
        .collect();
    devices.sort_by_key(|d| d.path().to_bytes());
    devices
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn inventory(api: &HidApi, inspect: bool) -> Vec<Interface> {
    matching(api)
        .into_iter()
        .map(|d| {
            let mut item = Interface {
                vendor_id: d.vendor_id(),
                product_id: d.product_id(),
                product: d.product_string().map(str::to_owned),
                path: d.path().to_string_lossy().into_owned(),
                interface_number: d.interface_number(),
                usage_page: d.usage_page(),
                usage: d.usage(),
                release_number: d.release_number(),
                access: None,
                descriptor_hex: None,
                reports: None,
                layout: None,
            };
            if inspect {
                match d.open_device(api) {
                    Ok(handle) => {
                        item.access = Some("opened_shared".into());
                        let mut buf = [0u8; hidapi::MAX_REPORT_DESCRIPTOR_SIZE];
                        match handle.get_report_descriptor(&mut buf) {
                            Ok(n) => {
                                item.descriptor_hex = Some(hex(&buf[..n]));
                                item.layout = Some(crate::input::layout(&buf[..n]));
                                match crate::descriptor::sizes(&buf[..n]) {
                                    Ok(sizes) => item.reports = Some(sizes),
                                    Err(e) => {
                                        item.access =
                                            Some(format!("opened; descriptor parse failed: {e}"))
                                    }
                                }
                            }
                            Err(e) => {
                                item.access = Some(format!("opened; descriptor unavailable: {e}"))
                            }
                        }
                    }
                    Err(e) => {
                        item.access =
                            Some(format!("open_failed: {e}. {}", access_hint(&e.to_string())))
                    }
                }
            }
            item
        })
        .collect()
}

pub fn access_hint(error: &str) -> &'static str {
    if error.to_lowercase().contains("exclusive") || error.to_lowercase().contains("e00002c5") {
        "Another app may have seized this interface. Stop its capture of this keypad, then try again."
    } else {
        "On macOS check Input Monitoring for the launching terminal/app, then restart it. Also check connection and other device-control apps; this error alone does not prove a permission problem."
    }
}

pub fn select(api: &HidApi, interface: i32) -> anyhow::Result<&DeviceInfo> {
    let model = connected_model(api)?;
    let devices: Vec<_> = matching(api)
        .into_iter()
        .filter(|d| d.product_id() == model.product_id() && d.interface_number() == interface)
        .collect();
    anyhow::ensure!(
        devices.len() == 1,
        "expected one {} interface {interface}, found {}; connect exactly one supported keypad ({SUPPORTED_LIST})",
        model.name(),
        devices.len()
    );
    Ok(devices[0])
}

pub fn connected_model(api: &HidApi) -> anyhow::Result<Model> {
    let keyboards: Vec<_> = matching(api)
        .into_iter()
        .filter(|d| d.interface_number() == 0)
        .collect();
    anyhow::ensure!(
        keyboards.len() == 1,
        "Connect exactly one supported keypad ({SUPPORTED_LIST}); found {} primary keyboard interfaces",
        keyboards.len()
    );
    Ok(Model::from_ids(keyboards[0].vendor_id(), keyboards[0].product_id()).unwrap())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model::Family;
    #[test]
    fn only_the_three_supported_keypads_match() {
        assert_eq!(Model::from_ids(VID, ORIGINAL_PID), Some(Model::Original));
        assert_eq!(Model::from_ids(VID, CHROMA_PID), Some(Model::Chroma));
        assert_eq!(
            Model::from_ids(VID, TARTARUS_V2_PID),
            Some(Model::TartarusV2)
        );
        assert_eq!(Model::from_ids(0x1234, ORIGINAL_PID), None);
        assert_eq!(Model::from_ids(0x1234, TARTARUS_V2_PID), None);
        // Neighbouring Razer keypads are not supported: Tartarus, Tartarus Chroma, Tartarus Pro.
        for pid in [0x0201, 0x0208, 0x0244, 0x0111] {
            assert_eq!(Model::from_ids(VID, pid), None);
        }
        assert!(!Model::Original.supports_rgb());
        assert!(Model::Chroma.supports_rgb());
        assert!(Model::TartarusV2.supports_rgb());
        assert_eq!(Model::Original.family(), Family::Orbweaver);
        assert_eq!(Model::Chroma.family(), Family::Orbweaver);
        assert_eq!(Model::TartarusV2.family(), Family::TartarusV2);
        for model in [Model::Original, Model::Chroma, Model::TartarusV2] {
            assert_eq!(Model::from_ids(VID, model.product_id()), Some(model));
            assert!(SUPPORTED_PIDS.contains(&model.product_id()));
        }
    }
}
