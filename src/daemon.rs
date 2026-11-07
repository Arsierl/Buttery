use crate::smooth::Axis;
use crate::{t, w};
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Dwm::DwmFlush;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const TASK: &str = "Buttery";
const CLASS: &str = "ButterySmooth";
const NO_WINDOW: u32 = 0x0800_0000;
const TRAY_MSG: u32 = WM_APP + 1;
const MENU_OFF: usize = 1;
const FRAME: Duration = Duration::from_millis(16);

static AXES: Mutex<[Axis; 2]> = Mutex::new([Axis::new(), Axis::new()]);
static WAKE: Condvar = Condvar::new();
static START: OnceLock<Instant> = OnceLock::new();
static FULLSCREEN: AtomicBool = AtomicBool::new(false);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

pub fn is_enabled() -> bool {
    schtasks(&["/query", "/tn", TASK])
}

pub fn enable() -> bool {
    let exe = std::env::current_exe().unwrap().display().to_string();
    let user = format!(
        "{}\\{}",
        std::env::var("USERDOMAIN").unwrap_or_default(),
        std::env::var("USERNAME").unwrap_or_default()
    );
    let esc = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <Triggers><LogonTrigger><UserId>{u}</UserId></LogonTrigger></Triggers>
  <Principals><Principal id="Author"><UserId>{u}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>HighestAvailable</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
  </Settings>
  <Actions Context="Author"><Exec><Command>{e}</Command><Arguments>--smooth</Arguments></Exec></Actions>
</Task>"#,
        u = esc(&user),
        e = esc(&exe)
    );
    let path = std::env::temp_dir().join("buttery-task.xml");
    let bytes: Vec<u8> = std::iter::once(0xFEFF)
        .chain(xml.encode_utf16())
        .flat_map(u16::to_le_bytes)
        .collect();
    if std::fs::write(&path, bytes).is_err() {
        return false;
    }
    let ok = schtasks(&[
        "/create",
        "/tn",
        TASK,
        "/xml",
        &path.display().to_string(),
        "/f",
    ]);
    let _ = std::fs::remove_file(&path);
    if ok {
        let _ = Command::new(&exe)
            .arg("--smooth")
            .creation_flags(NO_WINDOW)
            .spawn();
    }
    ok
}

pub fn disable() {
    schtasks(&["/delete", "/tn", TASK, "/f"]);
    let class = w(CLASS);
    let hwnd = unsafe { FindWindowW(class.as_ptr(), null_mut()) };
    if !hwnd.is_null() {
        unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) };
    }
}

fn schtasks(args: &[&str]) -> bool {
    Command::new("schtasks")
        .args(args)
        .creation_flags(NO_WINDOW)
        .output()
        .is_ok_and(|o| o.status.success())
}

pub fn run() {
    let class = w(CLASS);
    unsafe {
        if !FindWindowW(class.as_ptr(), null_mut()).is_null() {
            return;
        }
        SetPriorityClass(GetCurrentProcess(), ABOVE_NORMAL_PRIORITY_CLASS);
        START.get_or_init(Instant::now);

        let hinst = GetModuleHandleW(null_mut());
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            hinst,
            null_mut(),
        );

        let tc = RegisterWindowMessageW(w("TaskbarCreated").as_ptr());
        TASKBAR_CREATED.store(tc, Ordering::Relaxed);
        for m in [tc, TRAY_MSG] {
            ChangeWindowMessageFilterEx(hwnd, m, MSGFLT_ALLOW, null_mut());
        }
        tray(hwnd, NIM_ADD);
        SetTimer(hwnd, 1, 1000, None);

        std::thread::spawn(animate);
        let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(hook), hinst, 0);

        let mut msg = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            DispatchMessageW(&msg);
        }
        UnhookWindowsHookEx(hook);
    }
}

unsafe extern "system" fn hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        let wheel = wp == WM_MOUSEWHEEL as usize || wp == WM_MOUSEHWHEEL as usize;
        if code == HC_ACTION as i32 && wheel {
            let info = &*(lp as *const MSLLHOOKSTRUCT);
            if info.flags & LLMHF_INJECTED == 0
                && !FULLSCREEN.load(Ordering::Relaxed)
                && !modifier_down()
            {
                let delta = (info.mouseData >> 16) as i16 as i32;
                let now = START.get().unwrap().elapsed().as_secs_f64();
                AXES.lock().unwrap()[(wp == WM_MOUSEHWHEEL as usize) as usize].push(delta, now);
                WAKE.notify_one();
                return 1;
            }
        }
        CallNextHookEx(null_mut(), code, wp, lp)
    }
}

fn modifier_down() -> bool {
    unsafe { GetAsyncKeyState(VK_CONTROL as i32) < 0 || GetAsyncKeyState(VK_MENU as i32) < 0 }
}

fn animate() {
    loop {
        let mut g = AXES.lock().unwrap();
        while !g.iter().any(Axis::active) {
            g = WAKE.wait(g).unwrap();
        }
        drop(g);
        let mut last = Instant::now();
        loop {
            if unsafe { DwmFlush() } < 0 || last.elapsed() < Duration::from_millis(2) {
                std::thread::sleep(FRAME);
            }
            let now = Instant::now();
            let dt = (now - last).as_secs_f64();
            last = now;
            let (v, h, active) = {
                let mut g = AXES.lock().unwrap();
                if modifier_down() {
                    g.iter_mut().for_each(Axis::stop);
                }
                (g[0].tick(dt), g[1].tick(dt), g.iter().any(Axis::active))
            };
            send(v, h);
            if !active {
                break;
            }
        }
    }
}

fn send(v: i32, h: i32) {
    let inputs: Vec<INPUT> = [(v, MOUSEEVENTF_WHEEL), (h, MOUSEEVENTF_HWHEEL)]
        .into_iter()
        .filter(|(d, _)| *d != 0)
        .map(|(d, flag)| INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: d as _,
                    dwFlags: flag,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        })
        .collect();
    if !inputs.is_empty() {
        unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            )
        };
    }
}

fn query_fullscreen() -> bool {
    let mut s = 0;
    unsafe {
        SHQueryUserNotificationState(&mut s) >= 0
            && matches!(
                s,
                QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE
            )
    }
}

unsafe fn tray(hwnd: HWND, op: NOTIFY_ICON_MESSAGE) {
    unsafe {
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = TRAY_MSG;
        nid.hIcon = LoadIconW(null_mut(), IDI_APPLICATION);
        let tip: Vec<u16> = "Buttery".encode_utf16().take(127).collect();
        nid.szTip[..tip.len()].copy_from_slice(&tip);
        Shell_NotifyIconW(op, &nid);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_TIMER => FULLSCREEN.store(query_fullscreen(), Ordering::Relaxed),
            TRAY_MSG if matches!(lp as u32, WM_RBUTTONUP | WM_LBUTTONUP) => {
                let menu = CreatePopupMenu();
                AppendMenuW(
                    menu,
                    MF_STRING,
                    MENU_OFF,
                    w(t("关闭平滑滚动", "Turn off smooth scrolling")).as_ptr(),
                );
                let mut pt = std::mem::zeroed();
                GetCursorPos(&mut pt);
                SetForegroundWindow(hwnd);
                let cmd = TrackPopupMenu(
                    menu,
                    TPM_RETURNCMD | TPM_NONOTIFY,
                    pt.x,
                    pt.y,
                    0,
                    hwnd,
                    null_mut(),
                );
                DestroyMenu(menu);
                if cmd as usize == MENU_OFF {
                    schtasks(&["/delete", "/tn", TASK, "/f"]);
                    DestroyWindow(hwnd);
                }
            }
            WM_CLOSE => {
                DestroyWindow(hwnd);
            }
            WM_DESTROY => {
                tray(hwnd, NIM_DELETE);
                PostQuitMessage(0);
            }
            m if m == TASKBAR_CREATED.load(Ordering::Relaxed) && m != 0 => tray(hwnd, NIM_ADD),
            _ => return DefWindowProcW(hwnd, msg, wp, lp),
        }
        0
    }
}
