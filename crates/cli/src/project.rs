//! Where a screen's project is: the one place every command finds a
//! screen's project root and its tokens (ADR 0021).

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde_json::{Value, json};

/// The file at a project's root holding the domain its screens share
/// (ADR 0020); it marks where the project is.
pub const DOMAIN: &str = "domain.json";
/// The design tokens: found in the nearest directory at or above a screen
/// that has them.
pub const TOKENS: &str = "tokens.json";

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
}

impl Project {
    /// The project of the screen whose directory is `dir`; refused when
    /// the tokens it takes are not JSON.
    pub fn of(dir: &Path) -> anyhow::Result<Project> {
        let dir = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
        let root = nearest(&dir, DOMAIN).unwrap_or_else(|| dir.clone());
        let tokens_file = nearest(&dir, TOKENS).map(|at| at.join(TOKENS));
        let tokens = match &tokens_file {
            Some(file) => {
                let text = std::fs::read_to_string(file)
                    .with_context(|| format!("cannot read {}", file.display()))?;
                serde_json::from_str(&text).with_context(|| format!("{} is not JSON", file.display()))?
            }
            None => json!({}),
        };
        Ok(Project { root, tokens_file, tokens })
    }
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
