fn main() {
    #[cfg(target_os = "windows")]
    {
        let ico_path = std::path::Path::new("../../assets/app-icon.ico");
        println!("cargo:rerun-if-changed=../../assets/app-icon.ico");
        if ico_path.is_file() {
            let mut res = winresource::WindowsResource::new();
            res.set_icon("../../assets/app-icon.ico");
            if let Err(err) = res.compile() {
                panic!("Gagal menyematkan ikon aplikasi Windows: {err}");
            }
        }
    }
}
