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

    include!("liblore_layout.rs");

    pub fn generate() {
        let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
        let vendor = manifest.join("vendor/liblore");
        let header = vendor.join("lore.h");
        let target = std::env::var("TARGET").unwrap();
        let layout =
            liblore_layout(&target).unwrap_or_else(|e| panic!("--features liblore: {e}"));

        // Require the shared library to be present (fetched, not committed).
        let lib = vendor.join(layout.link_file);
        if !lib.exists() {
            panic!(
                "{} not found at {} — run scripts/fetch-liblore.sh first",
                layout.link_file,
                lib.display()
            );
        }

        // Link against liblore: the link name "lore" resolves per platform to
        // liblore.dylib / liblore.so / lore.lib.
        println!("cargo:rustc-link-search=native={}", vendor.display());
        println!("cargo:rustc-link-lib=dylib=lore");
        if layout.uses_rpath {
            // Bake an rpath so the library resolves at runtime in dev.
            println!("cargo:rustc-link-arg=-Wl,-rpath,{}", vendor.display());
        }
        if layout.runtime_file != layout.link_file {
            // Windows: no rpath — copy lore.dll next to the binaries (the
            // profile dir for the app, deps/ for test executables).
            let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
            let profile = out
                .ancestors()
                .nth(3)
                .expect("OUT_DIR should be target/<profile>/build/<crate>/out")
                .to_path_buf();
            for dir in [profile.clone(), profile.join("deps")] {
                std::fs::create_dir_all(&dir).ok();
                std::fs::copy(vendor.join(layout.runtime_file), dir.join(layout.runtime_file))
                    .unwrap_or_else(|e| panic!("copy {} to {}: {e}", layout.runtime_file, dir.display()));
            }
        }
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
