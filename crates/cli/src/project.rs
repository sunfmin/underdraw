//! Where a screen's project is: the one place every command finds a
//! screen's project root, its domain (ADR 0020), the components it shares
//! and its tokens (ADR 0021).

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde_json::{Map, Value, json};
use underdraw_format::{Screen, Shared};

/// The file at a project's root holding the domain its screens share
/// (ADR 0020); it marks where the project is.
pub const DOMAIN: &str = "domain.json";
/// The design tokens: found in the nearest directory at or above a screen
/// that has them.
pub const TOKENS: &str = "tokens.json";
/// The directory under a project's root holding the components its
/// screens share, each in `<Name>.json`.
pub const COMPONENTS: &str = "components";

/// The project a screen is in, as every command takes it.
pub struct Project {
    /// The nearest directory at or above the screen holding domain.json;
    /// with none, the screen's own directory.
    pub root: PathBuf,
    /// The tokens.json the screen takes its tokens from, when any
    /// directory at or above it has one.
    pub tokens_file: Option<PathBuf>,
    /// The screen's tokens: that file's, or none.
    pub tokens: Value,
    /// The domain its screens share: domain.json at the root, when there
    /// is one.
    pub domain: Option<Value>,
    /// The components its screens share: each `components/<Name>.json` at
    /// the root, by its name.
    pub components: Map<String, Value>,
}

impl Project {
    /// The project of the screen whose directory is `dir`; refused when
    /// the tokens it takes, or its domain, are not JSON.
    pub fn of(dir: &Path) -> anyhow::Result<Project> {
        let dir = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
        let domain_root = nearest(&dir, DOMAIN);
        let domain = domain_root.as_ref().map(|root| read_json(&root.join(DOMAIN))).transpose()?;
        let root = domain_root.unwrap_or_else(|| dir.clone());
        let components = shared(&root.join(COMPONENTS))?;
        let tokens_file = nearest(&dir, TOKENS).map(|at| at.join(TOKENS));
        let tokens = match &tokens_file {
            Some(file) => read_json(file)?,
            None => json!({}),
        };
        Ok(Project { root, tokens_file, tokens, domain, components })
    }

    /// The description `description` read as a screen of this project, as
    /// every command hands it to the pure crates: with the domain and the
    /// components it shares, when the project has them.
    pub fn screen(&self, description: &Value) -> Screen {
        let components = (!self.components.is_empty()).then_some(&self.components);
        Screen::read(description, Shared { domain: self.domain.as_ref(), components })
    }
}

/// The shared component the description at `dir` shows on its own, when
/// `dir` is `<root>/components/<Name>` beside the project's
/// `components/<Name>.json` and holds no screen.json: its name and that
/// file. Only notes.json is ever kept in `dir`, so it need not exist.
pub fn showcased(dir: &Path) -> Option<(String, PathBuf)> {
    let dir = std::path::absolute(dir).ok()?;
    let components =
        dir.parent().filter(|parent| parent.file_name().is_some_and(|name| name == COMPONENTS))?;
    let root = components.parent()?;
    let name = dir.file_name()?.to_str()?;
    let file = components.join(format!("{name}.json"));
    // The components of a directory under another project's root are not shared.
    let shared = nearest(root, DOMAIN).is_none_or(|at| at == root);
    (shared && file.is_file() && !dir.join(crate::SCREEN).is_file()).then(|| (name.to_string(), file))
}

/// Each component `root` shares, when it is a project's root: the
/// directory of the description that shows it on its own ([`showcased`]),
/// by its name, in name order.
pub fn showcases(root: &Path) -> Vec<(String, PathBuf)> {
    if nearest(root, DOMAIN).is_some_and(|at| at != root) {
        return Vec::new();
    }
    let Ok(components) = shared(&root.join(COMPONENTS)) else {
        return Vec::new();
    };
    components.keys().map(|name| (name.clone(), root.join(COMPONENTS).join(name))).collect()
}

/// Each `<Name>.json` in `dir`, by its name, in name order; none when
/// there is no `dir`. Refused when one is not JSON.
fn shared(dir: &Path) -> anyhow::Result<Map<String, Value>> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") && path.is_file() {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut components = Map::new();
    for file in files {
        if let Some(name) = file.file_stem().and_then(|stem| stem.to_str()) {
            components.insert(name.to_string(), read_json(&file)?);
        }
    }
    Ok(components)
}

pub fn read_json(file: &Path) -> anyhow::Result<Value> {
    let text = std::fs::read_to_string(file).with_context(|| format!("cannot read {}", file.display()))?;
    serde_json::from_str(&text).with_context(|| format!("{} is not JSON", file.display()))
}

/// The nearest directory at or above `dir` that holds the file `name`.
fn nearest(dir: &Path, name: &str) -> Option<PathBuf> {
    dir.ancestors().find(|at| at.join(name).is_file()).map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_and_the_root_are_found_up_the_tree_and_a_screens_own_win() {
        let base = std::env::temp_dir().join(format!("underdraw-project-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let screen = base.join("app/orders/list");
        std::fs::create_dir_all(&screen).unwrap();

        let alone = Project::of(&screen).unwrap();
        assert_eq!(alone.root, screen, "with no domain.json, the screen's own directory");
        assert_eq!((alone.tokens_file, alone.tokens), (None, json!({})));

        std::fs::write(base.join("app").join(DOMAIN), "{}").unwrap();
        std::fs::write(base.join("app").join(TOKENS), r#"{"space": {"4": 16}}"#).unwrap();
        let above = Project::of(&screen).unwrap();
        assert_eq!(above.root, base.join("app"));
        assert_eq!(above.tokens_file, Some(base.join("app").join(TOKENS)));
        assert_eq!(above.tokens, json!({"space": {"4": 16}}));

        std::fs::write(base.join("app/orders").join(TOKENS), r#"{"space": {"4": 20}}"#).unwrap();
        assert_eq!(Project::of(&screen).unwrap().tokens, json!({"space": {"4": 20}}), "the nearest");
        std::fs::write(screen.join(TOKENS), r#"{"space": {"4": 24}}"#).unwrap();
        let own = Project::of(&screen).unwrap();
        assert_eq!(own.tokens, json!({"space": {"4": 24}}), "the screen's own wins");
        assert_eq!(own.root, base.join("app"), "tokens do not move the root");

        std::fs::write(screen.join(TOKENS), "{").unwrap();
        let refused = Project::of(&screen).err().unwrap();
        assert!(format!("{refused:#}").contains("is not JSON"), "{refused:#}");
        std::fs::remove_dir_all(base).unwrap();
    }
}
