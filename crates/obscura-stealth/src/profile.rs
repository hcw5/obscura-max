use serde::Deserialize;

use crate::consistency::validate_profile;

#[derive(Debug, Clone, Deserialize)]
pub struct BrowserProfile {
    pub id: String,
    pub locale: String,
    pub timezone: String,
    pub accept_language: String,
    pub languages: Vec<String>,
    pub platform: String,
    pub hardware_class: String,
    pub user_agent: String,
    pub screen: ScreenProfile,
    pub window: WindowProfile,
    pub ua_data: UaDataProfile,
    pub webgl: WebGlProfile,
    pub battery: BatteryProfile,
    pub tls: TlsProfile,
    pub plugins: Vec<PluginProfile>,
    pub mime_types: Vec<MimeTypeProfile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScreenProfile {
    pub width: u32,
    pub height: u32,
    pub avail_width: u32,
    pub avail_height: u32,
    pub color_depth: u8,
    pub pixel_depth: u8,
    pub device_pixel_ratio: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WindowProfile {
    pub inner_width: u32,
    pub inner_height: u32,
    pub outer_width: u32,
    pub outer_height: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UaDataProfile {
    pub brands: Vec<UaBrand>,
    pub mobile: bool,
    pub platform: String,
    pub platform_version: String,
    pub architecture: String,
    pub model: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UaBrand {
    pub brand: String,
    pub version: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WebGlProfile {
    pub vendor: String,
    pub renderer: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BatteryProfile {
    pub charging: bool,
    pub charging_time: f64,
    pub discharging_time: f64,
    pub level: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TlsProfile {
    pub min_version: String,
    pub max_version: String,
    pub ciphers: Vec<String>,
    pub signature_algorithms: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PluginProfile {
    pub name: String,
    pub filename: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MimeTypeProfile {
    pub r#type: String,
    pub suffixes: String,
    pub description: String,
    pub enabled_plugin: String,
}

const CHROME_WIN11_EN_US: &str = include_str!("../profiles/chrome_win11_en_us.json");
const CHROME_MACOS_EN_US: &str = include_str!("../profiles/chrome_macos_en_us.json");
const CHROME_LINUX_EN_GB: &str = include_str!("../profiles/chrome_linux_en_gb.json");

pub fn load_profile_by_id(id: &str) -> BrowserProfile {
    let raw = match id {
        "chrome_win11_en_us" => CHROME_WIN11_EN_US,
        "chrome_macos_en_us" => CHROME_MACOS_EN_US,
        "chrome_linux_en_gb" => CHROME_LINUX_EN_GB,
        _ => panic!("Unknown browser profile id: {id}"),
    };

    load_profile_from_json(raw)
}

pub fn load_profile_from_json(raw: &str) -> BrowserProfile {
    let profile: BrowserProfile = serde_json::from_str(raw)
        .unwrap_or_else(|err| panic!("Invalid browser profile JSON: {err}"));

    if let Err(err) = validate_profile(&profile) {
        panic!("Browser profile consistency validation failed: {err}");
    }

    profile
}

pub fn builtin_profile_ids() -> &'static [&'static str] {
    &[
        "chrome_win11_en_us",
        "chrome_macos_en_us",
        "chrome_linux_en_gb",
    ]
}

#[cfg(test)]
mod tests {
    use super::{builtin_profile_ids, load_profile_by_id};

    #[test]
    fn all_builtin_profiles_load_and_validate() {
        for id in builtin_profile_ids() {
            let profile = load_profile_by_id(id);
            assert_eq!(&profile.id, id);
        }
    }
}
