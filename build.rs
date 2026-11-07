fn main() {
    for arg in [
        "/MANIFEST:EMBED",
        "/MANIFESTUAC:level='requireAdministrator'",
    ] {
        println!("cargo:rustc-link-arg-bins={arg}");
    }
}
