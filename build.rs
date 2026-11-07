// 内嵌应用清单。放在这里而不是 .cargo/config.toml，是因为 cargo-xwin 会按空格拆开 rustflags。
fn main() {
    for arg in [
        // 双击就弹管理员授权
        "/MANIFEST:EMBED",
        "/MANIFESTUAC:level='requireAdministrator'",
        // TaskDialog 需要 Common Controls v6
        "/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'",
    ] {
        println!("cargo:rustc-link-arg-bins={arg}");
    }
}
