// Per-target layout of the vendored liblore distribution.
//
// `include!`d by `build.rs` (to pick the link/runtime files and rpath policy)
// and by the unit tests in `src/lib.rs` (so the default `cargo test --lib`
// pins the mapping without needing the feature or the shared library) — which
// is also why these are plain comments: `//!` is illegal at an include! site.
// Keep the target → asset mapping in sync with `scripts/fetch-liblore.sh`;
// a unit test cross-checks the two.

pub const LIBLORE_VERSION: &str = "v0.8.3";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibloreLayout {
    /// Upstream release-asset name for this target.
    pub asset: String,
    /// File the linker needs in `vendor/liblore` at build time.
    pub link_file: &'static str,
    /// File the dynamic loader needs at runtime. Differs from `link_file` on
    /// Windows, where you link the import lib but load the DLL.
    pub runtime_file: &'static str,
    /// Bake an rpath to the vendor dir (Mach-O / ELF). Windows has no rpath;
    /// the DLL is copied next to the binaries instead.
    pub uses_rpath: bool,
}

pub fn liblore_layout(target: &str) -> Result<LibloreLayout, String> {
    let (asset_stem, ext) = match target {
        "aarch64-apple-darwin" => ("aarch64-apple-darwin", "tar.gz"),
        "x86_64-unknown-linux-gnu" => ("x86_64-unknown-linux-gnu", "tar.gz"),
        // Upstream's only aarch64 Linux build is the Neoverse-tuned one.
        "aarch64-unknown-linux-gnu" => ("aarch64-unknown-linux-gnu-neoverse-512tvb", "tar.gz"),
        "x86_64-pc-windows-msvc" => ("x86_64-pc-windows-msvc", "zip"),
        "x86_64-apple-darwin" => {
            return Err(
                "upstream publishes no liblore build for Intel macOS — build without \
                 --features liblore to use the CLI backend"
                    .to_string(),
            )
        }
        other => {
            return Err(format!(
                "no liblore distribution known for target {other} — build without \
                 --features liblore to use the CLI backend"
            ))
        }
    };
    let (link_file, runtime_file, uses_rpath) = if target.ends_with("windows-msvc") {
        ("lore.lib", "lore.dll", false)
    } else if target.contains("apple") {
        ("liblore.dylib", "liblore.dylib", true)
    } else {
        ("liblore.so", "liblore.so", true)
    };
    Ok(LibloreLayout {
        asset: format!("liblore-{LIBLORE_VERSION}-{asset_stem}.{ext}"),
        link_file,
        runtime_file,
        uses_rpath,
    })
}
