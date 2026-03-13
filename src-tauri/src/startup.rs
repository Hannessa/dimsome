#[cfg(target_os = "windows")]
fn startup_state_from_enabled(enabled: bool) -> crate::models::StartupRegistrationState {
    crate::models::StartupRegistrationState {
        is_enabled: enabled,
        can_change: true,
        status_text: if enabled {
            "Dimsome launches when you sign in.".to_string()
        } else {
            "Startup is off.".to_string()
        },
    }
}

#[cfg(target_os = "windows")]
fn reg_command() -> std::process::Command {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = std::process::Command::new("reg");

    // Keep background registry work from flashing a console window over the desktop.
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(target_os = "windows")]
pub fn get_startup_state() -> crate::models::StartupRegistrationState {
    // Read the standard Run key so startup behavior matches Windows expectations.
    let value = reg_command()
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            "Dimsome",
        ])
        .output();

    match value {
        Ok(output) if output.status.success() => startup_state_from_enabled(true),
        _ => startup_state_from_enabled(false),
    }
}

#[cfg(target_os = "windows")]
pub fn set_startup_enabled(
    enabled: bool,
    executable_path: &str,
) -> Result<crate::models::StartupRegistrationState, String> {
    // Add or remove the Run entry instead of keeping a separate startup manifest.
    let status = if enabled {
        reg_command()
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
        reg_command()
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

    // Return the requested state directly so one toggle only launches one helper process.
    Ok(startup_state_from_enabled(enabled))
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
