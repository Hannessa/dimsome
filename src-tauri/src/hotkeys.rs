use std::{
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread,
    time::Duration,
};

use windows::Win32::UI::{
    Input::KeyboardAndMouse::{
        RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_SHIFT,
        MOD_WIN,
    },
    WindowsAndMessaging::{PeekMessageW, MSG, PM_NOREMOVE, PM_REMOVE, WM_HOTKEY},
};

use crate::models::{HotkeyBinding, ManualHotkeys};

const HOTKEY_DIM_MORE_ID: i32 = 2001;
const HOTKEY_DIM_LESS_ID: i32 = 2002;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyAction {
    DimMore,
    DimLess,
}

enum HotkeyCommand {
    UpdateBindings(ManualHotkeys),
    Shutdown,
}

pub struct HotkeyManager {
    sender: Sender<HotkeyCommand>,
}

impl HotkeyManager {
    pub fn new<F>(handler: F) -> Self
    where
        F: Fn(HotkeyAction) + Send + 'static,
    {
        // Run Win32 hotkey registration on its own thread with its own message pump.
        let (sender, receiver) = mpsc::channel::<HotkeyCommand>();
        thread::spawn(move || hotkey_thread(receiver, Box::new(handler)));
        Self { sender }
    }

    pub fn update_bindings(&self, hotkeys: ManualHotkeys) {
        let _ = self.sender.send(HotkeyCommand::UpdateBindings(hotkeys));
    }
}

impl Drop for HotkeyManager {
    fn drop(&mut self) {
        let _ = self.sender.send(HotkeyCommand::Shutdown);
    }
}

fn hotkey_thread(receiver: Receiver<HotkeyCommand>, handler: Box<dyn Fn(HotkeyAction) + Send>) {
    ensure_message_queue();

    loop {
        // Drain any queued binding updates before we wait for the next event.
        while let Ok(command) = receiver.try_recv() {
            if !handle_command(command) {
                unregister_all();
                return;
            }
        }

        pump_hotkey_messages(&handler);

        match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(command) => {
                if !handle_command(command) {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        // Process any WM_HOTKEY messages that arrived while we were waiting.
        pump_hotkey_messages(&handler);
    }

    unregister_all();
}

fn handle_command(command: HotkeyCommand) -> bool {
    match command {
        HotkeyCommand::UpdateBindings(hotkeys) => {
            update_registrations(&hotkeys);
            true
        }
        HotkeyCommand::Shutdown => false,
    }
}

fn ensure_message_queue() {
    unsafe {
        // Touch the message queue once so RegisterHotKey has a thread queue to target.
        let mut msg = MSG::default();
        let _ = PeekMessageW(&mut msg, None, WM_HOTKEY, WM_HOTKEY, PM_NOREMOVE);
    }
}

fn update_registrations(hotkeys: &ManualHotkeys) {
    // Re-register from scratch so the active bindings always match saved settings.
    unregister_all();
    register_binding(&hotkeys.dim_more, HOTKEY_DIM_MORE_ID, "Dim more");
    register_binding(&hotkeys.dim_less, HOTKEY_DIM_LESS_ID, "Dim less");
}

fn unregister_all() {
    unsafe {
        let _ = UnregisterHotKey(None, HOTKEY_DIM_MORE_ID);
        let _ = UnregisterHotKey(None, HOTKEY_DIM_LESS_ID);
    }
}

fn register_binding(binding: &HotkeyBinding, id: i32, label: &str) {
    if !binding.enabled {
        return;
    }

    let Some(modifiers) = parse_modifier_flags(&binding.modifiers) else {
        eprintln!(
            "{label} hotkey ignored because modifiers '{0}' could not be parsed.",
            binding.modifiers
        );
        return;
    };

    let Some(virtual_key) = virtual_key_code(&binding.key) else {
        eprintln!(
            "{label} hotkey ignored because key '{0}' is unsupported.",
            binding.key
        );
        return;
    };

    unsafe {
        // Let Windows own the actual chord matching once the binding is registered.
        if let Err(error) = RegisterHotKey(None, id, modifiers, virtual_key) {
            eprintln!("{label} hotkey registration failed: {error}");
        }
    }
}

fn pump_hotkey_messages(handler: &dyn Fn(HotkeyAction)) {
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, WM_HOTKEY, WM_HOTKEY, PM_REMOVE).into() {
            match msg.wParam.0 as i32 {
                HOTKEY_DIM_MORE_ID => handler(HotkeyAction::DimMore),
                HOTKEY_DIM_LESS_ID => handler(HotkeyAction::DimLess),
                _ => {}
            }
        }
    }
}

fn parse_modifier_flags(text: &str) -> Option<HOT_KEY_MODIFIERS> {
    let mut saw_token = false;
    let mut modifiers = HOT_KEY_MODIFIERS(0);

    // Accept either comma- or plus-separated modifier strings from persisted settings.
    for token in text.split(|character| character == '+' || character == ',') {
        let normalized = token.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            continue;
        }

        saw_token = true;
        match normalized.as_str() {
            "alt" => modifiers |= MOD_ALT,
            "ctrl" | "control" => modifiers |= MOD_CONTROL,
            "shift" => modifiers |= MOD_SHIFT,
            "win" | "windows" => modifiers |= MOD_WIN,
            _ => return None,
        }
    }

    if saw_token {
        Some(modifiers)
    } else {
        None
    }
}

fn virtual_key_code(key: &str) -> Option<u32> {
    let normalized = key.trim();
    if normalized.chars().count() == 1 {
        let character = normalized.chars().next()?.to_ascii_uppercase();
        if character.is_ascii_uppercase() || character.is_ascii_digit() {
            return Some(character as u32);
        }
    }

    // Map the named keys we expose in settings onto Win32 virtual-key codes.
    match normalized.to_ascii_lowercase().as_str() {
        "pageup" => Some(0x21),
        "pagedown" => Some(0x22),
        "end" => Some(0x23),
        "home" => Some(0x24),
        "left" => Some(0x25),
        "up" => Some(0x26),
        "right" => Some(0x27),
        "down" => Some(0x28),
        "insert" => Some(0x2D),
        "delete" => Some(0x2E),
        "f1" => Some(0x70),
        "f2" => Some(0x71),
        "f3" => Some(0x72),
        "f4" => Some(0x73),
        "f5" => Some(0x74),
        "f6" => Some(0x75),
        "f7" => Some(0x76),
        "f8" => Some(0x77),
        "f9" => Some(0x78),
        "f10" => Some(0x79),
        "f11" => Some(0x7A),
        "f12" => Some(0x7B),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_modifier() {
        assert_eq!(
            parse_modifier_flags("Alt").map(|value| value.0),
            Some(MOD_ALT.0)
        );
    }

    #[test]
    fn parses_multiple_modifiers() {
        let parsed = parse_modifier_flags("Control, Shift").map(|value| value.0);
        assert_eq!(parsed, Some((MOD_CONTROL | MOD_SHIFT).0));
    }

    #[test]
    fn resolves_virtual_key_codes() {
        assert_eq!(virtual_key_code("PageDown"), Some(0x22));
        assert_eq!(virtual_key_code("A"), Some('A' as u32));
    }
}
