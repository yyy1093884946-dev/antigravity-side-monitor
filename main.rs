#![windows_subsystem = "windows"]

use std::collections::HashMap;
use std::env;
use std::ffi::c_void;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::mem::{size_of, zeroed};
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// ========================= Win32 & GDI+ FFI =========================

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct RECT {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl RECT {
    fn width(&self) -> i32 {
        self.right - self.left
    }
    fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct POINT {
    x: i32,
    y: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct SIZE {
    cx: i32,
    cy: i32,
}

#[repr(C)]
struct MSG {
    hwnd: *mut c_void,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    pt: POINT,
}

type WndProc = extern "system" fn(*mut c_void, u32, usize, isize) -> isize;
type EnumWindowsProc = extern "system" fn(*mut c_void, isize) -> i32;
type WinEventProc = extern "system" fn(*mut c_void, u32, *mut c_void, i32, i32, u32, u32);

#[repr(C)]
struct WNDCLASSEXW {
    cb_size: u32,
    style: u32,
    lpfn_wnd_proc: WndProc,
    cb_cls_extra: i32,
    cb_wnd_extra: i32,
    h_instance: *mut c_void,
    h_icon: *mut c_void,
    h_cursor: *mut c_void,
    hbr_background: *mut c_void,
    lpsz_menu_name: *const u16,
    lpsz_class_name: *const u16,
    h_icon_sm: *mut c_void,
}

#[repr(C)]
struct MONITORINFO {
    cb_size: u32,
    rc_monitor: RECT,
    rc_work: RECT,
    dw_flags: u32,
}

#[repr(C)]
struct BITMAPINFOHEADER {
    bi_size: u32,
    bi_width: i32,
    bi_height: i32,
    bi_planes: u16,
    bi_bit_count: u16,
    bi_compression: u32,
    bi_size_image: u32,
    bi_xpels_per_meter: i32,
    bi_ypels_per_meter: i32,
    bi_clr_used: u32,
    bi_clr_important: u32,
}

#[repr(C)]
struct BLENDFUNCTION {
    blend_op: u8,
    blend_flags: u8,
    source_constant_alpha: u8,
    alpha_format: u8,
}

#[repr(C)]
struct GdiplusStartupInput {
    gdiplus_version: u32,
    debug_event_callback: *const c_void,
    suppress_background_thread: i32,
    suppress_external_codecs: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RectF {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[repr(C)]
struct ICONINFO {
    f_icon: i32,
    x_hotspot: u32,
    y_hotspot: u32,
    hbm_mask: *mut c_void,
    hbm_color: *mut c_void,
}

#[repr(C)]
struct NOTIFYICONDATAW {
    cb_size: u32,
    h_wnd: *mut c_void,
    u_id: u32,
    u_flags: u32,
    u_callback_message: u32,
    h_icon: *mut c_void,
    sz_tip: [u16; 128],
    dw_state: u32,
    dw_state_mask: u32,
    sz_info: [u16; 256],
    u_timeout_or_version: u32,
    sz_info_title: [u16; 64],
    dw_info_flags: u32,
    guid_item: [u8; 16],
    h_balloon_icon: *mut c_void,
}

#[repr(C)]
struct PROCESS_MEMORY_COUNTERS {
    cb: u32,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
}

#[repr(C)]
struct SYSTEMTIME {
    w_year: u16,
    w_month: u16,
    w_day_of_week: u16,
    w_day: u16,
    w_hour: u16,
    w_minute: u16,
    w_second: u16,
    w_milliseconds: u16,
}

const WS_POPUP: u32 = 0x8000_0000;
const WS_EX_TOPMOST: u32 = 0x0000_0008;
const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
const WS_EX_LAYERED: u32 = 0x0008_0000;
const WS_EX_NOACTIVATE: u32 = 0x0800_0000;

const SW_HIDE: i32 = 0;
const SW_SHOWNOACTIVATE: i32 = 4;

const SWP_NOACTIVATE: u32 = 0x0010;
const SWP_SHOWWINDOW: u32 = 0x0040;

const HWND_TOP: *mut c_void = 0isize as *mut c_void;
const HWND_TOPMOST: *mut c_void = (-1isize) as *mut c_void;
const HWND_NOTOPMOST: *mut c_void = (-2isize) as *mut c_void;

const GW_HWNDPREV: u32 = 3;
const MONITOR_DEFAULTTONEAREST: u32 = 2;
const DWMWA_EXTENDED_FRAME_BOUNDS: u32 = 9;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
const ERROR_ALREADY_EXISTS: u32 = 183;

const EVENT_SYSTEM_FOREGROUND: u32 = 0x0003;
const EVENT_SYSTEM_MINIMIZESTART: u32 = 0x0016;
const EVENT_SYSTEM_MINIMIZEEND: u32 = 0x0017;
const EVENT_OBJECT_DESTROY: u32 = 0x8001;
const EVENT_OBJECT_LOCATIONCHANGE: u32 = 0x800B;
const WINEVENT_OUTOFCONTEXT: u32 = 0x0000;
const WINEVENT_SKIPOWNPROCESS: u32 = 0x0002;

const WM_NULL: u32 = 0x0000;
const WM_DESTROY: u32 = 0x0002;
const WM_TIMER: u32 = 0x0113;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_RBUTTONUP: u32 = 0x0205;
const WM_APP: u32 = 0x8000;
const WM_TRAYICON: u32 = WM_APP + 1;

const ULW_ALPHA: u32 = 0x0000_0002;
const AC_SRC_OVER: u8 = 0x00;
const AC_SRC_ALPHA: u8 = 0x01;
const DIB_RGB_COLORS: u32 = 0;

const NIM_ADD: u32 = 0x0000_0000;
const NIM_DELETE: u32 = 0x0000_0002;
const NIF_MESSAGE: u32 = 0x0000_0001;
const NIF_ICON: u32 = 0x0000_0002;
const NIF_TIP: u32 = 0x0000_0004;

const MF_STRING: u32 = 0x0000;
const MF_SEPARATOR: u32 = 0x0800;
const MF_CHECKED: u32 = 0x0008;
const TPM_RIGHTBUTTON: u32 = 0x0002;
const TPM_RETURNCMD: u32 = 0x0100;

const IDC_ARROW: usize = 32512;

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(lpModuleName: *const u16) -> *mut c_void;
    fn CreateMutexW(lpMutexAttributes: *mut c_void, bInitialOwner: i32, lpName: *const u16) -> *mut c_void;
    fn GetLastError() -> u32;
    fn CloseHandle(hObject: *mut c_void) -> i32;
    fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> *mut c_void;
    fn QueryFullProcessImageNameW(hProcess: *mut c_void, dwFlags: u32, lpExeName: *mut u16, lpdwSize: *mut u32) -> i32;
    fn GetCurrentProcess() -> *mut c_void;
    fn SetProcessWorkingSetSize(hProcess: *mut c_void, dwMinimumWorkingSetSize: usize, dwMaximumWorkingSetSize: usize) -> i32;
    fn AttachConsole(dwProcessId: i32) -> i32;
    fn K32GetProcessMemoryInfo(process: *mut c_void, ppsmemCounters: *mut PROCESS_MEMORY_COUNTERS, cb: u32) -> i32;
    fn GetLocalTime(lpSystemTime: *mut SYSTEMTIME);
}

#[link(name = "user32")]
extern "system" {
    fn SetProcessDpiAwarenessContext(value: *mut c_void) -> i32;
    fn SetProcessDPIAware() -> i32;
    fn GetDpiForWindow(hwnd: *mut c_void) -> u32;
    fn LoadCursorW(hInstance: *mut c_void, lpCursorName: *const u16) -> *mut c_void;
    fn RegisterClassExW(lpwcx: *const WNDCLASSEXW) -> u16;
    fn CreateWindowExW(
        dwExStyle: u32,
        lpClassName: *const u16,
        lpWindowName: *const u16,
        dwStyle: u32,
        x: i32,
        y: i32,
        nWidth: i32,
        nHeight: i32,
        hWndParent: *mut c_void,
        hMenu: *mut c_void,
        hInstance: *mut c_void,
        lpParam: *mut c_void,
    ) -> *mut c_void;
    fn DestroyWindow(hWnd: *mut c_void) -> i32;
    fn DefWindowProcW(hWnd: *mut c_void, msg: u32, wParam: usize, lParam: isize) -> isize;
    fn PostQuitMessage(nExitCode: i32);
    fn GetMessageW(lpMsg: *mut MSG, hWnd: *mut c_void, wMsgFilterMin: u32, wMsgFilterMax: u32) -> i32;
    fn TranslateMessage(lpMsg: *const MSG) -> i32;
    fn DispatchMessageW(lpMsg: *const MSG) -> isize;
    fn PostMessageW(hWnd: *mut c_void, msg: u32, wParam: usize, lParam: isize) -> i32;
    fn ShowWindow(hWnd: *mut c_void, nCmdShow: i32) -> i32;
    fn SetWindowPos(hWnd: *mut c_void, hWndInsertAfter: *mut c_void, x: i32, y: i32, cx: i32, cy: i32, uFlags: u32) -> i32;
    fn UpdateLayeredWindow(
        hWnd: *mut c_void,
        hdcDst: *mut c_void,
        pptDst: *const POINT,
        psize: *const SIZE,
        hdcSrc: *mut c_void,
        pptSrc: *const POINT,
        crKey: u32,
        pblend: *const BLENDFUNCTION,
        dwFlags: u32,
    ) -> i32;
    fn SetTimer(hWnd: *mut c_void, nIDEvent: usize, uElapse: u32, lpTimerFunc: *const c_void) -> usize;
    fn KillTimer(hWnd: *mut c_void, uIDEvent: usize) -> i32;
    fn EnumWindows(lpEnumFunc: EnumWindowsProc, lParam: isize) -> i32;
    fn IsWindow(hWnd: *mut c_void) -> i32;
    fn IsWindowVisible(hWnd: *mut c_void) -> i32;
    fn IsIconic(hWnd: *mut c_void) -> i32;
    fn GetWindowThreadProcessId(hWnd: *mut c_void, lpdwProcessId: *mut u32) -> u32;
    fn GetWindowRect(hWnd: *mut c_void, lpRect: *mut RECT) -> i32;
    fn GetForegroundWindow() -> *mut c_void;
    fn SetForegroundWindow(hWnd: *mut c_void) -> i32;
    fn GetWindow(hWnd: *mut c_void, uCmd: u32) -> *mut c_void;
    fn MonitorFromRect(lprc: *const RECT, dwFlags: u32) -> *mut c_void;
    fn GetMonitorInfoW(hMonitor: *mut c_void, lpmi: *mut MONITORINFO) -> i32;
    fn SetWinEventHook(
        eventMin: u32,
        eventMax: u32,
        hmodWinEventProc: *mut c_void,
        pfnWinEventProc: WinEventProc,
        idProcess: u32,
        idThread: u32,
        dwFlags: u32,
    ) -> *mut c_void;
    fn UnhookWinEvent(hWinEventHook: *mut c_void) -> i32;
    fn GetCursorPos(lpPoint: *mut POINT) -> i32;
    fn SetCapture(hWnd: *mut c_void) -> *mut c_void;
    fn ReleaseCapture() -> i32;
    fn GetDC(hWnd: *mut c_void) -> *mut c_void;
    fn ReleaseDC(hWnd: *mut c_void, hDC: *mut c_void) -> i32;
    fn CreatePopupMenu() -> *mut c_void;
    fn AppendMenuW(hMenu: *mut c_void, uFlags: u32, uIDNewItem: usize, lpNewItem: *const u16) -> i32;
    fn TrackPopupMenu(hMenu: *mut c_void, uFlags: u32, x: i32, y: i32, nReserved: i32, hWnd: *mut c_void, prcRect: *const RECT) -> i32;
    fn DestroyMenu(hMenu: *mut c_void) -> i32;
    fn CreateIconIndirect(piconinfo: *const ICONINFO) -> *mut c_void;
    fn DestroyIcon(hIcon: *mut c_void) -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    fn CreateCompatibleDC(hdc: *mut c_void) -> *mut c_void;
    fn DeleteDC(hdc: *mut c_void) -> i32;
    fn SelectObject(hdc: *mut c_void, h: *mut c_void) -> *mut c_void;
    fn DeleteObject(ho: *mut c_void) -> i32;
    fn CreateDIBSection(
        hdc: *mut c_void,
        pbmi: *const BITMAPINFOHEADER,
        usage: u32,
        ppvBits: *mut *mut c_void,
        hSection: *mut c_void,
        offset: u32,
    ) -> *mut c_void;
    fn CreateBitmap(nWidth: i32, nHeight: i32, nPlanes: u32, nBitCount: u32, lpBits: *const c_void) -> *mut c_void;
}

#[link(name = "dwmapi")]
extern "system" {
    fn DwmGetWindowAttribute(hwnd: *mut c_void, dwAttribute: u32, pvAttribute: *mut RECT, cbAttribute: u32) -> i32;
}

#[link(name = "shell32")]
extern "system" {
    fn Shell_NotifyIconW(dwMessage: u32, lpData: *const NOTIFYICONDATAW) -> i32;
    fn ShellExecuteW(
        hwnd: *mut c_void,
        lpOperation: *const u16,
        lpFile: *const u16,
        lpParameters: *const u16,
        lpDirectory: *const u16,
        nShowCmd: i32,
    ) -> isize;
}

#[link(name = "gdiplus")]
extern "system" {
    fn GdiplusStartup(token: *mut usize, input: *const GdiplusStartupInput, output: *mut c_void) -> i32;
    fn GdiplusShutdown(token: usize);
    fn GdipCreateBitmapFromScan0(
        width: i32,
        height: i32,
        stride: i32,
        format: i32,
        scan0: *mut u8,
        bitmap: *mut *mut c_void,
    ) -> i32;
    fn GdipGetImageGraphicsContext(image: *mut c_void, graphics: *mut *mut c_void) -> i32;
    fn GdipSetSmoothingMode(graphics: *mut c_void, smoothingMode: i32) -> i32;
    fn GdipSetTextRenderingHint(graphics: *mut c_void, mode: i32) -> i32;
    fn GdipCreateSolidFill(color: u32, brush: *mut *mut c_void) -> i32;
    fn GdipDeleteBrush(brush: *mut c_void) -> i32;
    fn GdipCreatePen1(color: u32, width: f32, unit: i32, pen: *mut *mut c_void) -> i32;
    fn GdipDeletePen(pen: *mut c_void) -> i32;
    fn GdipCreatePath(brushMode: i32, path: *mut *mut c_void) -> i32;
    fn GdipAddPathArc(path: *mut c_void, x: f32, y: f32, width: f32, height: f32, startAngle: f32, sweepAngle: f32) -> i32;
    fn GdipClosePathFigure(path: *mut c_void) -> i32;
    fn GdipFillPath(graphics: *mut c_void, brush: *mut c_void, path: *mut c_void) -> i32;
    fn GdipDrawPath(graphics: *mut c_void, pen: *mut c_void, path: *mut c_void) -> i32;
    fn GdipDeletePath(path: *mut c_void) -> i32;
    fn GdipDrawArc(graphics: *mut c_void, pen: *mut c_void, x: f32, y: f32, width: f32, height: f32, startAngle: f32, sweepAngle: f32) -> i32;
    fn GdipDrawLine(graphics: *mut c_void, pen: *mut c_void, x1: f32, y1: f32, x2: f32, y2: f32) -> i32;
    fn GdipCreateFontFamilyFromName(name: *const u16, fontCollection: *mut c_void, fontFamily: *mut *mut c_void) -> i32;
    fn GdipDeleteFontFamily(fontFamily: *mut c_void) -> i32;
    fn GdipCreateFont(fontFamily: *mut c_void, emSize: f32, style: i32, unit: i32, font: *mut *mut c_void) -> i32;
    fn GdipDeleteFont(font: *mut c_void) -> i32;
    fn GdipCreateStringFormat(formatAttributes: i32, language: u16, format: *mut *mut c_void) -> i32;
    fn GdipSetStringFormatAlign(format: *mut c_void, align: i32) -> i32;
    fn GdipSetStringFormatLineAlign(format: *mut c_void, align: i32) -> i32;
    fn GdipSetStringFormatTrimming(format: *mut c_void, trimming: i32) -> i32;
    fn GdipSetStringFormatFlags(format: *mut c_void, flags: i32) -> i32;
    fn GdipDeleteStringFormat(format: *mut c_void) -> i32;
    fn GdipDrawString(
        graphics: *mut c_void,
        string: *const u16,
        length: i32,
        font: *mut c_void,
        layoutRect: *const RectF,
        stringFormat: *mut c_void,
        brush: *mut c_void,
    ) -> i32;
    fn GdipDeleteGraphics(graphics: *mut c_void) -> i32;
    fn GdipDisposeImage(image: *mut c_void) -> i32;
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn enable_dpi_awareness() {
    unsafe {
        if SetProcessDpiAwarenessContext((-4isize) as *mut c_void) != 0 {
            return;
        }
        SetProcessDPIAware();
    }
}

fn trim_working_set() {
    unsafe {
        SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

fn current_working_set_mb() -> f64 {
    unsafe {
        let mut pmc: PROCESS_MEMORY_COUNTERS = zeroed();
        pmc.cb = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        if K32GetProcessMemoryInfo(GetCurrentProcess(), &mut pmc, pmc.cb) != 0 {
            pmc.working_set_size as f64 / (1024.0 * 1024.0)
        } else {
            0.0
        }
    }
}

// ========================= Inside Docking Math =========================

#[derive(Clone, Copy)]
struct DockConfig {
    offset_from_right_dip: i32,
    offset_from_top_dip: i32,
}

impl Default for DockConfig {
    fn default() -> Self {
        DockConfig {
            offset_from_right_dip: 152, // Clears — □ × titlebar controls cleanly
            offset_from_top_dip: 4,     // Vertically centered in #111015 titlebar
        }
    }
}

fn compute_inside_position(
    ag_rect: RECT,
    work_area: RECT,
    win_w: i32,
    win_h: i32,
    cfg: DockConfig,
    scale: f32,
) -> POINT {
    let pad = (6.0 * scale).round() as i32;
    let right_off = (cfg.offset_from_right_dip as f32 * scale).round() as i32;
    let top_off = (cfg.offset_from_top_dip as f32 * scale).round() as i32;

    let mut x = ag_rect.right - right_off - win_w;
    let mut y = ag_rect.top + top_off;

    // Clamp strictly inside Antigravity window bounds
    let ag_min_x = ag_rect.left + pad;
    let ag_max_x = (ag_rect.right - win_w - pad).max(ag_min_x);
    if x < ag_min_x {
        x = ag_min_x;
    }
    if x > ag_max_x {
        x = ag_max_x;
    }

    let ag_min_y = ag_rect.top + 2;
    let ag_max_y = (ag_rect.bottom - win_h - pad).max(ag_min_y);
    if y < ag_min_y {
        y = ag_min_y;
    }
    if y > ag_max_y {
        y = ag_max_y;
    }

    // Clamp inside monitor working area as safety guard
    let wa_min_x = work_area.left + 2;
    let wa_max_x = (work_area.right - win_w - 2).max(wa_min_x);
    if x < wa_min_x {
        x = wa_min_x;
    }
    if x > wa_max_x {
        x = wa_max_x;
    }

    let wa_min_y = work_area.top + 2;
    let wa_max_y = (work_area.bottom - win_h - 2).max(wa_min_y);
    if y < wa_min_y {
        y = wa_min_y;
    }
    if y > wa_max_y {
        y = wa_max_y;
    }

    POINT { x, y }
}

// ========================= Zero-DOM Incremental Transcript Analyzer =========================

#[derive(Clone, Default, PartialEq)]
struct MonitorSnapshot {
    has_data: bool,
    session_id: String,
    model: String,
    last_updated_hms: String,
    context_used: u64,
    latest_output_tokens: u64,
    latest_duration_sec: f64,
    latest_tps: f64,
    avg_tps: f64,
    latest_hit_rate: f64,
    session_hit_rate: f64,
    total_steps: u32,
    total_cache_read: u64,
    total_uncached_in: u64,
}

#[derive(Clone, Default)]
struct ParsedStep {
    step_index: Option<i64>,
    created_at_sec: Option<f64>,
    model: Option<String>,
    input_tokens: Option<u64>,
    cache_read_tokens: Option<u64>,
    output_tokens: Option<u64>,
}

fn parse_iso8601_utc(s: &str) -> Option<f64> {
    // Expects e.g. "2026-11-22T07:15:53Z" or "2026-11-22T07:15:53.123456Z"
    let b = s.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || (b[10] != b'T' && b[10] != b't') || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    let month: i64 = s.get(5..7)?.parse().ok()?;
    let day: i64 = s.get(8..10)?.parse().ok()?;
    let hour: i64 = s.get(11..13)?.parse().ok()?;
    let min: i64 = s.get(14..16)?.parse().ok()?;
    let sec: i64 = s.get(17..19)?.parse().ok()?;

    let mut frac = 0.0f64;
    let mut tz_offset_sec = 0i64;
    let mut idx = 19;
    if idx < b.len() && b[idx] == b'.' {
        idx += 1;
        let start = idx;
        while idx < b.len() && b[idx].is_ascii_digit() {
            idx += 1;
        }
        if idx > start {
            let digits = &s[start..idx.min(start + 6)];
            if let Ok(v) = digits.parse::<f64>() {
                frac = v / 10f64.powi(digits.len() as i32);
            }
        }
    }
    if idx < b.len() && (b[idx] == b'+' || b[idx] == b'-') && idx + 5 < b.len() {
        let sign = if b[idx] == b'+' { 1 } else { -1 };
        let th: i64 = s.get(idx + 1..idx + 3)?.parse().ok()?;
        let tm: i64 = s.get(idx + 4..idx + 6)?.parse().ok()?;
        tz_offset_sec = sign * (th * 3600 + tm * 60);
    }

    // Howard Hinnant's days_from_civil
    let y = year - if month <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let secs = days * 86400 + hour * 3600 + min * 60 + sec - tz_offset_sec;
    Some(secs as f64 + frac)
}

fn skip_json_string(b: &[u8], mut i: usize) -> Option<(usize, String)> {
    // b[i] must be b'"'
    if i >= b.len() || b[i] != b'"' {
        return None;
    }
    i += 1;
    let mut out = String::new();
    let mut chunk_start = i;
    while i < b.len() {
        match b[i] {
            b'"' => {
                if i > chunk_start {
                    out.push_str(&String::from_utf8_lossy(&b[chunk_start..i]));
                }
                return Some((i + 1, out));
            }
            b'\\' => {
                if i > chunk_start {
                    out.push_str(&String::from_utf8_lossy(&b[chunk_start..i]));
                }
                i += 1;
                if i >= b.len() {
                    return None;
                }
                match b[i] {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    _ => out.push(b[i] as char),
                }
                i += 1;
                chunk_start = i;
            }
            _ => i += 1,
        }
    }
    None
}

fn skip_json_string_fast(b: &[u8], mut i: usize) -> Option<usize> {
    if i >= b.len() || b[i] != b'"' {
        return None;
    }
    i += 1;
    while i < b.len() {
        if b[i] == b'\\' {
            i += 2;
        } else if b[i] == b'"' {
            return Some(i + 1);
        } else {
            i += 1;
        }
    }
    None
}

fn skip_json_value_fast(b: &[u8], mut i: usize) -> Option<usize> {
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= b.len() {
        return None;
    }
    match b[i] {
        b'"' => skip_json_string_fast(b, i),
        b'{' | b'[' => {
            let mut depth = 0i32;
            while i < b.len() {
                match b[i] {
                    b'"' => i = skip_json_string_fast(b, i)?,
                    b'{' | b'[' => {
                        depth += 1;
                        i += 1;
                    }
                    b'}' | b']' => {
                        depth -= 1;
                        i += 1;
                        if depth == 0 {
                            return Some(i);
                        }
                    }
                    _ => i += 1,
                }
            }
            None
        }
        _ => {
            while i < b.len() && !matches!(b[i], b',' | b'}' | b']') && !b[i].is_ascii_whitespace() {
                i += 1;
            }
            Some(i)
        }
    }
}

fn parse_transcript_line(line: &str) -> Option<ParsedStep> {
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= b.len() || b[i] != b'{' {
        return None;
    }
    i += 1;

    let mut step = ParsedStep::default();
    loop {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b',') {
            i += 1;
        }
        if i >= b.len() {
            return None;
        }
        if b[i] == b'}' {
            return Some(step);
        }
        if b[i] != b'"' {
            return None;
        }
        let (next_i, key) = skip_json_string(b, i)?;
        i = next_i;
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() || b[i] != b':' {
            return None;
        }
        i += 1;
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() {
            return None;
        }

        match key.as_str() {
            "step_index" | "input_tokens" | "cache_read_tokens" | "output_tokens" => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'-') {
                    i += 1;
                }
                let num_str = std::str::from_utf8(&b[start..i]).ok()?;
                if key == "step_index" {
                    step.step_index = num_str.parse::<i64>().ok();
                } else if let Ok(v) = num_str.parse::<u64>() {
                    match key.as_str() {
                        "input_tokens" => step.input_tokens = Some(v),
                        "cache_read_tokens" => step.cache_read_tokens = Some(v),
                        "output_tokens" => step.output_tokens = Some(v),
                        _ => {}
                    }
                }
            }
            "created_at" | "model" => {
                if b[i] == b'"' {
                    let (after, val) = skip_json_string(b, i)?;
                    i = after;
                    if key == "created_at" {
                        step.created_at_sec = parse_iso8601_utc(&val);
                    } else {
                        step.model = Some(val);
                    }
                } else {
                    i = skip_json_value_fast(b, i)?;
                }
            }
            _ => {
                i = skip_json_value_fast(b, i)?;
            }
        }
    }
}

struct CachedStepRecord {
    created_at_sec: Option<f64>,
    model: Option<String>,
    input_tokens: Option<u64>,
    cache_read_tokens: Option<u64>,
    output_tokens: Option<u64>,
    observed_finish_sec: Option<f64>,
}

struct TranscriptAnalyzer {
    brain_root: PathBuf,
    active_path: Option<PathBuf>,
    active_session_id: String,
    last_offset: u64,
    last_modified: Option<SystemTime>,
    partial_line: String,
    records: Vec<CachedStepRecord>,
    scan_tick_counter: u32,
    snapshot: MonitorSnapshot,
}

impl TranscriptAnalyzer {
    fn new() -> Self {
        let profile = env::var_os("USERPROFILE").unwrap_or_default();
        let brain_root = PathBuf::from(profile).join(".gemini").join("antigravity").join("brain");
        TranscriptAnalyzer {
            brain_root,
            active_path: None,
            active_session_id: String::new(),
            last_offset: 0,
            last_modified: None,
            partial_line: String::new(),
            records: Vec::new(),
            scan_tick_counter: 0,
            snapshot: MonitorSnapshot::default(),
        }
    }

    fn find_latest_transcript(&self) -> Option<(PathBuf, String, u64, SystemTime)> {
        let dirs = fs::read_dir(&self.brain_root).ok()?;
        let mut best: Option<(PathBuf, String, u64, SystemTime)> = None;
        for entry in dirs.flatten() {
            let candidate = entry.path().join(".system_generated").join("logs").join("transcript.jsonl");
            if let Ok(meta) = fs::metadata(&candidate) {
                if meta.len() == 0 {
                    continue;
                }
                if let Ok(mtime) = meta.modified() {
                    let is_newer = match &best {
                        None => true,
                        Some((_, _, _, best_t)) => mtime > *best_t,
                    };
                    if is_newer {
                        let sid = entry.file_name().to_string_lossy().into_owned();
                        best = Some((candidate, sid, meta.len(), mtime));
                    }
                }
            }
        }
        best
    }

    fn refresh(&mut self, force_scan: bool) -> (bool, &MonitorSnapshot) {
        self.scan_tick_counter = self.scan_tick_counter.wrapping_add(1);
        let should_scan_dirs = force_scan || self.active_path.is_none() || (self.scan_tick_counter % 3 == 0);

        if should_scan_dirs {
            if let Some((path, sid, _len, _mtime)) = self.find_latest_transcript() {
                let changed_file = self.active_path.as_ref() != Some(&path);
                if changed_file {
                    self.active_path = Some(path);
                    self.active_session_id = sid;
                    self.last_offset = 0;
                    self.last_modified = None;
                    self.partial_line.clear();
                    self.records.clear();
                }
            }
        }

        let path = match &self.active_path {
            Some(p) => p.clone(),
            None => return (false, &self.snapshot),
        };

        let meta = match fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => return (false, &self.snapshot),
        };
        let file_len = meta.len();
        let file_mtime = meta.modified().ok();

        if self.last_offset == file_len && self.last_modified == file_mtime && self.snapshot.has_data {
            return (false, &self.snapshot);
        }

        let is_initial = self.last_offset == 0;
        if file_len < self.last_offset {
            // File rotated/truncated
            self.last_offset = 0;
            self.partial_line.clear();
            self.records.clear();
        }

        if file_len > self.last_offset {
            if let Ok(mut f) = File::open(&path) {
                if f.seek(SeekFrom::Start(self.last_offset)).is_ok() {
                    let to_read = (file_len - self.last_offset) as usize;
                    let mut buf = vec![0u8; to_read];
                    if let Ok(n) = f.read(&mut buf) {
                        self.last_offset += n as u64;
                        let text = String::from_utf8_lossy(&buf[..n]);
                        self.partial_line.push_str(&text);

                        let now_sec = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or(Duration::from_secs(0))
                            .as_secs_f64();

                        while let Some(pos) = self.partial_line.find('\n') {
                            let line = self.partial_line[..pos].trim_end_matches('\r').to_string();
                            self.partial_line.drain(..=pos);
                            if line.is_empty() {
                                continue;
                            }
                            if let Some(parsed) = parse_transcript_line(&line) {
                                let is_planner = parsed.input_tokens.is_some() && parsed.model.is_some();
                                let obs = if !is_initial && is_planner {
                                    Some(now_sec)
                                } else {
                                    None
                                };
                                self.records.push(CachedStepRecord {
                                    created_at_sec: parsed.created_at_sec,
                                    model: parsed.model,
                                    input_tokens: parsed.input_tokens,
                                    cache_read_tokens: parsed.cache_read_tokens,
                                    output_tokens: parsed.output_tokens,
                                    observed_finish_sec: obs,
                                });
                            }
                        }
                    }
                }
            }
        }

        self.last_modified = file_mtime;
        let file_mtime_sec = file_mtime
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs_f64());

        let new_snap = self.compute_snapshot(file_mtime_sec);
        let changed = new_snap != self.snapshot;
        self.snapshot = new_snap;
        (changed, &self.snapshot)
    }

    fn compute_snapshot(&self, file_mtime_sec: Option<f64>) -> MonitorSnapshot {
        let mut snap = MonitorSnapshot::default();
        snap.session_id = self.active_session_id.clone();

        let mut sum_in = 0u64;
        let mut sum_cache = 0u64;
        let mut planner_count = 0u32;
        let mut weighted_out = 0.0f64;
        let mut weighted_dur = 0.0f64;

        let mut latest_idx: Option<usize> = None;
        let mut latest_dur = 0.0f64;

        for i in 0..self.records.len() {
            let rec = &self.records[i];
            if rec.input_tokens.is_none() || rec.model.is_none() {
                continue;
            }
            let in_tok = rec.input_tokens.unwrap_or(0);
            let cache_tok = rec.cache_read_tokens.unwrap_or(0);
            let out_tok = rec.output_tokens.unwrap_or(0);

            planner_count += 1;
            sum_in += in_tok;
            sum_cache += cache_tok;

            let mut dur = 0.0f64;
            if let (Some(obs), Some(created)) = (rec.observed_finish_sec, rec.created_at_sec) {
                let d = obs - created;
                if (0.5..=300.0).contains(&d) {
                    dur = d;
                }
            } else if let Some(created) = rec.created_at_sec {
                let next_t = if i + 1 < self.records.len() {
                    self.records[i + 1].created_at_sec
                } else {
                    file_mtime_sec
                };
                if let Some(nt) = next_t {
                    let d = nt - created;
                    if (1.0..=300.0).contains(&d) {
                        dur = d;
                    }
                }
            }

            if dur > 0.0 && out_tok > 0 {
                weighted_out += out_tok as f64;
                weighted_dur += dur;
            }

            latest_idx = Some(i);
            latest_dur = dur;
        }

        let idx = match latest_idx {
            Some(i) => i,
            None => return snap,
        };
        let latest = &self.records[idx];
        let l_in = latest.input_tokens.unwrap_or(0);
        let l_cache = latest.cache_read_tokens.unwrap_or(0);
        let l_out = latest.output_tokens.unwrap_or(0);

        snap.has_data = true;
        snap.model = latest.model.clone().unwrap_or_else(|| "unknown".to_string());
        snap.context_used = l_in + l_cache + l_out;
        snap.latest_output_tokens = l_out;
        snap.latest_duration_sec = latest_dur;
        snap.latest_tps = if latest_dur > 0.0 && l_out > 0 {
            l_out as f64 / latest_dur
        } else {
            0.0
        };
        snap.avg_tps = if weighted_dur > 0.0 {
            weighted_out / weighted_dur
        } else {
            0.0
        };

        let latest_tot = l_in + l_cache;
        snap.latest_hit_rate = if latest_tot > 0 {
            l_cache as f64 * 100.0 / latest_tot as f64
        } else {
            0.0
        };
        let sess_tot = sum_in + sum_cache;
        snap.session_hit_rate = if sess_tot > 0 {
            sum_cache as f64 * 100.0 / sess_tot as f64
        } else {
            0.0
        };

        snap.total_steps = planner_count;
        snap.total_cache_read = sum_cache;
        snap.total_uncached_in = sum_in;

        unsafe {
            let mut st: SYSTEMTIME = zeroed();
            GetLocalTime(&mut st);
            snap.last_updated_hms = format!("{:02}:{:02}:{:02}", st.w_hour, st.w_minute, st.w_second);
        }
        snap
    }
}

// ========================= GDI+ Drawing Helpers =========================

fn argb(a: u8, r: u8, g: u8, b: u8) -> u32 {
    ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

fn add_rounded_rect_path(path: *mut c_void, x: f32, y: f32, w: f32, h: f32, radius: f32) {
    let d = (radius * 2.0).min(w).min(h);
    if d <= 0.5 {
        return;
    }
    unsafe {
        GdipAddPathArc(path, x, y, d, d, 180.0, 90.0);
        GdipAddPathArc(path, x + w - d, y, d, d, 270.0, 90.0);
        GdipAddPathArc(path, x + w - d, y + h - d, d, d, 0.0, 90.0);
        GdipAddPathArc(path, x, y + h - d, d, d, 90.0, 90.0);
        GdipClosePathFigure(path);
    }
}

fn fill_round_rect(g: *mut c_void, x: f32, y: f32, w: f32, h: f32, radius: f32, color: u32) {
    unsafe {
        let mut path: *mut c_void = null_mut();
        let mut brush: *mut c_void = null_mut();
        GdipCreatePath(0, &mut path);
        add_rounded_rect_path(path, x, y, w, h, radius);
        GdipCreateSolidFill(color, &mut brush);
        GdipFillPath(g, brush, path);
        GdipDeleteBrush(brush);
        GdipDeletePath(path);
    }
}

fn stroke_round_rect(g: *mut c_void, x: f32, y: f32, w: f32, h: f32, radius: f32, color: u32, width: f32) {
    unsafe {
        let mut path: *mut c_void = null_mut();
        let mut pen: *mut c_void = null_mut();
        GdipCreatePath(0, &mut path);
        add_rounded_rect_path(path, x, y, w, h, radius);
        GdipCreatePen1(color, width, 2 /* UnitPixel */, &mut pen);
        GdipDrawPath(g, pen, path);
        GdipDeletePen(pen);
        GdipDeletePath(path);
    }
}

fn draw_text(
    g: *mut c_void,
    text: &str,
    font: *mut c_void,
    color: u32,
    rect: RectF,
    h_align: i32,
    v_align: i32,
    ellipsis: bool,
) {
    unsafe {
        let wide = to_wide(text);
        let mut brush: *mut c_void = null_mut();
        let mut fmt: *mut c_void = null_mut();
        GdipCreateSolidFill(color, &mut brush);
        GdipCreateStringFormat(0, 0, &mut fmt);
        GdipSetStringFormatAlign(fmt, h_align);
        GdipSetStringFormatLineAlign(fmt, v_align);
        // StringFormatFlagsNoWrap = 0x1000
        GdipSetStringFormatFlags(fmt, 0x1000);
        if ellipsis {
            // StringTrimmingEllipsisCharacter = 3
            GdipSetStringFormatTrimming(fmt, 3);
        }
        GdipDrawString(g, wide.as_ptr(), -1, font, &rect, fmt, brush);
        GdipDeleteStringFormat(fmt);
        GdipDeleteBrush(brush);
    }
}

fn format_compact(v: u64) -> String {
    if v >= 1_000_000 {
        format!("{:.2}M", v as f64 / 1_000_000.0)
    } else if v >= 1_000 {
        format!("{:.1}K", v as f64 / 1_000.0)
    } else {
        format!("{}", v)
    }
}

fn format_cap_label(cap: u64) -> String {
    match cap {
        200_000 => "200K".to_string(),
        256_000 => "256K".to_string(),
        512_000 => "512K".to_string(),
        1_000_000 => "1M".to_string(),
        _ => format_compact(cap),
    }
}

fn color_by_context(pct: f64) -> u32 {
    if pct >= 85.0 {
        argb(255, 239, 68, 68) // #EF4444
    } else if pct >= 70.0 {
        argb(255, 245, 158, 11) // #F59E0B
    } else {
        argb(255, 34, 197, 94) // #22C55E
    }
}

fn color_by_hit(hit: f64) -> u32 {
    if hit >= 80.0 {
        argb(255, 34, 197, 94)
    } else if hit >= 50.0 {
        argb(255, 245, 158, 11)
    } else {
        argb(255, 239, 68, 68)
    }
}

// ========================= Application State & Window =========================

const CAPSULE_W_DIP: i32 = 242;
const CAPSULE_H_DIP: i32 = 24;
const POPOVER_W_DIP: i32 = 268;
const POPOVER_H_DIP: i32 = 226;

const TIMER_DATA: usize = 1;
const TIMER_DISCOVER: usize = 2;

const CMD_TOGGLE_EXPAND: usize = 1001;
const CMD_TOGGLE_TRANSPARENT: usize = 1002;
const CMD_CYCLE_CAP: usize = 1003;
const CMD_PRESET_TOPBAR: usize = 1004;
const CMD_PRESET_TOPRIGHT: usize = 1005;
const CMD_PRESET_BOTRIGHT: usize = 1006;
const CMD_TOGGLE_PIN: usize = 1007;
const CMD_OPEN_LOGS: usize = 1008;
const CMD_EXIT: usize = 1009;

struct AppState {
    my_hwnd: *mut c_void,
    tracked_ag_hwnd: *mut c_void,
    ag_visible: bool,
    last_x: i32,
    last_y: i32,
    last_w: i32,
    last_h: i32,
    last_topmost: Option<bool>,
    scale: f32,

    expanded: bool,
    transparent_mode: bool,
    always_topmost: bool,
    cap_preset_idx: usize,
    dock_cfg: DockConfig,

    dragging: bool,
    drag_moved: bool,
    drag_start_cursor: POINT,
    drag_start_right_dip: i32,
    drag_start_top_dip: i32,

    pid_cache: HashMap<u32, bool>,
    analyzer: TranscriptAnalyzer,
    tray_icon: *mut c_void,
    font_family_ui: *mut c_void,
    font_family_mono: *mut c_void,
}

static mut G_STATE: *mut AppState = null_mut();
static IN_SYNC: AtomicBool = AtomicBool::new(false);

const CAP_PRESETS: [u64; 4] = [256_000, 512_000, 1_000_000, 200_000];

impl AppState {
    fn current_cap(&self) -> u64 {
        CAP_PRESETS[self.cap_preset_idx % CAP_PRESETS.len()]
    }

    fn target_dip_size(&self) -> (i32, i32) {
        if self.expanded {
            (POPOVER_W_DIP, POPOVER_H_DIP)
        } else {
            (CAPSULE_W_DIP, CAPSULE_H_DIP)
        }
    }

    fn is_antigravity_pid(&mut self, pid: u32) -> bool {
        if let Some(&cached) = self.pid_cache.get(&pid) {
            return cached;
        }
        if self.pid_cache.len() > 64 {
            self.pid_cache.clear();
        }
        let mut is_ag = false;
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if !h.is_null() {
                let mut buf = [0u16; 512];
                let mut len = buf.len() as u32;
                if QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len) != 0 {
                    let path = String::from_utf16_lossy(&buf[..len as usize]);
                    let lower = path.to_ascii_lowercase();
                    is_ag = lower.ends_with("\\antigravity.exe");
                }
                CloseHandle(h);
            }
        }
        self.pid_cache.insert(pid, is_ag);
        is_ag
    }

    fn find_antigravity_window(&mut self) -> (*mut c_void, RECT, bool) {
        struct EnumCtx {
            state: *mut AppState,
            fg_hwnd: *mut c_void,
            best_hwnd: *mut c_void,
            best_rect: RECT,
            best_score: i64,
            saw_minimized: bool,
        }

        extern "system" fn enum_cb(hwnd: *mut c_void, lparam: isize) -> i32 {
            unsafe {
                let ctx = &mut *(lparam as *mut EnumCtx);
                if IsWindowVisible(hwnd) == 0 {
                    return 1;
                }
                let mut pid = 0u32;
                GetWindowThreadProcessId(hwnd, &mut pid);
                if !(*ctx.state).is_antigravity_pid(pid) {
                    return 1;
                }
                if IsIconic(hwnd) != 0 {
                    ctx.saw_minimized = true;
                    return 1;
                }
                let r = get_visual_rect(hwnd);
                if r.width() < 300 || r.height() < 200 {
                    return 1;
                }
                let area = (r.width() as i64) * (r.height() as i64);
                let mut bonus = 0i64;
                if hwnd == ctx.fg_hwnd {
                    bonus += 200_000_000;
                } else if hwnd == (*ctx.state).tracked_ag_hwnd {
                    bonus += 100_000_000;
                }
                let score = bonus + area.min(50_000_000);
                if score > ctx.best_score {
                    ctx.best_score = score;
                    ctx.best_hwnd = hwnd;
                    ctx.best_rect = r;
                }
                1
            }
        }

        unsafe {
            let mut ctx = EnumCtx {
                state: self as *mut AppState,
                fg_hwnd: GetForegroundWindow(),
                best_hwnd: null_mut(),
                best_rect: RECT::default(),
                best_score: -1,
                saw_minimized: false,
            };
            EnumWindows(enum_cb, (&mut ctx as *mut EnumCtx) as isize);
            (ctx.best_hwnd, ctx.best_rect, ctx.saw_minimized)
        }
    }

    fn sync_dock_and_zorder(&mut self, force_repaint: bool) {
        if IN_SYNC.swap(true, Ordering::SeqCst) {
            return;
        }
        unsafe {
            let (ag_hwnd, ag_rect, _minimized) = self.find_antigravity_window();
            if ag_hwnd.is_null() {
                self.tracked_ag_hwnd = null_mut();
                if self.ag_visible {
                    self.ag_visible = false;
                    ShowWindow(self.my_hwnd, SW_HIDE);
                    trim_working_set();
                }
                IN_SYNC.store(false, Ordering::SeqCst);
                return;
            }

            self.tracked_ag_hwnd = ag_hwnd;
            let dpi = GetDpiForWindow(ag_hwnd);
            let new_scale = if dpi >= 96 { dpi as f32 / 96.0 } else { 1.0 };
            let scale_changed = (new_scale - self.scale).abs() > 0.01;
            self.scale = new_scale;

            let (dip_w, dip_h) = self.target_dip_size();
            let win_w = (dip_w as f32 * self.scale).round() as i32;
            let win_h = (dip_h as f32 * self.scale).round() as i32;

            let mut mi: MONITORINFO = zeroed();
            mi.cb_size = size_of::<MONITORINFO>() as u32;
            let hmon = MonitorFromRect(&ag_rect, MONITOR_DEFAULTTONEAREST);
            let work_area = if !hmon.is_null() && GetMonitorInfoW(hmon, &mut mi) != 0 {
                mi.rc_work
            } else {
                ag_rect
            };

            let pos = compute_inside_position(ag_rect, work_area, win_w, win_h, self.dock_cfg, self.scale);

            if force_repaint || scale_changed || win_w != self.last_w || win_h != self.last_h || !self.ag_visible {
                self.render_layered(win_w, win_h);
            }

            let fg = GetForegroundWindow();
            let want_topmost = self.always_topmost || fg == ag_hwnd || fg == self.my_hwnd;
            let pos_changed = pos.x != self.last_x || pos.y != self.last_y || win_w != self.last_w || win_h != self.last_h;
            let z_changed = self.last_topmost != Some(want_topmost) || !self.ag_visible;

            if !self.ag_visible {
                ShowWindow(self.my_hwnd, SW_SHOWNOACTIVATE);
                self.ag_visible = true;
            }

            if pos_changed || z_changed {
                if want_topmost {
                    SetWindowPos(
                        self.my_hwnd,
                        HWND_TOPMOST,
                        pos.x,
                        pos.y,
                        win_w,
                        win_h,
                        SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                } else {
                    if self.last_topmost == Some(true) {
                        SetWindowPos(
                            self.my_hwnd,
                            HWND_NOTOPMOST,
                            pos.x,
                            pos.y,
                            win_w,
                            win_h,
                            SWP_NOACTIVATE | SWP_SHOWWINDOW,
                        );
                    }
                    let prev = GetWindow(ag_hwnd, GW_HWNDPREV);
                    let insert_after = if !prev.is_null() && prev != self.my_hwnd {
                        prev
                    } else {
                        HWND_TOP
                    };
                    SetWindowPos(
                        self.my_hwnd,
                        insert_after,
                        pos.x,
                        pos.y,
                        win_w,
                        win_h,
                        SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                }
                self.last_x = pos.x;
                self.last_y = pos.y;
                self.last_w = win_w;
                self.last_h = win_h;
                self.last_topmost = Some(want_topmost);
            }
        }
        IN_SYNC.store(false, Ordering::SeqCst);
    }

    fn render_layered(&mut self, w: i32, h: i32) {
        if w <= 0 || h <= 0 || self.my_hwnd.is_null() {
            return;
        }
        unsafe {
            let screen_dc = GetDC(null_mut());
            let mem_dc = CreateCompatibleDC(screen_dc);

            let bmi = BITMAPINFOHEADER {
                bi_size: size_of::<BITMAPINFOHEADER>() as u32,
                bi_width: w,
                bi_height: -h, // top-down DIB
                bi_planes: 1,
                bi_bit_count: 32,
                bi_compression: 0, // BI_RGB
                bi_size_image: 0,
                bi_xpels_per_meter: 0,
                bi_ypels_per_meter: 0,
                bi_clr_used: 0,
                bi_clr_important: 0,
            };

            let mut bits_ptr: *mut c_void = null_mut();
            let hbmp = CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits_ptr, null_mut(), 0);
            if hbmp.is_null() || bits_ptr.is_null() {
                DeleteDC(mem_dc);
                ReleaseDC(null_mut(), screen_dc);
                return;
            }
            let old_bmp = SelectObject(mem_dc, hbmp);
            std::ptr::write_bytes(bits_ptr as *mut u8, 0, (w * h * 4) as usize);

            let mut gp_bmp: *mut c_void = null_mut();
            let mut g: *mut c_void = null_mut();
            // PixelFormat32bppPARGB = 0x000E200B
            GdipCreateBitmapFromScan0(w, h, w * 4, 0x000E_200B, bits_ptr as *mut u8, &mut gp_bmp);
            GdipGetImageGraphicsContext(gp_bmp, &mut g);
            GdipSetSmoothingMode(g, 4); // SmoothingModeAntiAlias
            GdipSetTextRenderingHint(g, 4); // TextRenderingHintAntiAlias (safe for alpha!)

            if self.expanded {
                self.paint_expanded(g, w as f32, h as f32);
            } else {
                self.paint_capsule(g, w as f32, h as f32);
            }

            let pt_src = POINT { x: 0, y: 0 };
            let mut pt_dst = POINT {
                x: if self.last_x != i32::MIN { self.last_x } else { 0 },
                y: if self.last_y != i32::MIN { self.last_y } else { 0 },
            };
            if self.last_x == i32::MIN {
                let mut r: RECT = zeroed();
                GetWindowRect(self.my_hwnd, &mut r);
                pt_dst.x = r.left;
                pt_dst.y = r.top;
            }
            let sz = SIZE { cx: w, cy: h };
            let blend = BLENDFUNCTION {
                blend_op: AC_SRC_OVER,
                blend_flags: 0,
                source_constant_alpha: 255,
                alpha_format: AC_SRC_ALPHA,
            };

            UpdateLayeredWindow(self.my_hwnd, screen_dc, &pt_dst, &sz, mem_dc, &pt_src, 0, &blend, ULW_ALPHA);

            GdipDeleteGraphics(g);
            GdipDisposeImage(gp_bmp);
            SelectObject(mem_dc, old_bmp);
            DeleteObject(hbmp);
            DeleteDC(mem_dc);
            ReleaseDC(null_mut(), screen_dc);
        }
    }

    fn make_font(&self, family: *mut c_void, dip_size: f32, bold: bool) -> *mut c_void {
        unsafe {
            let mut f: *mut c_void = null_mut();
            let style = if bold { 1 } else { 0 };
            // UnitPixel = 2
            GdipCreateFont(family, dip_size * self.scale, style, 2, &mut f);
            f
        }
    }

    fn paint_capsule(&self, g: *mut c_void, w: f32, h: f32) {
        let s = self.scale;
        let snap = &self.analyzer.snapshot;
        let cap = self.current_cap();

        let (bg_col, border_col) = if self.transparent_mode {
            // 5% alpha keeps full pill clickable & draggable without visible box
            (argb(13, 17, 16, 21), argb(0, 0, 0, 0))
        } else {
            (argb(242, 22, 21, 27), argb(220, 44, 43, 54)) // #16151B + #2C2B36
        };

        fill_round_rect(g, 0.5 * s, 0.5 * s, w - s, h - s, 11.0 * s, bg_col);
        if border_col != 0 {
            stroke_round_rect(g, 0.5 * s, 0.5 * s, w - s, h - s, 11.0 * s, border_col, 1.0 * s);
        }

        let ctx_pct = if snap.has_data && cap > 0 {
            (snap.context_used as f64 * 100.0 / cap as f64).min(100.0)
        } else {
            0.0
        };
        let ctx_col = if snap.has_data { color_by_context(ctx_pct) } else { argb(255, 148, 163, 184) };
        let hit_col = if snap.has_data { color_by_hit(snap.session_hit_rate) } else { argb(255, 148, 163, 184) };

        // Circular context ring on the left
        let ring_d = 12.0 * s;
        let ring_x = 9.0 * s;
        let ring_y = (h - ring_d) * 0.5;
        unsafe {
            let mut pen_track: *mut c_void = null_mut();
            let mut pen_arc: *mut c_void = null_mut();
            GdipCreatePen1(argb(255, 48, 47, 60), 2.0 * s, 2, &mut pen_track);
            GdipDrawArc(g, pen_track, ring_x, ring_y, ring_d, ring_d, 0.0, 360.0);
            GdipDeletePen(pen_track);

            if snap.has_data && ctx_pct > 0.5 {
                GdipCreatePen1(ctx_col, 2.0 * s, 2, &mut pen_arc);
                GdipDrawArc(g, pen_arc, ring_x, ring_y, ring_d, ring_d, -90.0, (ctx_pct as f32 / 100.0) * 360.0);
                GdipDeletePen(pen_arc);
            }
        }

        let f_val = self.make_font(self.font_family_mono, 11.2, true);
        let f_dim = self.make_font(self.font_family_ui, 10.5, false);

        let ctx_str = if snap.has_data {
            format!("{:.0}%", ctx_pct)
        } else {
            "--%".to_string()
        };
        let tps_val = if snap.latest_tps > 0.0 { snap.latest_tps } else { snap.avg_tps };
        let tps_str = if tps_val > 0.0 {
            format!("{:.0} t/s", tps_val)
        } else {
            "-- t/s".to_string()
        };
        let hit_str = if snap.has_data {
            format!("HIT {:.0}%", snap.session_hit_rate)
        } else {
            "HIT --".to_string()
        };

        draw_text(
            g,
            &ctx_str,
            f_val,
            ctx_col,
            RectF { x: 25.0 * s, y: 0.0, width: 38.0 * s, height: h },
            0,
            1,
            false,
        );

        unsafe {
            let mut pen_div: *mut c_void = null_mut();
            GdipCreatePen1(argb(180, 56, 55, 70), 1.0 * s, 2, &mut pen_div);
            GdipDrawLine(g, pen_div, 67.0 * s, 6.0 * s, 67.0 * s, h - 6.0 * s);
            GdipDrawLine(g, pen_div, 137.0 * s, 6.0 * s, 137.0 * s, h - 6.0 * s);
            GdipDeletePen(pen_div);
        }

        draw_text(
            g,
            &tps_str,
            f_val,
            argb(255, 56, 189, 248), // #38BDF8
            RectF { x: 71.0 * s, y: 0.0, width: 64.0 * s, height: h },
            1,
            1,
            false,
        );

        draw_text(
            g,
            &hit_str,
            f_val,
            hit_col,
            RectF { x: 141.0 * s, y: 0.0, width: 78.0 * s, height: h },
            1,
            1,
            false,
        );

        draw_text(
            g,
            "▾",
            f_dim,
            argb(255, 142, 141, 152),
            RectF { x: w - 20.0 * s, y: 0.0, width: 14.0 * s, height: h },
            1,
            1,
            false,
        );

        unsafe {
            GdipDeleteFont(f_val);
            GdipDeleteFont(f_dim);
        }
    }

    fn paint_expanded(&self, g: *mut c_void, w: f32, h: f32) {
        let s = self.scale;
        let snap = &self.analyzer.snapshot;
        let cap = self.current_cap();

        // Outer popover card matching Antigravity #111015
        fill_round_rect(g, 0.5 * s, 0.5 * s, w - s, h - s, 10.0 * s, argb(246, 17, 16, 21));
        stroke_round_rect(g, 0.5 * s, 0.5 * s, w - s, h - s, 10.0 * s, argb(255, 42, 41, 51), 1.0 * s);

        let f_title = self.make_font(self.font_family_ui, 11.5, true);
        let f_btn = self.make_font(self.font_family_ui, 9.8, false);
        let f_label = self.make_font(self.font_family_ui, 10.5, false);
        let f_mono = self.make_font(self.font_family_mono, 11.0, false);
        let f_big = self.make_font(self.font_family_mono, 14.0, true);
        let f_sub = self.make_font(self.font_family_ui, 9.8, false);

        // Header bar + 4 tiny hit-tested buttons
        draw_text(
            g,
            "AGY 监控 · 只读",
            f_title,
            argb(255, 228, 228, 231),
            RectF { x: 11.0 * s, y: 7.0 * s, width: 110.0 * s, height: 20.0 * s },
            0,
            1,
            false,
        );

        let trans_lbl = if self.transparent_mode { "同色" } else { "透明" };
        let cap_lbl = format_cap_label(cap);
        let buttons = [
            (w - 134.0 * s, 36.0 * s, trans_lbl),
            (w - 94.0 * s, 40.0 * s, cap_lbl.as_str()),
            (w - 50.0 * s, 20.0 * s, "复"),
            (w - 26.0 * s, 18.0 * s, "×"),
        ];
        for (bx, bw, lbl) in buttons.iter() {
            fill_round_rect(g, *bx, 8.0 * s, *bw, 18.0 * s, 4.0 * s, argb(255, 28, 27, 35));
            stroke_round_rect(g, *bx, 8.0 * s, *bw, 18.0 * s, 4.0 * s, argb(255, 48, 47, 60), 1.0 * s);
            draw_text(
                g,
                lbl,
                f_btn,
                argb(255, 203, 213, 225),
                RectF { x: *bx, y: 8.0 * s, width: *bw, height: 18.0 * s },
                1,
                1,
                false,
            );
        }

        // Model pill
        let pad_x = 10.0 * s;
        let card_w = w - 20.0 * s;
        fill_round_rect(g, pad_x, 32.0 * s, card_w, 24.0 * s, 6.0 * s, argb(255, 22, 21, 27));
        stroke_round_rect(g, pad_x, 32.0 * s, card_w, 24.0 * s, 6.0 * s, argb(255, 38, 37, 46), 1.0 * s);
        let model_str = if snap.has_data { snap.model.as_str() } else { "等待本地对话日志..." };
        draw_text(
            g,
            model_str,
            f_mono,
            argb(255, 147, 197, 253), // #93C5FD
            RectF { x: pad_x + 7.0 * s, y: 32.0 * s, width: card_w - 14.0 * s, height: 24.0 * s },
            0,
            1,
            true,
        );

        // Card 1: Context Usage (y = 62, h = 48)
        fill_round_rect(g, pad_x, 62.0 * s, card_w, 48.0 * s, 7.0 * s, argb(255, 22, 21, 27));
        stroke_round_rect(g, pad_x, 62.0 * s, card_w, 48.0 * s, 7.0 * s, argb(255, 38, 37, 46), 1.0 * s);
        let ctx_pct = if snap.has_data && cap > 0 {
            (snap.context_used as f64 * 100.0 / cap as f64).min(100.0)
        } else {
            0.0
        };
        let ctx_col = color_by_context(ctx_pct);
        draw_text(
            g,
            "上下文用量",
            f_label,
            argb(255, 148, 163, 184),
            RectF { x: pad_x + 8.0 * s, y: 66.0 * s, width: 80.0 * s, height: 16.0 * s },
            0,
            1,
            false,
        );
        let ctx_val = if snap.has_data {
            format!("{} / {} ({:.1}%)", format_compact(snap.context_used), format_cap_label(cap), ctx_pct)
        } else {
            format!("-- / {}", format_cap_label(cap))
        };
        draw_text(
            g,
            &ctx_val,
            f_mono,
            ctx_col,
            RectF { x: pad_x + 80.0 * s, y: 66.0 * s, width: card_w - 88.0 * s, height: 16.0 * s },
            2,
            1,
            false,
        );
        // Progress bar
        let bar_x = pad_x + 8.0 * s;
        let bar_y = 92.0 * s;
        let bar_w = card_w - 16.0 * s;
        fill_round_rect(g, bar_x, bar_y, bar_w, 6.0 * s, 3.0 * s, argb(255, 35, 34, 44));
        if snap.has_data && ctx_pct > 0.0 {
            let fill_w = ((ctx_pct as f32 / 100.0) * bar_w).max(4.0 * s).min(bar_w);
            fill_round_rect(g, bar_x, bar_y, fill_w, 6.0 * s, 3.0 * s, ctx_col);
        }

        // Card 2: TPS (y = 116, h = 44)
        fill_round_rect(g, pad_x, 116.0 * s, card_w, 44.0 * s, 7.0 * s, argb(255, 22, 21, 27));
        stroke_round_rect(g, pad_x, 116.0 * s, card_w, 44.0 * s, 7.0 * s, argb(255, 38, 37, 46), 1.0 * s);
        draw_text(
            g,
            "生成速度",
            f_label,
            argb(255, 148, 163, 184),
            RectF { x: pad_x + 8.0 * s, y: 120.0 * s, width: 70.0 * s, height: 18.0 * s },
            0,
            1,
            false,
        );
        let tps_main = if snap.latest_tps > 0.0 {
            format!("{:.1} tok/s", snap.latest_tps)
        } else if snap.avg_tps > 0.0 {
            format!("{:.1} tok/s", snap.avg_tps)
        } else {
            "-- tok/s".to_string()
        };
        draw_text(
            g,
            &tps_main,
            f_big,
            argb(255, 56, 189, 248),
            RectF { x: pad_x + 74.0 * s, y: 118.0 * s, width: card_w - 82.0 * s, height: 22.0 * s },
            2,
            1,
            false,
        );
        let tps_sub = if snap.latest_tps > 0.0 {
            format!("本轮 {} tok · {:.1}s  |  均速 {:.1}", snap.latest_output_tokens, snap.latest_duration_sec, snap.avg_tps)
        } else if snap.has_data {
            format!("会话均速 {:.1}  |  本轮 {} tok", snap.avg_tps, snap.latest_output_tokens)
        } else {
            "等待新回复计算耗时...".to_string()
        };
        draw_text(
            g,
            &tps_sub,
            f_sub,
            argb(255, 148, 163, 184),
            RectF { x: pad_x + 8.0 * s, y: 141.0 * s, width: card_w - 16.0 * s, height: 15.0 * s },
            0,
            1,
            true,
        );

        // Card 3: Cache Hit Rate (y = 166, h = 38)
        fill_round_rect(g, pad_x, 166.0 * s, card_w, 38.0 * s, 7.0 * s, argb(255, 22, 21, 27));
        stroke_round_rect(g, pad_x, 166.0 * s, card_w, 38.0 * s, 7.0 * s, argb(255, 38, 37, 46), 1.0 * s);
        let hit_col = if snap.has_data { color_by_hit(snap.session_hit_rate) } else { argb(255, 148, 163, 184) };
        draw_text(
            g,
            "缓存命中",
            f_label,
            argb(255, 148, 163, 184),
            RectF { x: pad_x + 8.0 * s, y: 169.0 * s, width: 64.0 * s, height: 16.0 * s },
            0,
            1,
            false,
        );
        let hit_main = if snap.has_data {
            format!("累计 {:.1}% · 本轮 {:.1}%", snap.session_hit_rate, snap.latest_hit_rate)
        } else {
            "--".to_string()
        };
        draw_text(
            g,
            &hit_main,
            f_mono,
            hit_col,
            RectF { x: pad_x + 68.0 * s, y: 169.0 * s, width: card_w - 76.0 * s, height: 16.0 * s },
            2,
            1,
            false,
        );
        let hit_sub = format!(
            "命中 {}  ·  未命中 {}",
            format_compact(snap.total_cache_read),
            format_compact(snap.total_uncached_in)
        );
        draw_text(
            g,
            &hit_sub,
            f_sub,
            argb(255, 148, 163, 184),
            RectF { x: pad_x + 8.0 * s, y: 186.0 * s, width: card_w - 16.0 * s, height: 14.0 * s },
            0,
            1,
            true,
        );

        // Footer (y = 207)
        let sid_short = if snap.session_id.len() >= 8 {
            &snap.session_id[..8]
        } else {
            snap.session_id.as_str()
        };
        let footer = if snap.has_data {
            format!("{} · {}步 · {} · 右键菜单", sid_short, snap.total_steps, snap.last_updated_hms)
        } else {
            "纯只读 · 零注入 · 右键菜单".to_string()
        };
        draw_text(
            g,
            &footer,
            f_sub,
            argb(255, 100, 116, 139),
            RectF { x: pad_x, y: 207.0 * s, width: card_w, height: 15.0 * s },
            0,
            1,
            true,
        );

        unsafe {
            GdipDeleteFont(f_title);
            GdipDeleteFont(f_btn);
            GdipDeleteFont(f_label);
            GdipDeleteFont(f_mono);
            GdipDeleteFont(f_big);
            GdipDeleteFont(f_sub);
        }
    }

    fn handle_expanded_click(&mut self, local_x_px: i32, local_y_px: i32) {
        let x = local_x_px as f32 / self.scale;
        let y = local_y_px as f32 / self.scale;
        let w = POPOVER_W_DIP as f32;

        if (6.0..=28.0).contains(&y) {
            if x >= w - 27.0 && x <= w - 7.0 {
                unsafe { DestroyWindow(self.my_hwnd); }
                return;
            }
            if x >= w - 51.0 && x <= w - 29.0 {
                self.dock_cfg = DockConfig::default();
                self.sync_dock_and_zorder(true);
                return;
            }
            if x >= w - 95.0 && x <= w - 53.0 {
                self.cap_preset_idx = (self.cap_preset_idx + 1) % CAP_PRESETS.len();
                self.sync_dock_and_zorder(true);
                return;
            }
            if x >= w - 135.0 && x <= w - 97.0 {
                self.transparent_mode = !self.transparent_mode;
                self.sync_dock_and_zorder(true);
                return;
            }
            // Clicking title text collapses back to capsule
            self.expanded = false;
            self.sync_dock_and_zorder(true);
        } else if (62.0..=110.0).contains(&y) {
            // Clicking context card cycles reference cap
            self.cap_preset_idx = (self.cap_preset_idx + 1) % CAP_PRESETS.len();
            self.sync_dock_and_zorder(true);
        }
    }

    fn show_context_menu(&mut self) {
        unsafe {
            let hmenu = CreatePopupMenu();
            if hmenu.is_null() {
                return;
            }
            let expand_lbl = if self.expanded { "收起为顶栏胶囊" } else { "展开详细面板" };
            let trans_lbl = "纯透明背景（无胶囊底色）";
            let pin_lbl = "始终置顶（切走其他软件时也保留）";

            AppendMenuW(hmenu, MF_STRING, CMD_TOGGLE_EXPAND, to_wide(expand_lbl).as_ptr());
            AppendMenuW(
                hmenu,
                MF_STRING | if self.transparent_mode { MF_CHECKED } else { 0 },
                CMD_TOGGLE_TRANSPARENT,
                to_wide(trans_lbl).as_ptr(),
            );
            let cap_lbl = format!("切换上下文参考上限（当前 {}）", format_cap_label(self.current_cap()));
            AppendMenuW(hmenu, MF_STRING, CMD_CYCLE_CAP, to_wide(&cap_lbl).as_ptr());
            AppendMenuW(hmenu, MF_SEPARATOR, 0, null());
            AppendMenuW(hmenu, MF_STRING, CMD_PRESET_TOPBAR, to_wide("吸附位置：顶栏胶囊（默认）").as_ptr());
            AppendMenuW(hmenu, MF_STRING, CMD_PRESET_TOPRIGHT, to_wide("吸附位置：右上内侧").as_ptr());
            AppendMenuW(hmenu, MF_STRING, CMD_PRESET_BOTRIGHT, to_wide("吸附位置：右下内侧").as_ptr());
            AppendMenuW(
                hmenu,
                MF_STRING | if self.always_topmost { MF_CHECKED } else { 0 },
                CMD_TOGGLE_PIN,
                to_wide(pin_lbl).as_ptr(),
            );
            AppendMenuW(hmenu, MF_SEPARATOR, 0, null());
            AppendMenuW(hmenu, MF_STRING, CMD_OPEN_LOGS, to_wide("打开本地日志目录").as_ptr());
            AppendMenuW(hmenu, MF_STRING, CMD_EXIT, to_wide("退出监控").as_ptr());

            let mut pt = POINT::default();
            GetCursorPos(&mut pt);
            SetForegroundWindow(self.my_hwnd);
            let cmd = TrackPopupMenu(hmenu, TPM_RIGHTBUTTON | TPM_RETURNCMD, pt.x, pt.y, 0, self.my_hwnd, null()) as usize;
            PostMessageW(self.my_hwnd, WM_NULL, 0, 0);
            DestroyMenu(hmenu);

            match cmd {
                CMD_TOGGLE_EXPAND => {
                    self.expanded = !self.expanded;
                    self.sync_dock_and_zorder(true);
                }
                CMD_TOGGLE_TRANSPARENT => {
                    self.transparent_mode = !self.transparent_mode;
                    self.sync_dock_and_zorder(true);
                }
                CMD_CYCLE_CAP => {
                    self.cap_preset_idx = (self.cap_preset_idx + 1) % CAP_PRESETS.len();
                    self.sync_dock_and_zorder(true);
                }
                CMD_PRESET_TOPBAR => {
                    self.dock_cfg = DockConfig::default();
                    self.sync_dock_and_zorder(true);
                }
                CMD_PRESET_TOPRIGHT => {
                    self.dock_cfg = DockConfig { offset_from_right_dip: 16, offset_from_top_dip: 64 };
                    self.sync_dock_and_zorder(true);
                }
                CMD_PRESET_BOTRIGHT => {
                    if !self.tracked_ag_hwnd.is_null() {
                        let r = get_visual_rect(self.tracked_ag_hwnd);
                        let (_dw, dh) = self.target_dip_size();
                        let h_dip = ((r.height() as f32 / self.scale).round() as i32 - dh - 72).max(40);
                        self.dock_cfg = DockConfig { offset_from_right_dip: 16, offset_from_top_dip: h_dip };
                    }
                    self.sync_dock_and_zorder(true);
                }
                CMD_TOGGLE_PIN => {
                    self.always_topmost = !self.always_topmost;
                    self.sync_dock_and_zorder(false);
                }
                CMD_OPEN_LOGS => {
                    let op = to_wide("open");
                    let path_w = to_wide(&self.analyzer.brain_root.to_string_lossy());
                    ShellExecuteW(null_mut(), op.as_ptr(), path_w.as_ptr(), null(), null(), SW_SHOWNOACTIVATE);
                }
                CMD_EXIT => {
                    DestroyWindow(self.my_hwnd);
                }
                _ => {}
            }
        }
    }
}

unsafe fn get_visual_rect(hwnd: *mut c_void) -> RECT {
    let mut r = RECT::default();
    if DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, &mut r, size_of::<RECT>() as u32) == 0 && r.width() > 0 && r.height() > 0 {
        return r;
    }
    GetWindowRect(hwnd, &mut r);
    r
}

extern "system" fn win_event_proc(
    _hook: *mut c_void,
    event: u32,
    hwnd: *mut c_void,
    id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if id_object != 0 {
        return;
    }
    unsafe {
        if G_STATE.is_null() {
            return;
        }
        let st = &mut *G_STATE;
        if event == EVENT_OBJECT_LOCATIONCHANGE && hwnd != st.tracked_ag_hwnd {
            return;
        }
        if event == EVENT_OBJECT_DESTROY && hwnd != st.tracked_ag_hwnd {
            return;
        }
        st.sync_dock_and_zorder(false);
    }
}

extern "system" fn wnd_proc(hwnd: *mut c_void, msg: u32, w_param: usize, l_param: isize) -> isize {
    unsafe {
        if G_STATE.is_null() {
            return DefWindowProcW(hwnd, msg, w_param, l_param);
        }
        let st = &mut *G_STATE;

        match msg {
            WM_TIMER => {
                if w_param == TIMER_DATA {
                    let (changed, _) = st.analyzer.refresh(false);
                    if changed && st.ag_visible {
                        st.sync_dock_and_zorder(true);
                    }
                } else if w_param == TIMER_DISCOVER {
                    if st.tracked_ag_hwnd.is_null() || IsWindow(st.tracked_ag_hwnd) == 0 {
                        st.sync_dock_and_zorder(false);
                    }
                }
                0
            }
            WM_LBUTTONDOWN => {
                let mut pt = POINT::default();
                GetCursorPos(&mut pt);
                st.dragging = true;
                st.drag_moved = false;
                st.drag_start_cursor = pt;
                st.drag_start_right_dip = st.dock_cfg.offset_from_right_dip;
                st.drag_start_top_dip = st.dock_cfg.offset_from_top_dip;
                SetCapture(hwnd);
                0
            }
            WM_MOUSEMOVE => {
                if st.dragging {
                    let mut pt = POINT::default();
                    GetCursorPos(&mut pt);
                    let dx = pt.x - st.drag_start_cursor.x;
                    let dy = pt.y - st.drag_start_cursor.y;
                    if dx.abs() > 4 || dy.abs() > 4 {
                        st.drag_moved = true;
                    }
                    if st.drag_moved {
                        let dx_dip = (dx as f32 / st.scale).round() as i32;
                        let dy_dip = (dy as f32 / st.scale).round() as i32;
                        st.dock_cfg.offset_from_right_dip = (st.drag_start_right_dip - dx_dip).max(4);
                        st.dock_cfg.offset_from_top_dip = (st.drag_start_top_dip + dy_dip).max(2);
                        st.sync_dock_and_zorder(false);
                    }
                }
                0
            }
            WM_LBUTTONUP => {
                if st.dragging {
                    st.dragging = false;
                    ReleaseCapture();
                    if !st.drag_moved {
                        let lx = (l_param & 0xFFFF) as i16 as i32;
                        let ly = ((l_param >> 16) & 0xFFFF) as i16 as i32;
                        if st.expanded {
                            st.handle_expanded_click(lx, ly);
                        } else {
                            st.expanded = true;
                            st.sync_dock_and_zorder(true);
                        }
                    }
                }
                0
            }
            WM_RBUTTONUP => {
                st.show_context_menu();
                0
            }
            WM_TRAYICON => {
                let ev = (l_param & 0xFFFF) as u32;
                if ev == WM_RBUTTONUP {
                    st.show_context_menu();
                } else if ev == 0x0203 /* WM_LBUTTONDBLCLK */ {
                    st.expanded = !st.expanded;
                    st.sync_dock_and_zorder(true);
                }
                0
            }
            WM_DESTROY => {
                KillTimer(hwnd, TIMER_DATA);
                KillTimer(hwnd, TIMER_DISCOVER);
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, w_param, l_param),
        }
    }
}

unsafe fn create_tray_icon() -> *mut c_void {
    let mut pixels = [0u32; 16 * 16];
    let bg = argb(255, 17, 16, 21);
    let border = argb(255, 56, 189, 248);
    let bar = argb(255, 34, 197, 94);
    for y in 0..16 {
        for x in 0..16 {
            let idx = y * 16 + x;
            if x == 1 || x == 14 || y == 1 || y == 14 {
                pixels[idx] = border;
            } else if (4..=5).contains(&x) && (8..=12).contains(&y) {
                pixels[idx] = bar;
            } else if (7..=8).contains(&x) && (5..=12).contains(&y) {
                pixels[idx] = bar;
            } else if (10..=11).contains(&x) && (3..=12).contains(&y) {
                pixels[idx] = bar;
            } else {
                pixels[idx] = bg;
            }
        }
    }
    let h_color = CreateBitmap(16, 16, 1, 32, pixels.as_ptr() as *const c_void);
    let h_mask = CreateBitmap(16, 16, 1, 1, null());
    let ii = ICONINFO {
        f_icon: 1,
        x_hotspot: 0,
        y_hotspot: 0,
        hbm_mask: h_mask,
        hbm_color: h_color,
    };
    let hicon = CreateIconIndirect(&ii);
    DeleteObject(h_color);
    DeleteObject(h_mask);
    hicon
}

// ========================= Self Test =========================

fn run_self_test(report_path: Option<&str>) -> i32 {
    unsafe {
        AttachConsole(-1);
    }

    let mut log = String::new();
    let mut ok_all = true;

    let mut assert_check = |cond: bool, msg: String| {
        if cond {
            log.push_str("PASS: ");
            log.push_str(&msg);
            log.push('\n');
        } else {
            ok_all = false;
            log.push_str("FAIL: ");
            log.push_str(&msg);
            log.push('\n');
        }
    };

    let work = RECT { left: 0, top: 0, right: 1920, bottom: 1040 };
    let ag = RECT { left: 80, top: 50, right: 1680, bottom: 980 };
    let cfg = DockConfig::default();

    // 1. Default inside-topbar capsule
    let p1 = compute_inside_position(ag, work, CAPSULE_W_DIP, CAPSULE_H_DIP, cfg, 1.0);
    assert_check(
        p1.x >= ag.left && p1.x + CAPSULE_W_DIP <= ag.right && p1.x == 1680 - 152 - 242 && p1.y == 54,
        format!("Inside-topbar capsule -> x={}, y={} (ag=80..1680, top=50)", p1.x, p1.y),
    );

    // 2. Expanded popover aligns right edge with capsule
    let p2 = compute_inside_position(ag, work, POPOVER_W_DIP, POPOVER_H_DIP, cfg, 1.0);
    assert_check(
        (p2.x + POPOVER_W_DIP) == (p1.x + CAPSULE_W_DIP) && p2.y == 54,
        format!("Expanded right-edge aligned -> cap_right={}, pop_right={}", p1.x + CAPSULE_W_DIP, p2.x + POPOVER_W_DIP),
    );

    // 3. Maximized window stays strictly inside
    let ag_max = RECT { left: 0, top: 0, right: 1920, bottom: 1040 };
    let p3 = compute_inside_position(ag_max, work, CAPSULE_W_DIP, CAPSULE_H_DIP, cfg, 1.0);
    assert_check(
        p3.x >= 6 && p3.x + CAPSULE_W_DIP <= 1914 && p3.y == 4,
        format!("Maximized inside-topbar -> x={}, y={}", p3.x, p3.y),
    );

    // 4. Drag clamping stays strictly inside Antigravity
    let bad_drag = DockConfig { offset_from_right_dip: 5000, offset_from_top_dip: 5000 };
    let p4 = compute_inside_position(ag, work, CAPSULE_W_DIP, CAPSULE_H_DIP, bad_drag, 1.0);
    assert_check(
        p4.x == ag.left + 6 && p4.y == ag.bottom - CAPSULE_H_DIP - 6,
        format!("Drag clamped strictly inside -> x={}, y={}", p4.x, p4.y),
    );

    // 5. Flat zero-DOM JSON parser with nested tool_calls & escaped quotes
    let sample = r#"{"step_index":7,"type":"PLANNER_RESPONSE","created_at":"2026-11-22T07:15:53.250Z","model":"gemini-4-neon-xhigh","input_tokens":4504,"cache_read_tokens":16190,"output_tokens":436,"tool_calls":[{"name":"read","args":{"text":"hello \"quoted\" {braces}"}}]}"#;
    let parsed = parse_transcript_line(sample);
    assert_check(
        parsed.as_ref().and_then(|s| s.input_tokens) == Some(4504)
            && parsed.as_ref().and_then(|s| s.cache_read_tokens) == Some(16190)
            && parsed.as_ref().and_then(|s| s.output_tokens) == Some(436)
            && parsed.as_ref().and_then(|s| s.model.as_deref()) == Some("gemini-4-neon-xhigh")
            && parsed.as_ref().and_then(|s| s.created_at_sec).is_some(),
        "Flat JSON scanner handles nested objects and escaped quotes".to_string(),
    );

    // 6. Real transcript.jsonl read-only parse + memory check
    let mut analyzer = TranscriptAnalyzer::new();
    let (_, snap) = analyzer.refresh(true);
    assert_check(
        snap.has_data && snap.context_used > 0 && snap.total_steps > 0,
        format!(
            "Real transcript parsed -> session={}, model={}, steps={}, context={}, hit={:.2}%, avgTps={:.2}",
            snap.session_id, snap.model, snap.total_steps, snap.context_used, snap.session_hit_rate, snap.avg_tps
        ),
    );

    let ws_mb = current_working_set_mb();
    assert_check(
        ws_mb > 0.0 && ws_mb < 15.0,
        format!("Lightweight Working Set RAM = {:.2} MB (< 15 MB target)", ws_mb),
    );

    assert_check(
        CAP_PRESETS.contains(&512_000) && format_cap_label(512_000) == "512K",
        format!("512K preset supported -> presets={:?}, label={}", CAP_PRESETS, format_cap_label(512_000)),
    );

    if let Some(p) = report_path {
        let _ = fs::write(p, &log);
    }
    print!("{}", log);
    if ok_all { 0 } else { 1 }
}

fn main() {
    enable_dpi_awareness();

    let args: Vec<String> = env::args().collect();
    if args.len() > 1 && args[1].eq_ignore_ascii_case("--self-test") {
        let report = args.get(2).map(|s| s.as_str());
        let code = run_self_test(report);
        std::process::exit(code);
    }

    unsafe {
        let mutex_name = to_wide("Local\\AntigravitySideMonitor_SingleInstance");
        let h_mutex = CreateMutexW(null_mut(), 1, mutex_name.as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            if !h_mutex.is_null() {
                CloseHandle(h_mutex);
            }
            return;
        }

        let mut gp_token = 0usize;
        let gp_in = GdiplusStartupInput {
            gdiplus_version: 1,
            debug_event_callback: null(),
            suppress_background_thread: 0,
            suppress_external_codecs: 0,
        };
        if GdiplusStartup(&mut gp_token, &gp_in, null_mut()) != 0 {
            CloseHandle(h_mutex);
            return;
        }

        let mut font_ui: *mut c_void = null_mut();
        let mut font_mono: *mut c_void = null_mut();
        GdipCreateFontFamilyFromName(to_wide("Microsoft YaHei UI").as_ptr(), null_mut(), &mut font_ui);
        if font_ui.is_null() {
            GdipCreateFontFamilyFromName(to_wide("Segoe UI").as_ptr(), null_mut(), &mut font_ui);
        }
        GdipCreateFontFamilyFromName(to_wide("Consolas").as_ptr(), null_mut(), &mut font_mono);
        if font_mono.is_null() {
            font_mono = font_ui;
        }

        let tray_icon = create_tray_icon();

        let state = Box::new(AppState {
            my_hwnd: null_mut(),
            tracked_ag_hwnd: null_mut(),
            ag_visible: false,
            last_x: i32::MIN,
            last_y: i32::MIN,
            last_w: 0,
            last_h: 0,
            last_topmost: None,
            scale: 1.0,
            expanded: false,
            transparent_mode: false,
            always_topmost: false,
            cap_preset_idx: 0,
            dock_cfg: DockConfig::default(),
            dragging: false,
            drag_moved: false,
            drag_start_cursor: POINT::default(),
            drag_start_right_dip: 0,
            drag_start_top_dip: 0,
            pid_cache: HashMap::new(),
            analyzer: TranscriptAnalyzer::new(),
            tray_icon,
            font_family_ui: font_ui,
            font_family_mono: font_mono,
        });
        G_STATE = Box::into_raw(state);

        let h_inst = GetModuleHandleW(null());
        let cls_name = to_wide("AntigravitySideMonitorCapsule");
        let wc = WNDCLASSEXW {
            cb_size: size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfn_wnd_proc: wnd_proc,
            cb_cls_extra: 0,
            cb_wnd_extra: 0,
            h_instance: h_inst,
            h_icon: tray_icon,
            h_cursor: LoadCursorW(null_mut(), IDC_ARROW as *const u16),
            hbr_background: null_mut(),
            lpsz_menu_name: null(),
            lpsz_class_name: cls_name.as_ptr(),
            h_icon_sm: tray_icon,
        };
        RegisterClassExW(&wc);

        let title = to_wide("Antigravity Monitor");
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
            cls_name.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            0,
            0,
            CAPSULE_W_DIP,
            CAPSULE_H_DIP,
            null_mut(),
            null_mut(),
            h_inst,
            null_mut(),
        );
        (*G_STATE).my_hwnd = hwnd;

        // Register system tray icon
        let mut nid: NOTIFYICONDATAW = zeroed();
        nid.cb_size = size_of::<NOTIFYICONDATAW>() as u32;
        nid.h_wnd = hwnd;
        nid.u_id = 1;
        nid.u_flags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.u_callback_message = WM_TRAYICON;
        nid.h_icon = tray_icon;
        let tip = to_wide("Antigravity 顶栏监控 (纯只读)");
        let copy_len = tip.len().min(nid.sz_tip.len());
        nid.sz_tip[..copy_len].copy_from_slice(&tip[..copy_len]);
        Shell_NotifyIconW(NIM_ADD, &nid);

        // Initial data load & dock sync
        (*G_STATE).analyzer.refresh(true);
        (*G_STATE).sync_dock_and_zorder(true);
        trim_working_set();

        // Event-driven window hooks (0% idle CPU)
        let h_fg = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            null_mut(),
            win_event_proc,
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
        let h_min = SetWinEventHook(
            EVENT_SYSTEM_MINIMIZESTART,
            EVENT_SYSTEM_MINIMIZEEND,
            null_mut(),
            win_event_proc,
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
        let h_loc = SetWinEventHook(
            EVENT_OBJECT_DESTROY,
            EVENT_OBJECT_LOCATIONCHANGE,
            null_mut(),
            win_event_proc,
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );

        SetTimer(hwnd, TIMER_DATA, 1500, null());
        SetTimer(hwnd, TIMER_DISCOVER, 2000, null());

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        if !h_fg.is_null() { UnhookWinEvent(h_fg); }
        if !h_min.is_null() { UnhookWinEvent(h_min); }
        if !h_loc.is_null() { UnhookWinEvent(h_loc); }
        Shell_NotifyIconW(NIM_DELETE, &nid);

        let st_box = Box::from_raw(G_STATE);
        G_STATE = null_mut();
        if !st_box.font_family_mono.is_null() && st_box.font_family_mono != st_box.font_family_ui {
            GdipDeleteFontFamily(st_box.font_family_mono);
        }
        if !st_box.font_family_ui.is_null() {
            GdipDeleteFontFamily(st_box.font_family_ui);
        }
        if !st_box.tray_icon.is_null() {
            DestroyIcon(st_box.tray_icon);
        }
        drop(st_box);

        GdiplusShutdown(gp_token);
        CloseHandle(h_mutex);
    }
}
