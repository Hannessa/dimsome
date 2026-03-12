use std::{
    collections::HashMap,
    sync::mpsc::{self, RecvTimeoutError, Sender},
    thread,
    time::Duration,
};

use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{BOOL, COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            EnumDisplayMonitors, GetMonitorInfoW, GetStockObject, HBRUSH, HDC, HMONITOR,
            MONITORINFOEXW, MonitorFromRect, BLACK_BRUSH, MONITOR_DEFAULTTONEAREST,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, PeekMessageW,
            RegisterClassW, SetLayeredWindowAttributes, SetWindowPos, ShowWindow, TranslateMessage,
            HTTRANSPARENT, HWND_TOPMOST, LWA_ALPHA, MA_NOACTIVATE, MSG, PM_REMOVE,
            SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_SHOWWINDOW, WM_MOUSEACTIVATE,
            WM_NCHITTEST, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
            WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP, WS_VISIBLE,
        },
    },
};

use crate::schedule::clamp_dim_precise;

enum OverlayCommand {
    Refresh,
    Apply(f64),
    Shutdown,
}

#[derive(Clone)]
pub struct OverlayManager {
    sender: Sender<OverlayCommand>,
}

impl OverlayManager {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel::<OverlayCommand>();
        thread::spawn(move || overlay_thread(receiver));
        Self { sender }
    }

    pub fn refresh(&mut self) {
        let _ = self.sender.send(OverlayCommand::Refresh);
    }

    pub fn apply(&mut self, dim_percent: f64) {
        let _ = self.sender.send(OverlayCommand::Apply(dim_percent));
    }
}

impl Default for OverlayManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for OverlayManager {
    fn drop(&mut self) {
        let _ = self.sender.send(OverlayCommand::Shutdown);
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

fn overlay_thread(receiver: mpsc::Receiver<OverlayCommand>) {
    let class_name = widestr("DimsomeOverlayWindow");
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

    let mut overlays: HashMap<String, HWND> = HashMap::new();
    let mut last_applied_dim_percent = 0.0;

    loop {
        match receiver.recv_timeout(Duration::from_millis(250)) {
            Ok(OverlayCommand::Refresh) => {
                let monitors = enumerate_monitors();
                let names = monitors
                    .iter()
                    .map(|monitor| monitor.device_name.clone())
                    .collect::<Vec<_>>();

                let removed = overlays
                    .keys()
                    .filter(|key| !names.contains(key))
                    .cloned()
                    .collect::<Vec<_>>();

                for key in removed {
                    if let Some(hwnd) = overlays.remove(&key) {
                        unsafe {
                            let _ = DestroyWindow(hwnd);
                        }
                    }
                }

                for monitor in monitors {
                    let hwnd = overlays
                        .entry(monitor.device_name.clone())
                        .or_insert_with(|| create_overlay_window(&class_name, &monitor));
                    unsafe {
                        let _ = SetWindowPos(
                            *hwnd,
                            HWND_TOPMOST,
                            monitor.left,
                            monitor.top,
                            monitor.width,
                            monitor.height,
                            SWP_NOACTIVATE | SWP_SHOWWINDOW,
                        );
                        let _ = ShowWindow(*hwnd, SW_SHOWNOACTIVATE);
                    }
                    apply_overlay_alpha(*hwnd, last_applied_dim_percent);
                }
            }
            Ok(OverlayCommand::Apply(dim_percent)) => {
                last_applied_dim_percent = clamp_dim_precise(dim_percent);
                for hwnd in overlays.values() {
                    apply_overlay_alpha(*hwnd, last_applied_dim_percent);
                }
            }
            Ok(OverlayCommand::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }

        pump_messages();
    }

    for hwnd in overlays.into_values() {
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
}

pub fn calculate_quick_panel_position(
    anchor_left: i32,
    anchor_top: i32,
    anchor_width: i32,
    anchor_height: i32,
    panel_width: i32,
    panel_height: i32,
) -> (i32, i32) {
    const SPACING: i32 = 12;

    let anchor_rect = RECT {
        left: anchor_left,
        top: anchor_top,
        right: anchor_left + anchor_width.max(1),
        bottom: anchor_top + anchor_height.max(1),
    };

    unsafe {
        let monitor = MonitorFromRect(&anchor_rect, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

        if GetMonitorInfoW(monitor, &mut info as *mut _ as *mut _).as_bool() {
            let monitor_rect = info.monitorInfo.rcMonitor;
            let work_area = info.monitorInfo.rcWork;

            let anchor_right = anchor_rect.right;
            let anchor_bottom = anchor_rect.bottom;
            let taskbar_bottom = work_area.bottom < monitor_rect.bottom;
            let taskbar_top = work_area.top > monitor_rect.top;
            let taskbar_left = work_area.left > monitor_rect.left;
            let centered_x = ((anchor_left + anchor_right) / 2) - (panel_width / 2);

            let min_x = work_area.left + SPACING;
            let max_x = work_area.right - panel_width - SPACING;
            let min_y = work_area.top + SPACING;
            let max_y = work_area.bottom - panel_height;

            let mut x = clamp_to_work_area(centered_x, min_x, max_x);
            let mut y = if taskbar_bottom {
                max_y
            } else if taskbar_top {
                anchor_bottom + SPACING
            } else {
                anchor_top
            };

            if taskbar_left {
                x = anchor_right + SPACING;
                y = clamp_to_work_area(anchor_top, min_y, work_area.bottom - panel_height - SPACING);
            }

            if !taskbar_bottom && !taskbar_top && !taskbar_left {
                x = anchor_left - panel_width - SPACING;
                y = clamp_to_work_area(anchor_top, min_y, work_area.bottom - panel_height - SPACING);
            }

            x = clamp_to_work_area(x, min_x, max_x);
            y = clamp_to_work_area(y, min_y, max_y);
            return (x, y);
        }
    }

    (
        anchor_left.max(0),
        (anchor_top - panel_height - SPACING).max(0),
    )
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
            GetModuleHandleW(PCWSTR::null())
                .expect("overlay module handle"),
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

fn clamp_to_work_area(value: i32, min: i32, max: i32) -> i32 {
    if max < min {
        min
    } else {
        value.clamp(min, max)
    }
}




