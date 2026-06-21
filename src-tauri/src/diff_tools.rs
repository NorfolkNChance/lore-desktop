//! Visual diff-tool integration hooks.
//!
//! Phase 4: a registry that bridges the client to native visual diff tools for
//! binary assets (blueprints, materials, meshes) — the things a line-based diff
//! can't show. Tools are detected per-platform and launched with two file
//! versions. The registry is the extension point: adding a tool is one entry.

use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;

/// How a tool wants its two file arguments laid out.
#[derive(Clone, Copy)]
enum ArgStyle {
    /// `tool <left> <right>` (FileMerge, p4merge, Beyond Compare).
    Pair,
    /// `tool --diff <left> <right>` (VS Code).
    DiffFlag,
}

struct ToolSpec {
    id: &'static str,
    name: &'static str,
    /// Binary names to look for on `PATH`.
    bins: &'static [&'static str],
    /// Absolute candidate locations (app bundles etc.) checked first.
    candidates: &'static [&'static str],
    arg_style: ArgStyle,
}

/// The known tools. Cross-platform: PATH lookup covers Windows/Linux installs;
/// `candidates` adds macOS app-bundle paths.
const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        id: "filemerge",
        name: "FileMerge (opendiff)",
        bins: &["opendiff"],
        candidates: &["/usr/bin/opendiff"],
        arg_style: ArgStyle::Pair,
    },
    ToolSpec {
        id: "p4merge",
        name: "P4Merge",
        bins: &["p4merge"],
        candidates: &[
            "/opt/homebrew/bin/p4merge",
            "/usr/local/bin/p4merge",
            "/Applications/p4merge.app/Contents/MacOS/p4merge",
        ],
        arg_style: ArgStyle::Pair,
    },
    ToolSpec {
        id: "bcompare",
        name: "Beyond Compare",
        bins: &["bcomp", "bcompare"],
        candidates: &[
            "/usr/local/bin/bcomp",
            "/Applications/Beyond Compare.app/Contents/MacOS/bcomp",
        ],
        arg_style: ArgStyle::Pair,
    },
    ToolSpec {
        id: "vscode",
        name: "Visual Studio Code",
        bins: &["code"],
        candidates: &[
            "/opt/homebrew/bin/code",
            "/usr/local/bin/code",
        ],
        arg_style: ArgStyle::DiffFlag,
    },
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffToolInfo {
    pub id: String,
    pub name: String,
    pub available: bool,
    pub path: Option<String>,
}

/// Resolve a tool's executable: absolute candidates first, then `PATH`.
fn resolve(spec: &ToolSpec) -> Option<PathBuf> {
    for c in spec.candidates {
        let p = PathBuf::from(c);
        if p.exists() {
            return Some(p);
        }
    }
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for bin in spec.bins {
            let cand = dir.join(bin);
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

/// All known tools with availability — the UI uses this to populate a picker.
pub fn list() -> Vec<DiffToolInfo> {
    TOOLS
        .iter()
        .map(|spec| {
            let resolved = resolve(spec);
            DiffToolInfo {
                id: spec.id.into(),
                name: spec.name.into(),
                available: resolved.is_some(),
                path: resolved.map(|p| p.display().to_string()),
            }
        })
        .collect()
}

/// The first available tool (preferred order = registry order).
pub fn default_tool() -> Option<DiffToolInfo> {
    list().into_iter().find(|t| t.available)
}

/// Launch `tool_id` (or the default) on two file paths. Fire-and-forget: the
/// GUI tool detaches and we don't block on it.
pub fn launch(tool_id: Option<&str>, left: &str, right: &str) -> Result<DiffToolInfo, String> {
    let spec = match tool_id {
        Some(id) => TOOLS.iter().find(|t| t.id == id),
        None => TOOLS.iter().find(|t| resolve(t).is_some()),
    }
    .ok_or_else(|| "no matching diff tool".to_string())?;

    let exe = resolve(spec).ok_or_else(|| format!("{} is not installed", spec.name))?;

    let mut cmd = Command::new(&exe);
    match spec.arg_style {
        ArgStyle::Pair => {
            cmd.arg(left).arg(right);
        }
        ArgStyle::DiffFlag => {
            cmd.arg("--diff").arg(left).arg(right);
        }
    }
    cmd.spawn().map_err(|e| format!("failed to launch {}: {e}", spec.name))?;

    Ok(DiffToolInfo {
        id: spec.id.into(),
        name: spec.name.into(),
        available: true,
        path: Some(exe.display().to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_lists_known_tools() {
        let tools = list();
        // The hook system knows about all registered tools regardless of which
        // are installed on this machine.
        for id in ["filemerge", "p4merge", "bcompare", "vscode"] {
            assert!(tools.iter().any(|t| t.id == id), "missing {id}");
        }
        // Availability is consistent with a resolved path.
        for t in &tools {
            assert_eq!(t.available, t.path.is_some());
        }
    }

    #[test]
    fn launching_unknown_tool_errors() {
        let err = launch(Some("does-not-exist"), "/tmp/a", "/tmp/b").unwrap_err();
        assert!(err.contains("no matching diff tool"));
    }
}
