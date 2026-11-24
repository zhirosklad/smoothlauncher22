#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::ffi::OsStr;
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::OsStrExt;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const HOTKEY_ID: i32 = 9001;
const WM_TRAYICON: u32 = WM_APP + 1;

const ID_CMD_SHOW: usize = 1001;
const ID_CMD_EXIT: usize = 1002;
const ID_CMD_LAUNCH_BASE: usize = 2000;

struct AppEntry {
    label: &'static str,
    target: &'static str,
}

const APPS: &[AppEntry] = &[
    AppEntry { label: "Блокнот", target: "notepad.exe" },
    AppEntry { label: "Калькулятор", target: "calc.exe" },
    AppEntry { label: "Проводник", target: "explorer.exe" },
    AppEntry { label: "Диспетчер задач", target: "taskmgr.exe" },
];

struct LauncherState {
    hwnd: HWND,
    tray_nid: NOTIFYICONDATAW,
    visible: bool,
    opacity: u8,
    target_opacity: u8,
    hover_index: isize,
}

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn set_window_opacity(hwnd: HWND, alpha: u8) {
    unsafe {
        SetLayeredWindowAttributes(hwnd, 0, alpha, LWA_ALPHA);
    }
}

fn start_fade(state: &mut LauncherState, show: bool) {
    unsafe {
        if show {
            state.target_opacity = 246; // ~96% непрозрачности
            if !state.visible {
                state.opacity = 0;
                set_window_opacity(state.hwnd, 0);
                ShowWindow(state.hwnd, SW_SHOWNOACTIVATE);
                SetForegroundWindow(state.hwnd);
                state.visible = true;
            }
            KillTimer(state.hwnd, 2);
            SetTimer(state.hwnd, 1, 16, None); // ~60 FPS плавное проявление
        } else if state.visible {
            state.target_opacity = 0;
            KillTimer(state.hwnd, 1);
            SetTimer(state.hwnd, 2, 16, None); // Плавное скрытие
        }
    }
}

fn launch_app(target: &str) {
    let w_target = to_wide(target);
    let w_open = to_wide("open");
    unsafe {
        ShellExecuteW(
            null_mut(),
            w_open.as_ptr(),
            w_target.as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        );
    }
}

fn show_tray_menu(state: &mut LauncherState) {
    unsafe {
        let hmenu = CreatePopupMenu();
        if hmenu.is_null() {
            return;
        }

        let w_open = to_wide("Открыть панель (Alt+Space)");
        AppendMenuW(hmenu, MF_STRING, ID_CMD_SHOW, w_open.as_ptr());
        AppendMenuW(hmenu, MF_SEPARATOR, 0, null());

        for (idx, app) in APPS.iter().enumerate() {
            let w_label = to_wide(app.label);
            AppendMenuW(hmenu, MF_STRING, ID_CMD_LAUNCH_BASE + idx, w_label.as_ptr());
        }

        AppendMenuW(hmenu, MF_SEPARATOR, 0, null());
        let w_exit = to_wide("Выход");
        AppendMenuW(hmenu, MF_STRING, ID_CMD_EXIT, w_exit.as_ptr());

        let mut pt: POINT = zeroed();
        GetCursorPos(&mut pt);
        SetForegroundWindow(state.hwnd);
        let cmd = TrackPopupMenu(
            hmenu,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
            pt.x,
            pt.y,
            0,
            state.hwnd,
            null(),
        );
        DestroyMenu(hmenu);

        if cmd == ID_CMD_SHOW as BOOL {
            start_fade(state, true);
        } else if cmd == ID_CMD_EXIT as BOOL {
            DestroyWindow(state.hwnd);
        } else if (cmd as usize) >= ID_CMD_LAUNCH_BASE
            && (cmd as usize) < ID_CMD_LAUNCH_BASE + APPS.len()
        {
            let idx = (cmd as usize) - ID_CMD_LAUNCH_BASE;
            launch_app(APPS[idx].target);
            start_fade(state, false);
        }
    }
}

fn hit_test_item(y: i32) -> isize {
    let start_y = 52;
    let item_h = 34;
    let gap = 6;
    for i in 0..APPS.len() {
        let top = start_y + (i as i32) * (item_h + gap);
        let bottom = top + item_h;
        if y >= top && y <= bottom {
            return i as isize;
        }
    }
    -1
}

fn paint_overlay(state: &LauncherState, hdc: HDC) {
    unsafe {
        let width = 340;
        let height = 240;

        // Фон панели
        let bg_brush = CreateSolidBrush(RGB(28, 30, 36));
        let full_rect = RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };
        FillRect(hdc, &full_rect, bg_brush);
        DeleteObject(bg_brush);

        // Тонкая рамка
        let border_pen = CreatePen(PS_SOLID as i32, 1, RGB(65, 72, 88));
        let old_pen = SelectObject(hdc, border_pen);
        let old_brush = SelectObject(hdc, GetStockObject(NULL_BRUSH));
        Rectangle(hdc, 0, 0, width, height);
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        DeleteObject(border_pen);

        SetBkMode(hdc, TRANSPARENT as i32);

        // Заголовок
        let title_font = CreateFontW(
            -18,
            0,
            0,
            0,
            FW_BOLD as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            DEFAULT_PITCH as u32,
            to_wide("Segoe UI").as_ptr(),
        );
        let old_font = SelectObject(hdc, title_font);
        SetTextColor(hdc, RGB(240, 244, 248));
        let mut title_rect = RECT {
            left: 18,
            top: 14,
            right: width - 18,
            bottom: 42,
        };
        let w_title = to_wide("Быстрый запуск (Alt+Space)");
        DrawTextW(
            hdc,
            w_title.as_ptr(),
            -1,
            &mut title_rect,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
        SelectObject(hdc, old_font);
        DeleteObject(title_font);

        // Кнопки приложений
        let btn_font = CreateFontW(
            -15,
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            DEFAULT_PITCH as u32,
            to_wide("Segoe UI").as_ptr(),
        );
        let old_btn_font = SelectObject(hdc, btn_font);

        let start_y = 52;
        let item_h = 34;
        let gap = 6;

        for (i, app) in APPS.iter().enumerate() {
            let top = start_y + (i as i32) * (item_h + gap);
            let bottom = top + item_h;
            let is_hover = state.hover_index == (i as isize);

            let btn_color = if is_hover {
                RGB(62, 72, 92)
            } else {
                RGB(42, 46, 55)
            };
            let btn_brush = CreateSolidBrush(btn_color);
            let btn_rect = RECT {
                left: 18,
                top,
                right: width - 18,
                bottom,
            };
            FillRect(hdc, &btn_rect, btn_brush);
            DeleteObject(btn_brush);

            SetTextColor(hdc, RGB(255, 255, 255));
            let mut text_rect = RECT {
                left: 30,
                top,
                right: width - 28,
                bottom,
            };
            let w_label = to_wide(app.label);
            DrawTextW(
                hdc,
                w_label.as_ptr(),
                -1,
                &mut text_rect,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
        }

        SelectObject(hdc, old_btn_font);
        DeleteObject(btn_font);
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let cs = &*(lparam as *const CREATESTRUCTW);
            let state_ptr = cs.lpCreateParams as *mut LauncherState;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
            RegisterHotKey(hwnd, HOTKEY_ID, MOD_ALT, VK_SPACE as u32);
            0
        }
        WM_HOTKEY => {
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut LauncherState;
            if !state_ptr.is_null() && wparam as i32 == HOTKEY_ID {
                let state = &mut *state_ptr;
                if state.visible {
                    start_fade(state, false);
                } else {
                    start_fade(state, true);
                }
            }
            0
        }
        WM_TIMER => {
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut LauncherState;
            if !state_ptr.is_null() {
                let state = &mut *state_ptr;
                let step: u8 = 36;
                if wparam == 1 {
                    // Плавное появление
                    if (state.target_opacity as i32) - (state.opacity as i32) <= step as i32 {
                        state.opacity = state.target_opacity;
                        set_window_opacity(hwnd, state.opacity);
                        KillTimer(hwnd, 1);
                    } else {
                        state.opacity = state.opacity.saturating_add(step);
                        set_window_opacity(hwnd, state.opacity);
                    }
                } else if wparam == 2 {
                    // Плавное скрытие
                    if (state.opacity as i32) <= step as i32 {
                        state.opacity = 0;
                        set_window_opacity(hwnd, 0);
                        KillTimer(hwnd, 2);
                        ShowWindow(hwnd, SW_HIDE);
                        state.visible = false;
                        state.hover_index = -1;
                    } else {
                        state.opacity = state.opacity.saturating_sub(step);
                        set_window_opacity(hwnd, state.opacity);
                    }
                }
            }
            0
        }
        WM_MOUSEMOVE => {
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut LauncherState;
            if !state_ptr.is_null() {
                let state = &mut *state_ptr;
                let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
                let new_hover = hit_test_item(y);
                if new_hover != state.hover_index {
                    state.hover_index = new_hover;
                    InvalidateRect(hwnd, null(), 0);
                }
            }
            0
        }
        WM_LBUTTONDOWN => {
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut LauncherState;
            if !state_ptr.is_null() {
                let state = &mut *state_ptr;
                let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
                let idx = hit_test_item(y);
                if idx >= 0 && (idx as usize) < APPS.len() {
                    launch_app(APPS[idx as usize].target);
                    start_fade(state, false);
                }
            }
            0
        }
        WM_KILLFOCUS => {
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut LauncherState;
            if !state_ptr.is_null() {
                let state = &mut *state_ptr;
                if state.visible {
                    start_fade(state, false);
                }
            }
            0
        }
        WM_PAINT => {
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const LauncherState;
            let mut ps: PAINTSTRUCT = zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            if !state_ptr.is_null() {
                paint_overlay(&*state_ptr, hdc);
            }
            EndPaint(hwnd, &ps);
            0
        }
        WM_TRAYICON => {
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut LauncherState;
            if !state_ptr.is_null() {
                let state = &mut *state_ptr;
                let event = (lparam as u32) & 0xFFFF;
                if event == WM_LBUTTONDBLCLK {
                    start_fade(state, true);
                } else if event == WM_RBUTTONUP {
                    show_tray_menu(state);
                }
            }
            0
        }
        WM_DESTROY => {
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut LauncherState;
            if !state_ptr.is_null() {
                let state = &mut *state_ptr;
                Shell_NotifyIconW(NIM_DELETE, &mut state.tray_nid);
            }
            UnregisterHotKey(hwnd, HOTKEY_ID);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn main() {
    unsafe {
        let hinstance = GetModuleHandleW(null());
        let class_name = to_wide("SmoothRustLauncherWnd");

        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: LoadIconW(null_mut(), IDI_APPLICATION),
            hCursor: LoadCursorW(null_mut(), IDC_HAND),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class_name.as_ptr(),
        };

        RegisterClassW(&wc);

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let win_w = 340;
        let win_h = 240;
        let pos_x = (screen_w - win_w) / 2;
        let pos_y = (screen_h - win_h) / 2;

        let mut state = LauncherState {
            hwnd: null_mut(),
            tray_nid: zeroed(),
            visible: false,
            opacity: 0,
            target_opacity: 246,
            hover_index: -1,
        };

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            class_name.as_ptr(),
            to_wide("Smooth Launcher").as_ptr(),
            WS_POPUP,
            pos_x,
            pos_y,
            win_w,
            win_h,
            null_mut(),
            null_mut(),
            hinstance,
            &mut state as *mut LauncherState as *const _,
        );

        if hwnd.is_null() {
            return;
        }

        state.hwnd = hwnd;
        set_window_opacity(hwnd, 0);

        // Регистрация иконки в системном трее
        let mut nid: NOTIFYICONDATAW = zeroed();
        nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        nid.uCallbackMessage = WM_TRAYICON;
        nid.hIcon = LoadIconW(null_mut(), IDI_APPLICATION);
        let tip = to_wide("Smooth Rust Launcher (фон)");
        let copy_len = tip.len().min(nid.szTip.len());
        nid.szTip[..copy_len].copy_from_slice(&tip[..copy_len]);
        Shell_NotifyIconW(NIM_ADD, &mut nid);
        state.tray_nid = nid;

        // Главный цикл сообщений без активного ожидания (0% CPU в простое)
        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            let thread_id = GetCurrentThreadId();
            let _ = thread_id;
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
