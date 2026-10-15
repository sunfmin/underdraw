//! The journeys under a root (docs/adr/0022): each `journeys/<name>.json`
//! read, the screens its steps name read from its project, and walked.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde_json::{Value, json};

use crate::gallery::{self, within};
use crate::project::Project;
use crate::read;

/// The directory, in the directory a journey is listed under, that holds
/// its journeys.
pub const JOURNEYS: &str = "journeys";

/// Every journey file under `root`, at any depth, in the tree's order and
/// each directory's in order of name.
pub fn under(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    collect(root, &mut found);
    found
}

fn collect(dir: &Path, found: &mut Vec<PathBuf>) {
    let held = dir.join(JOURNEYS);
    if let Ok(entries) = std::fs::read_dir(&held) {
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "json"))
            .collect();
        files.sort();
        found.extend(files);
    }
    for (name, path) in gallery::children(dir) {
        if name != JOURNEYS {
            collect(&path, found);
        }
    }
}

/// What `check <root>` says of the journey in `file`: where it is, relative
/// to `root`, its name, and every fault in walking it. One that cannot be
/// read has that as its one finding.
pub fn checked(root: &Path, file: &Path) -> Value {
    let name = file.file_stem().map(|stem| stem.to_string_lossy().to_string()).unwrap_or_default();
    let findings = match walked(file) {
        Ok(findings) => serde_json::to_value(findings).unwrap_or_default(),
        Err(e) => json!([{ "at": "", "problem": format!("{e:#}") }]),
    };
    let ok = findings.as_array().is_some_and(Vec::is_empty);
    json!({ "journey": within(root, file), "name": name, "ok": ok, "findings": findings })
}

/// The faults in walking the journey in `file`, its screens named by their
/// path from the project's root: the nearest directory at or above the one
/// it is listed under that holds domain.json, or that directory itself.
fn walked(file: &Path) -> anyhow::Result<Vec<underdraw_format::Finding>> {
    let text = std::fs::read_to_string(file).with_context(|| format!("cannot read {}", file.display()))?;
    let journey: Value = serde_json::from_str(&text).with_context(|| {
        format!("{} is not JSON: fix the line and column named here, then check again", file.display())
    })?;
    let listed = file.parent().and_then(Path::parent).unwrap_or(Path::new("."));
    let root = Project::of(listed)?.root;
    let mut screens = BTreeMap::new();
    for step in journey.get("steps").and_then(Value::as_array).into_iter().flatten() {
        if let Some(path) = step.get("screen").and_then(Value::as_str)
            && !screens.contains_key(path)
            && let Ok((screen, _)) = read(&root.join(path))
        {
            screens.insert(path.to_string(), screen);
        }
    }
    Ok(underdraw_format::walk_journey(&journey, &screens))
}
