fn main() {
    tauri_build::build();

    #[cfg(feature = "liblore")]
    liblore::generate();
}

/// liblore FFI binding generation + linking. Compiled only under the `liblore`
/// feature so the default build needs neither bindgen nor the shared library.
#[cfg(feature = "liblore")]
mod liblore {
    use std::path::PathBuf;

    pub fn generate() {
        let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
        let vendor = manifest.join("vendor/liblore");
        let header = vendor.join("lore.h");

        // Require the shared library to be present (fetched, not committed).
        let lib = vendor.join("liblore.dylib");
        if !lib.exists() {
            panic!(
                "liblore.dylib not found at {} — run scripts/fetch-liblore.sh first",
                lib.display()
            );
        }

        // Link against liblore (file is liblore.dylib => link name "lore") and
        // bake an rpath so it resolves at runtime in dev.
        println!("cargo:rustc-link-search=native={}", vendor.display());
        println!("cargo:rustc-link-lib=dylib=lore");
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", vendor.display());
        println!("cargo:rerun-if-changed={}", header.display());

        // Generate bindings for the liblore C API.
        let bindings = bindgen::Builder::default()
            .header(header.to_string_lossy())
            .allowlist_function("lore_.*")
            .allowlist_type("lore_.*")
            .allowlist_var("LORE_.*")
            .generate()
            .expect("failed to generate liblore bindings");

        let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
        bindings
            .write_to_file(out.join("liblore_bindings.rs"))
            .expect("failed to write liblore bindings");
    }
}
