use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
pub struct SoftwareOption {
    pub name: String,
    pub description: String,
    pub url: String,
    pub filename: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct SupportedDevice {
    pub name: String,
    pub vid: u16,
    pub pid: u16,
    pub options: Vec<SoftwareOption>,
}

pub struct AppState {
    pub found_devices: Vec<SupportedDevice>,
}

pub fn shorten_software_name(name: &str) -> &str {
    match name {
        "Onboard Memory Manager" => "OMM",
        _ => name,
    }
}

