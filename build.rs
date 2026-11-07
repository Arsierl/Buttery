fn main() {
    for arg in [
        "/MANIFEST:EMBED",
        "/MANIFESTUAC:level='requireAdministrator'",
        "/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'",
    ] {
        println!("cargo:rustc-link-arg-bins={arg}");
    }
}
