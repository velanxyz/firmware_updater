use crate::models::{SoftwareOption, SupportedDevice};
use hidapi::HidApi;

// --- ПОИСК УСТРОЙСТВ ---

pub fn scan_usb(database: &[SupportedDevice]) -> Vec<SupportedDevice> {
    let mut found = Vec::new();

    if let Ok(api) = HidApi::new() {
        for device in api.device_list() {
            let vid = device.vendor_id();
            let pid = device.product_id();
            let mut exact_match = false;

            // 1. Точное совпадение VID/PID в базе
            for supported in database {
                if vid == supported.vid && pid == supported.pid {
                    if !found.iter().any(|d: &SupportedDevice| d.name == supported.name) {
                        found.push(supported.clone());
                    }
                    exact_match = true;
                    break;
                }
            }

            // 2. Fallback для Logitech
            if !exact_match {
                if let Some(generic) =
                    get_vendor_fallback(vid, pid, device.product_string())
                {
                    if !found.iter().any(|d: &SupportedDevice| d.name == generic.name) {
                        found.push(generic);
                    }
                }
            }
        }
    }

    found
}

pub fn get_vendor_fallback(
    vid: u16,
    pid: u16,
    product_name: Option<&str>,
) -> Option<SupportedDevice> {
    let dev_name = product_name.unwrap_or("Unknown").to_string();

    match vid {
        0x046d => Some(SupportedDevice {
            name: format!("Logitech Device ({})", dev_name),
            vid,
            pid,
            options: vec![SoftwareOption {
                name: "Logitech G HUB".into(),
                description: "Основной драйвер".into(),
                url: "https://download01.logi.com/web/ftp/pub/techsupport/gaming/lghub_installer.exe"
                    .into(),
                filename: "lghub.exe".into(),
            }],
        }),
        _ => None,
    }
}

