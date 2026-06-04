use std::env;
use std::path::PathBuf;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    
    // Attempt to load cbindgen.toml config, fallback to default if missing
    let config_path = PathBuf::from(&crate_dir).join("cbindgen.toml");
    let config = if config_path.exists() {
        cbindgen::Config::from_file(&config_path).unwrap()
    } else {
        let mut def = cbindgen::Config::default();
        def.language = cbindgen::Language::C;
        def
    };

    // Generate C bindings
    cbindgen::generate_with_config(&crate_dir, config)
        .unwrap()
        .write_to_file(PathBuf::from(&crate_dir).join("libolayer_native.h"));

    // Re-run build script if source files change
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=cbindgen.toml");
}
