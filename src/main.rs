#![windows_subsystem = "windows"]

use std::os::windows::process::CommandExt;
use std::process::Command;
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::UI::Controls::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

mod daemon;
mod smooth;

const HID: &str = r"SYSTEM\CurrentControlSet\Enum\HID";

pub fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// 遍历 HID 下的所有设备实例，返回 (实例 ID, Device Parameters 句柄)。
/// 只返回已有 FlipFlopWheel 值的设备，也就是带滚轮的鼠标。
fn mice(write: bool) -> Vec<(String, HKEY)> {
    let mut out = vec![];
    let access = if write {
        KEY_QUERY_VALUE | KEY_SET_VALUE
    } else {
        KEY_QUERY_VALUE
    };
    for dev in subkeys(HID) {
        for inst in subkeys(&format!(r"{HID}\{dev}")) {
            let path = w(&format!(r"{HID}\{dev}\{inst}\Device Parameters"));
            let mut k: HKEY = null_mut();
            // 有些实例没有权限打开，直接跳过
            if unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, path.as_ptr(), 0, access, &mut k) }
                != ERROR_SUCCESS
            {
                continue;
            }
            if read_dword(k, "FlipFlopWheel").is_some() {
                out.push((format!(r"HID\{dev}\{inst}"), k));
            } else {
                unsafe { RegCloseKey(k) };
            }
        }
    }
    out
}

fn subkeys(path: &str) -> Vec<String> {
    let mut k: HKEY = null_mut();
    let p = w(path);
    if unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            p.as_ptr(),
            0,
            KEY_ENUMERATE_SUB_KEYS,
            &mut k,
        )
    } != ERROR_SUCCESS
    {
        return vec![];
    }
    let mut names = vec![];
    let mut buf = [0u16; 256];
    for i in 0.. {
        let mut len = buf.len() as u32;
        let r = unsafe {
            RegEnumKeyExW(
                k,
                i,
                buf.as_mut_ptr(),
                &mut len,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
            )
        };
        if r != ERROR_SUCCESS {
            break;
        }
        names.push(String::from_utf16_lossy(&buf[..len as usize]));
    }
    unsafe { RegCloseKey(k) };
    names
}

fn read_dword(k: HKEY, name: &str) -> Option<u32> {
    let (mut v, mut len) = (0u32, 4u32);
    let n = w(name);
    let r = unsafe {
        RegQueryValueExW(
            k,
            n.as_ptr(),
            null_mut(),
            null_mut(),
            &mut v as *mut _ as *mut u8,
            &mut len,
        )
    };
    (r == ERROR_SUCCESS).then_some(v)
}

fn write_dword(k: HKEY, name: &str, v: u32) -> bool {
    let n = w(name);
    unsafe {
        RegSetValueExW(k, n.as_ptr(), 0, REG_DWORD, &v as *const _ as *const u8, 4) == ERROR_SUCCESS
    }
}

/// 简体/繁体中文界面用中文，其余用英文
pub fn t(zh: &'static str, en: &'static str) -> &'static str {
    if unsafe { GetUserDefaultUILanguage() } & 0x3ff == 0x04 {
        zh
    } else {
        en
    }
}

fn msgbox(text: &str, flags: MESSAGEBOX_STYLE) {
    let (t, c) = (w(text), w("Natural Scroll"));
    unsafe { MessageBoxW(null_mut(), t.as_ptr(), c.as_ptr(), flags) };
}

/// None = 没找到带滚轮的鼠标；Some(true) = 当前是自然滚动
fn direction() -> Option<bool> {
    let devices = mice(false);
    let natural = devices
        .iter()
        .any(|(_, k)| read_dword(*k, "FlipFlopWheel") == Some(1));
    let found = !devices.is_empty();
    for (_, k) in devices {
        unsafe { RegCloseKey(k) };
    }
    found.then_some(natural)
}

fn set_direction(natural: bool) {
    let v = natural as u32;
    let (mut ok, mut failed) = (0, 0);
    for (id, k) in mice(true) {
        if write_dword(k, "FlipFlopWheel", v) && write_dword(k, "FlipFlopHScroll", v) {
            ok += 1;
            // 重启设备让设置立即生效（Win10 2004+）；失败就靠重新插拔
            let _ = Command::new("pnputil")
                .args(["/restart-device", &id])
                .creation_flags(0x0800_0000)
                .status();
        } else {
            failed += 1;
        }
        unsafe { RegCloseKey(k) };
    }
    if failed == 0 {
        let msg = format!(
            "{} {ok}\n\n{}",
            t("已更新鼠标数量：", "Mice updated:"),
            t(
                "如果滚动方向没变，请重新插拔鼠标或重启电脑。",
                "If the direction hasn't changed, reconnect the mouse or restart the PC."
            )
        );
        msgbox(&msg, MB_ICONINFORMATION);
    } else {
        let msg = format!(
            "{} {ok} / {failed}",
            t("成功 / 失败：", "Succeeded / failed:")
        );
        msgbox(&msg, MB_ICONERROR);
    }
}

const BTN_DIRECTION: i32 = 101;
const BTN_SMOOTH: i32 = 102;

/// 主界面。返回被点击的按钮 ID。
fn dialog(dir: Option<bool>, smooth: bool) -> i32 {
    let status = format!(
        "{}{}\n{}{}",
        t("滚动方向：", "Direction: "),
        match dir {
            Some(true) => t("自然滚动（Mac）", "Natural (Mac)"),
            Some(false) => t("Windows 默认", "Windows default"),
            None => t("没有找到带滚轮的鼠标", "No mouse with a scroll wheel found"),
        },
        t("平滑滚动：", "Smooth scrolling: "),
        if smooth {
            t("开启", "On")
        } else {
            t("关闭", "Off")
        },
    );
    let dir_text = if dir == Some(true) {
        t("恢复 Windows 默认方向", "Restore Windows default direction")
    } else {
        t(
            "改成自然滚动（Mac 方向）\n纵向和横向都会反转，改完不需要后台运行",
            "Switch to natural scrolling (Mac style)\nReverses vertical and horizontal; nothing runs in the background",
        )
    };
    let smooth_text = if smooth {
        t("关闭平滑滚动", "Turn off smooth scrolling")
    } else {
        t(
            "开启平滑滚动和惯性\n常驻托盘并开机启动，全屏游戏时自动停用",
            "Turn on smooth scrolling with inertia\nStays in the tray, starts at sign-in, pauses in full-screen games",
        )
    };
    let (title, main, status, dir_text, smooth_text) = (
        w("Natural Scroll"),
        w(t("Mac 风格的鼠标滚动", "Mac-style mouse scrolling")),
        w(&status),
        w(dir_text),
        w(smooth_text),
    );
    let mut buttons = vec![TASKDIALOG_BUTTON {
        nButtonID: BTN_SMOOTH,
        pszButtonText: smooth_text.as_ptr(),
    }];
    if dir.is_some() {
        buttons.insert(
            0,
            TASKDIALOG_BUTTON {
                nButtonID: BTN_DIRECTION,
                pszButtonText: dir_text.as_ptr(),
            },
        );
    }
    let mut cfg: TASKDIALOGCONFIG = unsafe { std::mem::zeroed() };
    cfg.cbSize = std::mem::size_of::<TASKDIALOGCONFIG>() as u32;
    cfg.dwFlags = TDF_USE_COMMAND_LINKS | TDF_ALLOW_DIALOG_CANCELLATION;
    cfg.dwCommonButtons = TDCBF_CLOSE_BUTTON;
    cfg.pszWindowTitle = title.as_ptr();
    cfg.pszMainInstruction = main.as_ptr();
    cfg.pszContent = status.as_ptr();
    cfg.cButtons = buttons.len() as u32;
    cfg.pButtons = buttons.as_ptr();
    let mut clicked = 0;
    unsafe { TaskDialogIndirect(&cfg, &mut clicked, null_mut(), null_mut()) };
    clicked
}

fn main() {
    // 声明支持高 DPI，否则 125%/150% 缩放下 Windows 会把界面当位图放大，文字发糊。
    // 必须在创建任何窗口之前调用；Win10 1703 以前不支持，失败就保持原样。
    unsafe {
        windows_sys::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
            windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        )
    };
    if std::env::args().nth(1).as_deref() == Some("--smooth") {
        return daemon::run();
    }
    loop {
        let dir = direction();
        let smooth = daemon::is_enabled();
        match dialog(dir, smooth) {
            BTN_DIRECTION => set_direction(dir != Some(true)),
            BTN_SMOOTH if smooth => daemon::disable(),
            BTN_SMOOTH if !daemon::enable() => msgbox(
                t(
                    "开启失败：无法创建开机启动任务。",
                    "Failed to create the sign-in task.",
                ),
                MB_ICONERROR,
            ),
            BTN_SMOOTH => {}
            _ => return,
        }
    }
}
