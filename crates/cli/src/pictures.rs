//! The states of other descriptions under a project, read and laid out for
//! a node that draws one inside it (the built-in `Wireframe`), as
//! `underdraw_layout` takes them: the one place a description reaches
//! another's files to draw it.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use underdraw_format::Lang;
use underdraw_layout::{Page, Pictures};

use crate::draw::{Description, Shown};
use crate::links;

/// A state of a screen, by the screen's name and the state's (none for
/// its first fixture).
type Wanted = (String, Option<String>);

/// The descriptions under one project's root, drawn on demand, for the
/// description at the end of `drawing`.
pub struct Album {
    root: PathBuf,
    /// The screens being drawn, outermost first, each drawing the next: a
    /// screen among them drawn again would draw itself.
    drawing: Vec<String>,
    /// The language underdraw's own assumptions are said in.
    lang: Lang,
    /// The description declaring each screen under the root, read once
    /// when first needed.
    declared: RefCell<Option<BTreeMap<String, PathBuf>>>,
    /// Each state already drawn, or why it cannot be, by screen and state.
    drawn: RefCell<BTreeMap<Wanted, Result<Page, String>>>,
}

impl Album {
    /// For the description whose project's root is `root`, drawn inside
    /// each of `drawing` in turn and named last in it.
    pub fn new(root: &Path, drawing: Vec<String>, lang: Lang) -> Album {
        Album {
            root: root.to_path_buf(),
            drawing,
            lang,
            declared: RefCell::new(None),
            drawn: RefCell::new(BTreeMap::new()),
        }
    }

    fn draw(&self, screen: &str, state: Option<&str>) -> Result<Page, String> {
        if self.drawing.iter().any(|drawn| drawn == screen) {
            let path = self.drawing.join(" → ");
            return Err(format!(
                "it draws {screen}, which is being drawn already ({path} → {screen}): a description cannot draw itself, at any depth; draw another screen here"
            ));
        }
        let dir = {
            let mut declared = self.declared.borrow_mut();
            let declared = declared.get_or_insert_with(|| links::declared(&self.root));
            declared.get(screen).cloned()
        };
        let Some(dir) = dir else {
            return Err(format!(
                "no description under the project ({}) declares the screen {screen:?}: name one whose \"screen\" is that, or draw it first",
                self.root.display()
            ));
        };
        let description = Description::read_inside(&dir, &self.drawing).map_err(|e| format!("{e:#}"))?;
        if let Some(first) = description.findings().first() {
            return Err(format!(
                "{screen} has {} finding(s), so a picture of it would mislead; the first, at {}: {}",
                description.findings().len(),
                first.at,
                first.problem
            ));
        }
        let states = description.states();
        let shown = match state {
            Some(state) => states.iter().find(|shown| shown.name() == state).cloned().ok_or_else(|| {
                let names: Vec<&str> = states.iter().map(Shown::name).collect();
                format!("{screen} has no state {state:?}; its fixtures and phases are {names:?}")
            })?,
            None => {
                states.iter().find(|shown| matches!(shown, Shown::Fixture(_))).cloned().ok_or_else(|| {
                    format!("{screen} has no fixture to draw: name one of its phases in \"state\"")
                })?
            }
        };
        let wanted = shown.wanted(description.widths()[0], 1);
        let laid = description.lay_out(&wanted, None, self.lang).map_err(|e| format!("{e:#}"))?;
        Ok(laid.page)
    }
}

impl Pictures for Album {
    fn picture(&self, screen: &str, state: Option<&str>) -> Result<Page, String> {
        let key = (screen.to_string(), state.map(str::to_string));
        if let Some(drawn) = self.drawn.borrow().get(&key) {
            return drawn.clone();
        }
        let drawn = self.draw(screen, state);
        self.drawn.borrow_mut().insert(key, drawn.clone());
        drawn
    }
}
