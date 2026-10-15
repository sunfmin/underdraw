//! The descriptions under one root together: each checked, and each
//! `navigate` resolved to the description that declares the screen it
//! names. A screen none declares is not drawn yet: an assumption, for the
//! person to confirm, not a fault.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde_json::{Value, json};
use underdraw_format::{Lang, ScreenParams};

use crate::gallery::{self, within};
use crate::{SCREEN, notes, read};

/// The screens the descriptions under `root` declare, by name: each one's
/// "screen", read without checking it. One that cannot be read declares
/// none.
pub fn drawn(root: &Path) -> BTreeSet<String> {
    declared(root).into_keys().collect()
}

/// The description under `root` that declares each screen, by the screen's
/// name; where two declare one, the first the tree lists.
pub fn declared(root: &Path) -> BTreeMap<String, PathBuf> {
    let dirs = description_dirs(root);
    let mut declared = BTreeMap::new();
    for (dir, screen) in dirs.iter().zip(screens(&dirs)) {
        if let Some(name) = screen.as_ref().and_then(declares) {
            declared.entry(name).or_insert_with(|| dir.clone());
        }
    }
    declared
}

/// The params each screen under `root` declares, by the screen's name;
/// where two declare one, the first the tree lists. A screen that declares
/// none is not in it.
pub fn params(root: &Path) -> ScreenParams {
    let dirs = description_dirs(root);
    let mut params = ScreenParams::new();
    for screen in screens(&dirs).into_iter().flatten() {
        if let Some(name) = declares(&screen)
            && !params.contains_key(&name)
            && let Some(declared) = underdraw_format::params(&screen)
        {
            params.insert(name, declared);
        }
    }
    params
}

/// `underdraw check <root>`, for a directory holding no screen.json: every
/// description under it checked, and the links between them, what is
/// assumed of one to a screen not drawn yet said in `lang`, and every
/// journey under it walked.
pub fn check_root(root: &Path, lang: Lang) -> anyhow::Result<Value> {
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
    let others = params(&root);
    let others = &others;
    let mut checked: Vec<Option<underdraw_format::Checked>> = dirs.iter().map(|_| None).collect();
    std::thread::scope(|scope| {
        for (dir, found) in dirs.iter().zip(&mut checked) {
            scope.spawn(move || *found = Some(gallery::checked(dir, others)));
        }
    });
    let screens = screens(&dirs);

    let mut ok = true;
    let mut descriptions = Vec::new();
    let mut counts = notes::Counts::default();
    for (dir, checked) in dirs.iter().zip(checked.into_iter().flatten()) {
        ok &= checked.ok;
        let counted = notes::counted(dir, &root);
        let count = |status: &str| counted[status].as_u64().unwrap_or(0) as usize;
        counts = counts.add(notes::Counts { open: count("open"), answered: count("answered") });
        descriptions.push(json!({
            "description": within(&root, dir),
            "screen": checked.screen,
            "ok": checked.ok,
            "fixtures": checked.fixtures,
            "phases": checked.phases,
            "conditions": checked.conditions,
            "findings": checked.findings,
            "notes": counted,
        }));
    }
    let links: Vec<Value> = between(&dirs, &screens)
        .into_iter()
        .map(|link| {
            json!({
                "description": within(&root, &link.description),
                "from": link.from,
                "node": link.node,
                "to": link.to,
                "resolved": link.target.is_some(),
                "assumed": match link.target {
                    Some(_) => Value::Null,
                    None => Value::String(underdraw_format::assumption::undrawn(std::slice::from_ref(&link.to), lang)),
                },
            })
        })
        .collect();
    let journeys: Vec<Value> =
        crate::journeys::under(&root).iter().map(|file| crate::journeys::checked(&root, file)).collect();
    ok &= journeys.iter().all(|journey| journey["ok"] == Value::Bool(true));
    Ok(json!({
        "root": root,
        "ok": ok,
        "descriptions": descriptions,
        "links": links,
        "journeys": journeys,
        "notes": counts,
    }))
}

/// One `navigate` from a description under a root to the screen it names.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    /// The directory of the description the navigate is in.
    pub description: PathBuf,
    /// The screen that description declares.
    pub from: String,
    /// The node the navigate is on, by its id as written.
    pub node: String,
    /// The screen it names.
    pub to: String,
    /// The directory of the description that declares `to`, the first in
    /// the tree's order; none for a screen not drawn yet.
    pub target: Option<PathBuf>,
}

/// Every navigate of the descriptions under `root`, resolved: what `check
/// <root>` reports as its "links", description by description in the
/// tree's order.
pub fn links(root: &Path) -> Vec<Link> {
    let dirs = description_dirs(root);
    between(&dirs, &screens(&dirs))
}

/// The navigates of the descriptions in `dirs`, each read as `screens`
/// holds it, resolved among them.
fn between(dirs: &[PathBuf], screens: &[Option<Value>]) -> Vec<Link> {
    let mut declared: Vec<(String, &PathBuf)> = Vec::new();
    for (dir, screen) in dirs.iter().zip(screens) {
        if let Some(name) = screen.as_ref().and_then(declares) {
            declared.push((name, dir));
        }
    }
    let mut links = Vec::new();
    for (dir, screen) in dirs.iter().zip(screens) {
        let Some(screen) = screen else { continue };
        let from = declares(screen).unwrap_or_default();
        for navigate in underdraw_format::navigates(screen) {
            let target =
                declared.iter().find(|(name, _)| *name == navigate.to).map(|(_, dir)| (*dir).clone());
            links.push(Link {
                description: dir.clone(),
                from: from.clone(),
                node: navigate.node,
                to: navigate.to,
                target,
            });
        }
    }
    links
}

/// The directories of the descriptions under `root`, as the gallery lists
/// them.
fn description_dirs(root: &Path) -> Vec<PathBuf> {
    gallery::descriptions(root)
        .into_iter()
        .filter_map(|row| row.description().map(Path::to_path_buf))
        .collect()
}

/// Each description in `dirs` read, or `None` where it cannot be.
fn screens(dirs: &[PathBuf]) -> Vec<Option<Value>> {
    dirs.iter().map(|dir| read(dir).ok().map(|(screen, _)| screen)).collect()
}

/// The screen a description declares, by its "screen".
fn declares(screen: &Value) -> Option<String> {
    screen.get("screen").and_then(Value::as_str).map(str::to_string)
}
