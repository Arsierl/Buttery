#![windows_subsystem = "windows"]

use std::os::windows::process::CommandExt;
use std::process::Command;
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const HID: &str = r"SYSTEM\CurrentControlSet\Enum\HID";

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// 遍历 HID 下的所有设备实例，返回 (实例 ID, Device Parameters 句柄)。
/// 只返回已有 FlipFlopWheel 值的设备，也就是带滚轮的鼠标。
fn mice(write: bool) -> Vec<(String, HKEY)> {
    let mut out = vec![];
    let access = if write { KEY_QUERY_VALUE | KEY_SET_VALUE } else { KEY_QUERY_VALUE };
    for dev in subkeys(HID) {
        for inst in subkeys(&format!(r"{HID}\{dev}")) {
            let path = w(&format!(r"{HID}\{dev}\{inst}\Device Parameters"));
            let mut k: HKEY = null_mut();
            // 有些实例没有权限打开，直接跳过
            if unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, path.as_ptr(), 0, access, &mut k) } != ERROR_SUCCESS {
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
    if unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, p.as_ptr(), 0, KEY_ENUMERATE_SUB_KEYS, &mut k) } != ERROR_SUCCESS {
        return vec![];
    }
    let mut names = vec![];
    let mut buf = [0u16; 256];
    for i in 0.. {
        let mut len = buf.len() as u32;
        let r = unsafe { RegEnumKeyExW(k, i, buf.as_mut_ptr(), &mut len, null_mut(), null_mut(), null_mut(), null_mut()) };
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
    let r = unsafe { RegQueryValueExW(k, n.as_ptr(), null_mut(), null_mut(), &mut v as *mut _ as *mut u8, &mut len) };
    (r == ERROR_SUCCESS).then_some(v)
}

fn write_dword(k: HKEY, name: &str, v: u32) -> bool {
    let n = w(name);
    unsafe { RegSetValueExW(k, n.as_ptr(), 0, REG_DWORD, &v as *const _ as *const u8, 4) == ERROR_SUCCESS }
}

fn msgbox(text: &str, flags: MESSAGEBOX_STYLE) -> MESSAGEBOX_RESULT {
    let (t, c) = (w(text), w("Natural Scroll"));
    unsafe { MessageBoxW(null_mut(), t.as_ptr(), c.as_ptr(), flags) }
}

fn main() {
    // 简体/繁体中文界面用中文，其余用英文
    let zh = unsafe { GetUserDefaultUILanguage() } & 0x3ff == 0x04;
    let t = |zh_s: &'static str, en_s: &'static str| if zh { zh_s } else { en_s };

    let devices = mice(false);
    if devices.is_empty() {
        msgbox(t("没有找到带滚轮的鼠标。", "No mouse with a scroll wheel was found."), MB_ICONWARNING);
        return;
    }
    let natural_now = devices.iter().any(|(_, k)| read_dword(*k, "FlipFlopWheel") == Some(1));
    for (_, k) in devices {
        unsafe { RegCloseKey(k) };
    }

    let prompt = if natural_now {
        t("当前：自然滚动（Mac 方向）\n\n要恢复成 Windows 默认方向吗？", "Current: natural scrolling (Mac style)\n\nRestore the Windows default direction?")
    } else {
        t("当前：Windows 默认方向\n\n要改成自然滚动（Mac 方向）吗？\n纵向和横向滚动都会反转。", "Current: Windows default direction\n\nSwitch to natural scrolling (Mac style)?\nBoth vertical and horizontal scrolling will be reversed.")
    };
    if msgbox(prompt, MB_YESNO | MB_ICONQUESTION) != IDYES {
        return;
    }

    let target = if natural_now { 0 } else { 1 };
    let (mut ok, mut failed) = (0, 0);
    for (id, k) in mice(true) {
        if write_dword(k, "FlipFlopWheel", target) && write_dword(k, "FlipFlopHScroll", target) {
            ok += 1;
            // 重启设备让设置立即生效（Win10 2004+）；失败就靠重新插拔
            let _ = Command::new("pnputil").args(["/restart-device", &id]).creation_flags(0x0800_0000).status();
        } else {
            failed += 1;
        }
        unsafe { RegCloseKey(k) };
    }

    let msg = if failed == 0 {
        format!("{} {ok}\n\n{}", t("已更新鼠标数量：", "Mice updated:"), t("如果滚动方向没变，请重新插拔鼠标或重启电脑。", "If the direction hasn't changed, reconnect the mouse or restart the PC."))
    } else {
        format!("{} {ok} / {failed}\n\n{}", t("成功 / 失败：", "Succeeded / failed:"), t("请右键选择“以管理员身份运行”。", "Right-click and choose \"Run as administrator\"."))
    };
    msgbox(&msg, if failed == 0 { MB_ICONINFORMATION } else { MB_ICONERROR });
}
