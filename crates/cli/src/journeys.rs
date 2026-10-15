//! The journeys under a root (docs/adr/0022): each `journeys/<name>.json`
//! read, the screens its steps name read from its project, and walked;
//! and its steps read for the gallery to play.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde_json::{Value, json};
use underdraw_format::Finding;

use crate::gallery::{self, within};
use crate::project::Project;
use crate::read;
use crate::reference::{On, Reference};

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

/// The note on the journey in `file`, as a reference: kept in the
/// notes.json of the directory its `journeys/` is in, pinned to its name.
/// None for a file not in a `journeys/`.
pub fn noted(file: &Path) -> Option<Reference> {
    let journeys = file.parent().filter(|dir| dir.file_name().is_some_and(|name| name == JOURNEYS))?;
    let journey = file.file_stem()?.to_string_lossy().to_string();
    Some(Reference { description: journeys.parent()?.to_path_buf(), on: On::Journey { journey } })
}

/// The journey files listed under `dir`, in its `journeys/`, in order of
/// name.
pub fn listed(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir.join(JOURNEYS)) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    files
}

fn collect(dir: &Path, found: &mut Vec<PathBuf>) {
    found.extend(listed(dir));
    for (name, path) in gallery::children(dir) {
        if name != JOURNEYS {
            collect(&path, found);
        }
    }
}

/// What `check <root>` says of the journey in `file`: where it is, relative
/// to `root`, its name, and every fault in walking it.
pub fn checked(root: &Path, file: &Path) -> Value {
    let name = file.file_stem().map(|stem| stem.to_string_lossy().to_string()).unwrap_or_default();
    let findings = faults(file);
    let ok = findings.is_empty();
    json!({ "journey": within(root, file), "name": name, "ok": ok, "findings": findings })
}

/// Every fault in walking the journey in `file`, its screens named by
/// their path from its project's root; one that cannot be read has that as
/// its one finding.
pub fn faults(file: &Path) -> Vec<Finding> {
    walked(file).unwrap_or_else(|e| vec![Finding { at: String::new(), problem: format!("{e:#}") }])
}

/// One step of a journey, as the gallery plays it: the description of its
/// screen, its state's name, and the act on it as written, if any.
pub struct Step {
    pub dir: PathBuf,
    pub state: String,
    /// The node acted on, by its id as written, and its event.
    pub on: Option<(String, String)>,
    pub outcome: Option<String>,
}

/// The steps of the journey in `file`, in order, each screen's description
/// found under its project's root. Read as written: whether it walks is
/// [`faults`]'s to say.
pub fn steps(file: &Path) -> anyhow::Result<Vec<Step>> {
    let (journey, root) = read_journey(file)?;
    let text = |step: &Value, key: &str| step.get(key).and_then(Value::as_str).map(str::to_string);
    let steps = journey.get("steps").and_then(Value::as_array).into_iter().flatten();
    Ok(steps
        .map(|step| Step {
            dir: root.join(text(step, "screen").unwrap_or_default()),
            state: text(step, "state").unwrap_or_default(),
            on: text(step, "on").and_then(|on| {
                let (node, event) = on.rsplit_once('.')?;
                Some((node.to_string(), event.to_string()))
            }),
            outcome: text(step, "outcome"),
        })
        .collect())
}

/// The journey in `file`, and the root of the project its screens are
/// named from: the nearest directory at or above the one it is listed
/// under that holds domain.json, or that directory itself.
fn read_journey(file: &Path) -> anyhow::Result<(Value, PathBuf)> {
    let text = std::fs::read_to_string(file).with_context(|| format!("cannot read {}", file.display()))?;
    let journey: Value = serde_json::from_str(&text).with_context(|| {
        format!("{} is not JSON: fix the line and column named here, then check again", file.display())
    })?;
    let listed = file.parent().and_then(Path::parent).unwrap_or(Path::new("."));
    Ok((journey, Project::of(listed)?.root))
}

/// The faults in walking the journey in `file`.
fn walked(file: &Path) -> anyhow::Result<Vec<Finding>> {
    let (journey, root) = read_journey(file)?;
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
