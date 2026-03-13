use std::{
    collections::HashMap,
    sync::mpsc::{self, RecvTimeoutError, Sender, SyncSender},
    thread,
    time::Duration,
};

use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{BOOL, COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            CreateDCW, DeleteDC, EnumDisplayMonitors, GetMonitorInfoW, GetStockObject, HBRUSH,
            HDC, HMONITOR, MONITORINFOEXW, BLACK_BRUSH,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            ColorSystem::SetDeviceGammaRamp,
            Magnification::{
                MagInitialize, MagSetFullscreenColorEffect, MagSetFullscreenTransform,
                MagUninitialize, MAGCOLOREFFECT,
            },
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, PeekMessageW,
                RegisterClassW, SetLayeredWindowAttributes, SetWindowPos, ShowWindow,
                TranslateMessage, HTTRANSPARENT, HWND_TOPMOST, LWA_ALPHA, MA_NOACTIVATE, MSG,
                PM_REMOVE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_SHOWWINDOW,
                WM_MOUSEACTIVATE, WM_NCHITTEST, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
                WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP, WS_VISIBLE,
            },
        },
    },
};

use crate::{
    models::{DimmingCapabilities, DimmingMethod},
    schedule::clamp_dim_precise,
};

const DISPLAY_DRIVER_NAME: &str = "DISPLAY";
const MAGNIFICATION_AVAILABLE_TEXT: &str =
    "Magnification uses a full-desktop Windows color effect without zoom and affects all displays at once.";
const MAGNIFICATION_UNAVAILABLE_TEXT: &str =
    "Magnification is unavailable in this process/runtime and has been disabled.";
type GammaRamp = [[u16; 256]; 3];

enum DimmingCommand {
    Sync {
        method: DimmingMethod,
        dim_percent: f64,
    },
    ResetAndAck {
        method: DimmingMethod,
        reply: SyncSender<()>,
    },
    Shutdown,
}

#[derive(Clone)]
pub struct DimmingManager {
    sender: Sender<DimmingCommand>,
}

impl DimmingManager {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel::<DimmingCommand>();
        thread::spawn(move || dimming_thread(receiver));
        Self { sender }
    }

    pub fn sync(&self, method: DimmingMethod, dim_percent: f64) {
        let _ = self.sender.send(DimmingCommand::Sync { method, dim_percent });
    }

    pub fn reset_to_full_brightness(&self, method: DimmingMethod) {
        let (reply, receiver) = mpsc::sync_channel(1);
        if self
            .sender
            .send(DimmingCommand::ResetAndAck { method, reply })
            .is_ok()
        {
            let _ = receiver.recv_timeout(Duration::from_secs(2));
        }
    }
}

impl Default for DimmingManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DimmingManager {
    fn drop(&mut self) {
        let _ = self.sender.send(DimmingCommand::Shutdown);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct MonitorInfo {
    device_name: String,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

struct OverlayEngine {
    class_name: Vec<u16>,
    overlays: HashMap<String, HWND>,
}

impl OverlayEngine {
    fn new(class_name: Vec<u16>) -> Self {
        Self {
            class_name,
            overlays: HashMap::new(),
        }
    }

    fn sync_monitors(&mut self, monitors: &[MonitorInfo]) -> bool {
        let names = monitors
            .iter()
            .map(|monitor| monitor.device_name.clone())
            .collect::<Vec<_>>();
        let removed = self
            .overlays
            .keys()
            .filter(|key| !names.contains(key))
            .cloned()
            .collect::<Vec<_>>();
        let mut changed = !removed.is_empty();

        for key in removed {
            if let Some(hwnd) = self.overlays.remove(&key) {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
            }
        }

        for monitor in monitors {
            let hwnd = if let Some(hwnd) = self.overlays.get(&monitor.device_name).copied() {
                hwnd
            } else {
                let hwnd = create_overlay_window(&self.class_name, monitor);
                self.overlays.insert(monitor.device_name.clone(), hwnd);
                changed = true;
                hwnd
            };

            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    monitor.left,
                    monitor.top,
                    monitor.width,
                    monitor.height,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
        }

        changed
    }

    fn apply(&self, dim_percent: f64) {
        for hwnd in self.overlays.values() {
            apply_overlay_alpha(*hwnd, dim_percent);
        }
    }

    fn shutdown(&mut self) {
        for hwnd in self.overlays.drain().map(|(_, hwnd)| hwnd) {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
        }
    }
}

struct GammaDevice {
    device_name: String,
    hdc: HDC,
}

impl GammaDevice {
    fn open(device_name: &str) -> Option<Self> {
        let driver = widestr(DISPLAY_DRIVER_NAME);
        let device = widestr(device_name);
        let hdc = unsafe {
            CreateDCW(
                PCWSTR(driver.as_ptr()),
                PCWSTR(device.as_ptr()),
                PCWSTR::null(),
                None,
            )
        };

        if hdc.0.is_null() {
            eprintln!("Failed to create display DC for {device_name}.");
            return None;
        }

        Some(Self {
            device_name: device_name.to_string(),
            hdc,
        })
    }

    fn apply(&self, ramp: &GammaRamp) -> bool {
        let success = unsafe { SetDeviceGammaRamp(self.hdc, ramp.as_ptr().cast()) }.as_bool();
        if !success {
            eprintln!("Failed to apply gamma ramp to {}.", self.device_name);
        }
        success
    }

    fn restore_identity(&self) -> bool {
        let identity = build_identity_gamma_ramp();
        let success = unsafe { SetDeviceGammaRamp(self.hdc, identity.as_ptr().cast()) }.as_bool();
        if !success {
            eprintln!("Failed to restore gamma ramp for {}.", self.device_name);
        }
        success
    }
}

impl Drop for GammaDevice {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteDC(self.hdc);
        }
    }
}

#[derive(Default)]
struct GammaEngine {
    devices: HashMap<String, GammaDevice>,
}

impl GammaEngine {
    fn sync_monitors(&mut self, monitors: &[MonitorInfo]) -> bool {
        let names = monitors
            .iter()
            .map(|monitor| monitor.device_name.clone())
            .collect::<Vec<_>>();
        let removed = self
            .devices
            .keys()
            .filter(|key| !names.contains(key))
            .cloned()
            .collect::<Vec<_>>();
        let mut changed = !removed.is_empty();

        for key in removed {
            if let Some(device) = self.devices.remove(&key) {
                let _ = device.restore_identity();
            }
        }

        for monitor in monitors {
            if self.devices.contains_key(&monitor.device_name) {
                continue;
            }

            if let Some(device) = GammaDevice::open(&monitor.device_name) {
                self.devices.insert(monitor.device_name.clone(), device);
                changed = true;
            }
        }

        changed
    }

    fn apply(&self, dim_percent: f64) -> bool {
        let ramp = build_gamma_ramp(dim_percent);
        let mut had_failures = false;

        for device in self.devices.values() {
            if !device.apply(&ramp) {
                had_failures = true;
            }
        }

        had_failures
    }

    fn shutdown(&mut self) {
        for (_, device) in self.devices.drain() {
            let _ = device.restore_identity();
        }
    }
}

#[derive(Default)]
struct MagnificationEngine {
    initialized: bool,
}

impl MagnificationEngine {
    fn ensure_initialized(&mut self) -> bool {
        if self.initialized {
            return true;
        }

        let success = unsafe { MagInitialize() }.as_bool();
        if !success {
            eprintln!("Failed to initialize Windows Magnification.");
            return false;
        }

        self.initialized = true;
        true
    }

    fn apply(&mut self, dim_percent: f64) -> bool {
        if !self.ensure_initialized() {
            return true;
        }

        let mut had_failures = false;
        let effect = build_magnification_effect(dim_percent);

        if !unsafe { MagSetFullscreenTransform(1.0, 0, 0) }.as_bool() {
            eprintln!("Failed to set Magnification fullscreen transform.");
            had_failures = true;
        }

        if !unsafe { MagSetFullscreenColorEffect(&effect) }.as_bool() {
            eprintln!("Failed to apply Magnification color effect.");
            had_failures = true;
        }

        had_failures
    }

    fn shutdown(&mut self) {
        if !self.initialized {
            return;
        }

        let identity = build_identity_magnification_effect();

        if !unsafe { MagSetFullscreenTransform(1.0, 0, 0) }.as_bool() {
            eprintln!("Failed to reset Magnification fullscreen transform.");
        }

        if !unsafe { MagSetFullscreenColorEffect(&identity) }.as_bool() {
            eprintln!("Failed to reset Magnification color effect.");
        }

        if !unsafe { MagUninitialize() }.as_bool() {
            eprintln!("Failed to uninitialize Windows Magnification.");
        }

        self.initialized = false;
    }
}

pub fn probe_dimming_capabilities() -> DimmingCapabilities {
    let initialized = unsafe { MagInitialize() }.as_bool();
    if !initialized {
        return DimmingCapabilities {
            magnification_available: false,
            magnification_status_text: MAGNIFICATION_UNAVAILABLE_TEXT.to_string(),
        };
    }

    let uninitialized = unsafe { MagUninitialize() }.as_bool();
    if !uninitialized {
        eprintln!("Magnification capability probe could not uninitialize cleanly.");
        return DimmingCapabilities {
            magnification_available: false,
            magnification_status_text: MAGNIFICATION_UNAVAILABLE_TEXT.to_string(),
        };
    }

    DimmingCapabilities {
        magnification_available: true,
        magnification_status_text: MAGNIFICATION_AVAILABLE_TEXT.to_string(),
    }
}

unsafe extern "system" fn overlay_window_proc(
    hwnd: HWND,
    message: u32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if message == WM_NCHITTEST {
        return LRESULT(HTTRANSPARENT as isize);
    }

    if message == WM_MOUSEACTIVATE {
        return LRESULT(MA_NOACTIVATE as isize);
    }

    DefWindowProcW(hwnd, message, w_param, l_param)
}

fn dimming_thread(receiver: mpsc::Receiver<DimmingCommand>) {
    let class_name = widestr("DimsomeOverlayWindow");
    register_overlay_window_class(&class_name);

    let mut overlay_engine = OverlayEngine::new(class_name);
    let mut gamma_engine = GammaEngine::default();
    let mut magnification_engine = MagnificationEngine::default();
    let mut active_method: Option<DimmingMethod> = None;
    let mut last_applied_dim_percent: Option<f64> = None;
    let mut needs_apply = true;

    loop {
        match receiver.recv_timeout(Duration::from_millis(250)) {
            Ok(DimmingCommand::Sync { method, dim_percent }) => {
                let dim_percent = clamp_dim_precise(dim_percent);

                if active_method.as_ref() != Some(&method) {
                    deactivate_active_method(
                        active_method.as_ref(),
                        &mut overlay_engine,
                        &mut gamma_engine,
                        &mut magnification_engine,
                    );
                    active_method = Some(method.clone());
                    last_applied_dim_percent = None;
                    needs_apply = true;
                }

                let monitors_changed = match method {
                    DimmingMethod::Overlay => {
                        let monitors = enumerate_monitors();
                        overlay_engine.sync_monitors(&monitors)
                    }
                    DimmingMethod::Gamma => {
                        let monitors = enumerate_monitors();
                        gamma_engine.sync_monitors(&monitors)
                    }
                    DimmingMethod::Magnification => false,
                };

                if monitors_changed || last_applied_dim_percent != Some(dim_percent) {
                    needs_apply = true;
                }

                if needs_apply {
                    let had_failures = match method {
                        DimmingMethod::Overlay => {
                            overlay_engine.apply(dim_percent);
                            false
                        }
                        DimmingMethod::Gamma => gamma_engine.apply(dim_percent),
                        DimmingMethod::Magnification => magnification_engine.apply(dim_percent),
                    };
                    last_applied_dim_percent = Some(dim_percent);
                    needs_apply = had_failures;
                }
            }
            Ok(DimmingCommand::ResetAndAck { method, reply }) => {
                let dim_percent = 0.0;

                if active_method.as_ref() != Some(&method) {
                    deactivate_active_method(
                        active_method.as_ref(),
                        &mut overlay_engine,
                        &mut gamma_engine,
                        &mut magnification_engine,
                    );
                    active_method = Some(method.clone());
                }

                match method {
                    DimmingMethod::Overlay => {
                        let monitors = enumerate_monitors();
                        let _ = overlay_engine.sync_monitors(&monitors);
                        overlay_engine.apply(dim_percent);
                    }
                    DimmingMethod::Gamma => {
                        let monitors = enumerate_monitors();
                        let _ = gamma_engine.sync_monitors(&monitors);
                        let _ = gamma_engine.apply(dim_percent);
                    }
                    DimmingMethod::Magnification => {
                        let _ = magnification_engine.apply(dim_percent);
                        magnification_engine.shutdown();
                        active_method = None;
                    }
                }

                last_applied_dim_percent = Some(dim_percent);
                needs_apply = false;
                let _ = reply.send(());
            }
            Ok(DimmingCommand::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }

        pump_messages();
    }

    deactivate_active_method(
        active_method.as_ref(),
        &mut overlay_engine,
        &mut gamma_engine,
        &mut magnification_engine,
    );
}

fn deactivate_active_method(
    method: Option<&DimmingMethod>,
    overlay_engine: &mut OverlayEngine,
    gamma_engine: &mut GammaEngine,
    magnification_engine: &mut MagnificationEngine,
) {
    match method {
        Some(DimmingMethod::Overlay) => overlay_engine.shutdown(),
        Some(DimmingMethod::Gamma) => gamma_engine.shutdown(),
        Some(DimmingMethod::Magnification) => magnification_engine.shutdown(),
        None => {}
    }
}

fn register_overlay_window_class(class_name: &[u16]) {
    unsafe {
        let instance = GetModuleHandleW(PCWSTR::null()).expect("overlay module handle");
        let wnd_class = WNDCLASSW {
            lpfnWndProc: Some(overlay_window_proc),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            hInstance: instance.into(),
            hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
            ..Default::default()
        };
        let _ = RegisterClassW(&wnd_class);
    }
}

fn pump_messages() {
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).into() {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }
    }
}

fn create_overlay_window(class_name: &[u16], monitor: &MonitorInfo) -> HWND {
    let title = widestr(&format!("Dimsome Overlay {}", monitor.device_name));
    unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST
                | WS_EX_TOOLWINDOW
                | WS_EX_NOACTIVATE
                | WS_EX_LAYERED
                | WS_EX_TRANSPARENT,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_POPUP | WS_VISIBLE,
            monitor.left,
            monitor.top,
            monitor.width,
            monitor.height,
            None,
            None,
            GetModuleHandleW(PCWSTR::null()).expect("overlay module handle"),
            None,
        )
        .expect("overlay window creation failed")
    }
}

fn apply_overlay_alpha(hwnd: HWND, dim_percent: f64) {
    let alpha = ((clamp_dim_precise(dim_percent) / 100.0) * 255.0).round() as u8;
    unsafe {
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
    }
}

fn build_identity_gamma_ramp() -> GammaRamp {
    build_gamma_ramp(0.0)
}

fn build_gamma_ramp(dim_percent: f64) -> GammaRamp {
    let brightness_scale = 1.0 - (clamp_dim_precise(dim_percent) / 100.0);
    let mut ramp = [[0u16; 256]; 3];

    for index in 0..256 {
        let identity_value = (index as u32) * 257;
        let scaled_value = ((identity_value as f64) * brightness_scale)
            .round()
            .clamp(0.0, u16::MAX as f64) as u16;

        ramp[0][index] = scaled_value;
        ramp[1][index] = scaled_value;
        ramp[2][index] = scaled_value;
    }

    ramp
}

fn build_identity_magnification_effect() -> MAGCOLOREFFECT {
    build_magnification_effect(0.0)
}

fn build_magnification_effect(dim_percent: f64) -> MAGCOLOREFFECT {
    let brightness_scale = (1.0 - (clamp_dim_precise(dim_percent) / 100.0)) as f32;

    MAGCOLOREFFECT {
        transform: [
            brightness_scale,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            brightness_scale,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            brightness_scale,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ],
    }
}

fn enumerate_monitors() -> Vec<MonitorInfo> {
    unsafe extern "system" fn callback(
        hmonitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        let monitors = &mut *(data.0 as *mut Vec<MonitorInfo>);
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(hmonitor, &mut info as *mut _ as *mut _).as_bool() {
            let end = info
                .szDevice
                .iter()
                .position(|ch| *ch == 0)
                .unwrap_or(info.szDevice.len());
            let device_name = String::from_utf16_lossy(&info.szDevice[..end]);
            monitors.push(MonitorInfo {
                device_name,
                left: info.monitorInfo.rcMonitor.left,
                top: info.monitorInfo.rcMonitor.top,
                width: info.monitorInfo.rcMonitor.right - info.monitorInfo.rcMonitor.left,
                height: info.monitorInfo.rcMonitor.bottom - info.monitorInfo.rcMonitor.top,
            });
        }
        true.into()
    }

    let mut monitors = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(callback),
            LPARAM((&mut monitors as *mut Vec<MonitorInfo>) as isize),
        );
    }
    monitors
}

fn widestr(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(left: f32, right: f32) {
        assert!(
            (left - right).abs() < 1e-6,
            "expected {left} to be close to {right}"
        );
    }

    #[test]
    fn gamma_ramp_is_identity_at_zero_dim() {
        let ramp = build_gamma_ramp(0.0);

        assert_eq!(ramp[0][0], 0);
        assert_eq!(ramp[0][255], u16::MAX);
        assert_eq!(ramp[0], ramp[1]);
        assert_eq!(ramp[1], ramp[2]);
    }

    #[test]
    fn gamma_ramp_darkens_monotonically_as_dim_increases() {
        let lighter_ramp = build_gamma_ramp(25.0);
        let darker_ramp = build_gamma_ramp(75.0);

        assert!(lighter_ramp[0][128] > darker_ramp[0][128]);

        for channel in 0..3 {
            for index in 1..256 {
                assert!(lighter_ramp[channel][index] >= lighter_ramp[channel][index - 1]);
                assert!(darker_ramp[channel][index] >= darker_ramp[channel][index - 1]);
            }
        }
    }

    #[test]
    fn gamma_ramp_stays_within_bounds_for_high_dim_values() {
        let ramp = build_gamma_ramp(99.0);

        for channel in ramp {
            for value in channel {
                assert!(value <= u16::MAX);
            }
        }
    }

    #[test]
    fn magnification_effect_is_identity_at_zero_dim() {
        let effect = build_magnification_effect(0.0);

        assert_close(effect.transform[0], 1.0);
        assert_close(effect.transform[6], 1.0);
        assert_close(effect.transform[12], 1.0);
        assert_close(effect.transform[18], 1.0);
        assert_close(effect.transform[24], 1.0);
    }

    #[test]
    fn magnification_effect_darkens_monotonically_as_dim_increases() {
        let lighter_effect = build_magnification_effect(25.0);
        let darker_effect = build_magnification_effect(75.0);

        assert!(lighter_effect.transform[0] > darker_effect.transform[0]);
        assert_close(lighter_effect.transform[0], lighter_effect.transform[6]);
        assert_close(lighter_effect.transform[6], lighter_effect.transform[12]);
        assert_close(darker_effect.transform[0], darker_effect.transform[6]);
        assert_close(darker_effect.transform[6], darker_effect.transform[12]);
    }

    #[test]
    fn magnification_effect_preserves_alpha_and_translation_rows() {
        let effect = build_magnification_effect(60.0);

        assert_close(effect.transform[15], 0.0);
        assert_close(effect.transform[16], 0.0);
        assert_close(effect.transform[17], 0.0);
        assert_close(effect.transform[18], 1.0);
        assert_close(effect.transform[19], 0.0);
        assert_close(effect.transform[20], 0.0);
        assert_close(effect.transform[21], 0.0);
        assert_close(effect.transform[22], 0.0);
        assert_close(effect.transform[23], 0.0);
        assert_close(effect.transform[24], 1.0);
    }
}
