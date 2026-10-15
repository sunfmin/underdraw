//! The descriptions under one root together: each checked, and each
//! `navigate` resolved to the description that declares the screen it
//! names. A screen none declares is not drawn yet: an assumption, for the
//! person to confirm, not a fault.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde_json::{Value, json};

use crate::gallery::{self, within};
use crate::{SCREEN, read};

/// The screens the descriptions under `root` declare, by name: each one's
/// "screen", read without checking it. One that cannot be read declares
/// none.
pub fn drawn(root: &Path) -> BTreeSet<String> {
    let dirs = description_dirs(root);
    screens(&dirs).iter().flatten().filter_map(declares).collect()
}

/// `underdraw check <root>`, for a directory holding no screen.json: every
/// description under it checked, and the links between them.
pub fn check_root(root: &Path) -> anyhow::Result<Value> {
    if !root.is_dir() {
        bail!("cannot read {}: pass a screen's directory, or a directory of them", root.display());
    }
    let root = root.canonicalize().with_context(|| format!("cannot read {}", root.display()))?;
    let dirs = description_dirs(&root);
    if dirs.is_empty() {
        bail!(
            "there is no screen description under {}: a description is a directory holding {SCREEN}",
            root.display()
        );
    }
    let mut checked: Vec<Option<underdraw_format::Checked>> = dirs.iter().map(|_| None).collect();
    std::thread::scope(|scope| {
        for (dir, found) in dirs.iter().zip(&mut checked) {
            scope.spawn(move || *found = Some(gallery::checked(dir)));
        }
    });
    let screens = screens(&dirs);
    let drawn: BTreeSet<String> = screens.iter().flatten().filter_map(declares).collect();

    let mut ok = true;
    let mut descriptions = Vec::new();
    for (dir, checked) in dirs.iter().zip(checked.into_iter().flatten()) {
        ok &= checked.ok;
        descriptions.push(json!({
            "description": within(&root, dir),
            "screen": checked.screen,
            "ok": checked.ok,
            "fixtures": checked.fixtures,
            "phases": checked.phases,
            "conditions": checked.conditions,
            "findings": checked.findings,
        }));
    }
    let mut links = Vec::new();
    for (dir, screen) in dirs.iter().zip(&screens) {
        let Some(screen) = screen else { continue };
        let from = declares(screen).unwrap_or_default();
        for navigate in underdraw_format::navigates(screen) {
            links.push(json!({
                "description": within(&root, dir),
                "from": from,
                "node": navigate.node,
                "to": navigate.to,
                "resolved": drawn.contains(&navigate.to),
            }));
        }
    }
    Ok(json!({ "root": root, "ok": ok, "descriptions": descriptions, "links": links }))
}

/// The directories of the descriptions under `root`, as the gallery lists
/// them.
fn description_dirs(root: &Path) -> Vec<PathBuf> {
    gallery::descriptions(root).into_iter().filter_map(|row| row.description).collect()
}

/// Each description in `dirs` read, or `None` where it cannot be.
fn screens(dirs: &[PathBuf]) -> Vec<Option<Value>> {
    dirs.iter().map(|dir| read(dir).ok().map(|(screen, _)| screen)).collect()
}

/// The screen a description declares, by its "screen".
fn declares(screen: &Value) -> Option<String> {
    screen.get("screen").and_then(Value::as_str).map(str::to_string)
}
