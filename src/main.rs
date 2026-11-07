#![windows_subsystem = "windows"]

use std::os::windows::process::CommandExt;
use std::process::Command;
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

mod daemon;
mod smooth;

const HID: &str = r"SYSTEM\CurrentControlSet\Enum\HID";

pub fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

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

fn set_direction(natural: bool) -> bool {
    let v = natural as u32;
    let mut ok = true;
    for (id, k) in mice(true) {
        if write_dword(k, "FlipFlopWheel", v) && write_dword(k, "FlipFlopHScroll", v) {
            let _ = Command::new("pnputil")
                .args(["/restart-device", &id])
                .creation_flags(0x0800_0000)
                .status();
        } else {
            ok = false;
        }
        unsafe { RegCloseKey(k) };
    }
    ok
}

slint::slint! {
    import { Switch } from "std-widgets.slint";

    component Row inherits HorizontalLayout {
        in property <string> label;
        in-out property <bool> checked;
        in property <bool> active: true;
        callback toggled(bool);
        alignment: space-between;
        Text { text: label; vertical-alignment: center; font-size: 14px; }
        Switch {
            checked <=> root.checked;
            enabled: root.active;
            toggled => { root.toggled(self.checked); }
        }
    }

    export component App inherits Window {
        title: "Natural Scroll";
        min-width: 340px;
        in property <string> natural-label;
        in property <string> smooth-label;
        in property <bool> has-mouse;
        in-out property <bool> natural;
        in-out property <bool> smooth;
        callback toggle-natural(bool);
        callback toggle-smooth(bool);

        VerticalLayout {
            padding: 24px;
            spacing: 18px;
            Row {
                label: natural-label;
                checked <=> root.natural;
                active: has-mouse;
                toggled(on) => { root.toggle-natural(on); }
            }
            Row {
                label: smooth-label;
                checked <=> root.smooth;
                toggled(on) => { root.toggle-smooth(on); }
            }
        }
    }
}

type Job = Box<dyn FnOnce() + Send>;

fn background(
    weak: &slint::Weak<App>,
    work: impl FnOnce() -> bool + Send + 'static,
    revert: impl FnOnce(&App) + Send + 'static,
) {
    static QUEUE: std::sync::OnceLock<std::sync::mpsc::Sender<Job>> = std::sync::OnceLock::new();
    let queue = QUEUE.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Job>();
        std::thread::spawn(move || rx.into_iter().for_each(|job| job()));
        tx
    });
    let weak = weak.clone();
    let _ = queue.send(Box::new(move || {
        if !work() {
            let _ = weak.upgrade_in_event_loop(move |app| revert(&app));
            msgbox(t("操作失败", "Failed"), MB_ICONERROR);
        }
    }));
}

fn main() {
    unsafe {
        windows_sys::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
            windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        )
    };
    if std::env::args().nth(1).as_deref() == Some("--smooth") {
        return daemon::run();
    }
    let app = App::new().unwrap();
    let dir = direction();
    app.set_natural_label(t("自然滚动", "Natural scrolling").into());
    app.set_smooth_label(t("平滑滚动", "Smooth scrolling").into());
    app.set_has_mouse(dir.is_some());
    app.set_natural(dir == Some(true));
    app.set_smooth(daemon::is_enabled());

    let weak = app.as_weak();
    app.on_toggle_natural(move |on| {
        background(
            &weak,
            move || set_direction(on),
            move |app| app.set_natural(!on),
        )
    });
    let weak = app.as_weak();
    app.on_toggle_smooth(move |on| {
        background(
            &weak,
            move || {
                if on {
                    daemon::enable()
                } else {
                    daemon::disable();
                    true
                }
            },
            |app| app.set_smooth(false),
        )
    });
    app.run().unwrap();
}
