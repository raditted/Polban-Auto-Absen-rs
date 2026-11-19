#[cfg(windows)]
fn main() {
    let icon_path = std::path::Path::new("../../assets/app-icon.ico");
    println!("cargo:rerun-if-changed={}", icon_path.display());
    if icon_path.is_file() {
        let mut res = winresource::WindowsResource::new();
        res.set("ProductName", "Polban Auto Absen");
        res.set("FileDescription", "Polban Auto Absen CLI/TUI");
        res.set_icon(icon_path.to_str().expect("valid UTF-8 icon path"));
        if let Err(err) = res.compile() {
            panic!("failed to embed Windows icon resource: {err}");
        }
    }
}

#[cfg(not(windows))]
fn main() {}
