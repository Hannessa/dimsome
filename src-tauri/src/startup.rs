#[cfg(target_os = "windows")]
pub fn get_startup_state() -> crate::models::StartupRegistrationState {
    use std::process::Command;

    // Read the standard Run key so startup behavior matches Windows expectations.
    let value = Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            "Dimsome",
        ])
        .output();

    match value {
        Ok(output) if output.status.success() => crate::models::StartupRegistrationState {
            is_enabled: true,
            can_change: true,
            status_text: "Dimsome launches when you sign in.".to_string(),
        },
        _ => crate::models::StartupRegistrationState {
            is_enabled: false,
            can_change: true,
            status_text: "Startup is off.".to_string(),
        },
    }
}

#[cfg(target_os = "windows")]
pub fn set_startup_enabled(
    enabled: bool,
    executable_path: &str,
) -> Result<crate::models::StartupRegistrationState, String> {
    use std::process::Command;

    // Add or remove the Run entry instead of keeping a separate startup manifest.
    let status = if enabled {
        Command::new("reg")
            .args([
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "Dimsome",
                "/t",
                "REG_SZ",
                "/d",
                executable_path,
                "/f",
            ])
            .status()
    } else {
        Command::new("reg")
            .args([
                "delete",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "Dimsome",
                "/f",
            ])
            .status()
    }
    .map_err(|error| error.to_string())?;

    if !status.success() {
        return Err("Failed to update startup registration.".into());
    }

    Ok(get_startup_state())
}

#[cfg(not(target_os = "windows"))]
pub fn get_startup_state() -> crate::models::StartupRegistrationState {
    crate::models::StartupRegistrationState {
        is_enabled: false,
        can_change: false,
        status_text: "Startup is not implemented on this platform yet.".to_string(),
    }
}

#[cfg(not(target_os = "windows"))]
pub fn set_startup_enabled(
    _enabled: bool,
    _executable_path: &str,
) -> Result<crate::models::StartupRegistrationState, String> {
    // Non-Windows builds report capability without pretending the operation succeeded.
    Ok(get_startup_state())
}
