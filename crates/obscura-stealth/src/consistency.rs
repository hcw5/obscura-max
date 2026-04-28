use crate::profile::BrowserProfile;

pub fn validate_profile(profile: &BrowserProfile) -> Result<(), String> {
    validate_screen_and_window(profile)?;
    validate_language_consistency(profile)?;
    validate_platform_consistency(profile)?;
    validate_timezone(profile)?;
    validate_hardware_class(profile)?;
    Ok(())
}

fn validate_screen_and_window(profile: &BrowserProfile) -> Result<(), String> {
    let screen = &profile.screen;
    let window = &profile.window;

    if screen.width == 0 || screen.height == 0 {
        return Err("screen width/height must be > 0".to_string());
    }
    if screen.avail_width == 0 || screen.avail_height == 0 {
        return Err("screen avail_width/avail_height must be > 0".to_string());
    }
    if screen.avail_width > screen.width || screen.avail_height > screen.height {
        return Err("screen available size cannot exceed full screen size".to_string());
    }
    if !(0.5..=4.0).contains(&screen.device_pixel_ratio) {
        return Err("device_pixel_ratio must be in [0.5, 4.0]".to_string());
    }

    if window.inner_width == 0 || window.inner_height == 0 {
        return Err("window inner dimensions must be > 0".to_string());
    }
    if window.inner_width > window.outer_width || window.inner_height > window.outer_height {
        return Err("window inner dimensions cannot exceed outer dimensions".to_string());
    }
    if window.outer_width > screen.width || window.outer_height > screen.height {
        return Err("window outer dimensions cannot exceed screen dimensions".to_string());
    }

    Ok(())
}

fn validate_language_consistency(profile: &BrowserProfile) -> Result<(), String> {
    if profile.languages.is_empty() {
        return Err("languages must not be empty".to_string());
    }

    let primary_language = profile
        .languages
        .first()
        .map(|s| s.to_lowercase())
        .ok_or_else(|| "missing primary language".to_string())?;

    let accept_primary = profile
        .accept_language
        .split(',')
        .next()
        .map(str::trim)
        .map(|v| v.split(';').next().unwrap_or(v))
        .map(|s| s.to_lowercase())
        .ok_or_else(|| "accept_language must include at least one locale".to_string())?;

    if accept_primary != primary_language {
        return Err(format!(
            "accept_language primary locale ({accept_primary}) does not match languages[0] ({primary_language})"
        ));
    }

    let locale = profile.locale.to_lowercase().replace('_', "-");
    if locale != primary_language {
        return Err(format!(
            "locale ({locale}) must match primary language ({primary_language})"
        ));
    }

    Ok(())
}

fn validate_platform_consistency(profile: &BrowserProfile) -> Result<(), String> {
    let platform = profile.platform.to_lowercase();
    let ua_platform = profile.ua_data.platform.to_lowercase();

    let compatible = match platform.as_str() {
        "win32" | "win64" => ua_platform == "windows",
        "macintel" | "mac" => ua_platform == "macos",
        "linux x86_64" | "linux" | "x11" => ua_platform == "linux",
        _ => false,
    };

    if !compatible {
        return Err(format!(
            "platform ({}) is not compatible with ua_data.platform ({})",
            profile.platform, profile.ua_data.platform
        ));
    }

    Ok(())
}

fn validate_timezone(profile: &BrowserProfile) -> Result<(), String> {
    let tz = profile.timezone.trim();
    if tz.is_empty() || !tz.contains('/') {
        return Err("timezone must be a non-empty IANA-style zone like Region/City".to_string());
    }
    Ok(())
}

fn validate_hardware_class(profile: &BrowserProfile) -> Result<(), String> {
    const ALLOWED: &[&str] = &["low", "mid", "high"];
    if !ALLOWED.contains(&profile.hardware_class.as_str()) {
        return Err(format!(
            "hardware_class must be one of {:?}, got {}",
            ALLOWED, profile.hardware_class
        ));
    }
    Ok(())
}
