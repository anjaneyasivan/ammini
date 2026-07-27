fn main() {
    #[cfg(target_os = "macos")]
    {
        for dir in ["/opt/homebrew/lib", "/usr/local/lib"] {
            if std::path::Path::new(dir).exists() {
                println!("cargo:rustc-link-search={dir}");
                println!("cargo:rustc-link-arg=-Wl,-rpath,{dir}");
                break;
            }
        }
    }
}
