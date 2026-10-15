//! The flow between the screens under one directory, as the gallery draws it
//! when the directory is picked: each screen a box, and each navigate an
//! arrow from the box of the screen it is in to the box of the screen it
//! names. Laid out from the links `check <root>` reports ([`links::links`]),
//! never found again here. Only where each box goes is decided here; the
//! window draws them.

use std::path::{Path, PathBuf};

use crate::links::Link;

/// A box's width, in points.
pub const WIDE: f64 = 180.0;
/// The name over a box's picture.
pub const TITLE: f64 = 24.0;
/// A box's height, its name included, in points.
pub const TALL: f64 = 300.0;
/// The space between one column of boxes and the next: room for the names
/// of the nodes along the arrows.
const ACROSS: f64 = 140.0;
/// The space between one box and the next in a column.
const DOWN: f64 = 40.0;

/// What a box of the map stands for.
#[derive(Clone, Debug, PartialEq)]
pub enum Screen {
    /// A description, by its directory.
    Drawn(PathBuf),
    /// A screen a navigate names that no description under the root
    /// declares, by that name: a screen not drawn yet.
    Undrawn(String),
}

/// One box of the map, and where it is, in the map's points.
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub screen: Screen,
    pub x: f64,
    pub y: f64,
}

/// One navigate drawn: from box `from` to box `to`, by their places in the
/// map, labelled with the node it is on: the navigate it stands for.
#[derive(Clone, Debug, PartialEq)]
pub struct Arrow {
    pub from: usize,
    pub to: usize,
    pub link: Link,
}

/// The screens under one directory and the navigates between them, laid out.
#[derive(Clone, Debug, PartialEq)]
pub struct Map {
    pub places: Vec<Place>,
    pub arrows: Vec<Arrow>,
    /// How wide and tall all of it is, in points.
    pub width: f64,
    pub height: f64,
}

impl Map {
    /// The box at `at`, in the map's points, if any.
    pub fn place_at(&self, at: (f64, f64)) -> Option<&Place> {
        self.places.iter().find(|place| {
            (place.x..=place.x + WIDE).contains(&at.0) && (place.y..=place.y + TALL).contains(&at.1)
        })
    }
}

/// The map of `descriptions`, the directories of the descriptions under the
/// directory picked in the tree's order, and of the navigates `links` holds
/// from them. A navigate to a description outside the directory brings that
/// description in, after the others; a navigate to a screen not drawn yet
/// ends at a box of its own, one for each screen named. Each box is put in
/// the column of the fewest navigates it is from a box no other navigates
/// to, and down each column in the order it came in.
pub fn map(descriptions: &[PathBuf], links: &[Link]) -> Map {
    let mut screens: Vec<Screen> = descriptions.iter().cloned().map(Screen::Drawn).collect();
    let mut arrows = Vec::new();
    for link in links {
        let Some(from) = drawn_at(&screens[..descriptions.len()], &link.description) else { continue };
        let to = match &link.target {
            Some(dir) => Screen::Drawn(dir.clone()),
            None => Screen::Undrawn(link.to.clone()),
        };
        let to = screens.iter().position(|screen| *screen == to).unwrap_or_else(|| {
            screens.push(to);
            screens.len() - 1
        });
        arrows.push(Arrow { from, to, link: link.clone() });
    }
    let columns = columns(screens.len(), &arrows);
    let mut down = vec![0usize; columns.iter().max().map_or(0, |deepest| deepest + 1)];
    let mut places = Vec::new();
    for (screen, column) in screens.into_iter().zip(columns) {
        let row = down[column];
        down[column] += 1;
        places.push(Place { screen, x: column as f64 * (WIDE + ACROSS), y: row as f64 * (TALL + DOWN) });
    }
    let width = places.iter().map(|place| place.x + WIDE).fold(0.0, f64::max);
    let height = places.iter().map(|place| place.y + TALL).fold(0.0, f64::max);
    Map { places, arrows, width, height }
}

/// Where the description in `dir` is among `screens`, if it is.
fn drawn_at(screens: &[Screen], dir: &Path) -> Option<usize> {
    screens.iter().position(|screen| matches!(screen, Screen::Drawn(at) if at == dir))
}

/// The column of each of `count` boxes: how few arrows lead to it from a
/// box no arrow leads to from another. Boxes only a loop reaches start a
/// column of their own, the first of them in order.
fn columns(count: usize, arrows: &[Arrow]) -> Vec<usize> {
    let mut column: Vec<Option<usize>> = vec![None; count];
    let led_to = |index: usize| arrows.iter().any(|arrow| arrow.to == index && arrow.from != index);
    let mut next: Vec<usize> = (0..count).filter(|&index| !led_to(index)).collect();
    loop {
        for &index in &next {
            column[index] = Some(0);
        }
        // Breadth first: each box in the column after the nearest that
        // leads to it.
        let mut at = 0;
        while at < next.len() {
            let from = next[at];
            let depth = column[from].unwrap_or(0);
            for arrow in arrows.iter().filter(|arrow| arrow.from == from) {
                if column[arrow.to].is_none() {
                    column[arrow.to] = Some(depth + 1);
                    next.push(arrow.to);
                }
            }
            at += 1;
        }
        match column.iter().position(Option::is_none) {
            Some(unreached) => next = vec![unreached],
            None => break,
        }
    }
    column.into_iter().map(|column| column.unwrap_or(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(from: &str, node: &str, to: &str, target: Option<&str>) -> Link {
        Link {
            description: PathBuf::from(from),
            from: from.to_string(),
            node: node.to_string(),
            to: to.to_string(),
            target: target.map(PathBuf::from),
        }
    }

    fn dirs(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    /// Each box as (what it is, its column, its row).
    fn boxes(map: &Map) -> Vec<(Screen, usize, usize)> {
        map.places
            .iter()
            .map(|p| {
                (
                    p.screen.clone(),
                    (p.x / (WIDE + ACROSS)).round() as usize,
                    (p.y / (TALL + DOWN)).round() as usize,
                )
            })
            .collect()
    }

    fn drawn(name: &str) -> Screen {
        Screen::Drawn(PathBuf::from(name))
    }

    #[test]
    fn each_navigate_leads_a_column_further() {
        let links = [
            link("list", "row", "Detail", Some("detail")),
            link("list", "add", "Form", None),
            link("detail", "edit", "Form", None),
            link("detail", "back", "List", Some("list")),
        ];
        let map = map(&dirs(&["detail", "list", "other"]), &links);
        assert_eq!(
            boxes(&map),
            vec![
                (drawn("detail"), 0, 0),
                (drawn("list"), 1, 0),
                (drawn("other"), 0, 1),
                (Screen::Undrawn("Form".to_string()), 1, 1),
            ],
            "a loop between detail and list starts at the first of them; other leads nowhere"
        );
        let arrows: Vec<(usize, usize, &str)> =
            map.arrows.iter().map(|a| (a.from, a.to, a.link.node.as_str())).collect();
        assert_eq!(arrows, [(1, 0, "row"), (1, 3, "add"), (0, 3, "edit"), (0, 1, "back")]);
        assert_eq!(map.width, 2.0 * WIDE + ACROSS);
        assert_eq!(map.height, 2.0 * TALL + DOWN);
    }

    #[test]
    fn one_box_for_each_screen_not_drawn_yet_and_none_from_elsewhere() {
        let links = [
            link("a", "one", "Missing", None),
            link("a", "two", "Missing", None),
            link("a", "out", "Outside", Some("elsewhere")),
            link("elsewhere", "in", "A", Some("a")),
        ];
        let map = map(&dirs(&["a"]), &links);
        assert_eq!(
            boxes(&map),
            vec![
                (drawn("a"), 0, 0),
                (Screen::Undrawn("Missing".to_string()), 1, 0),
                (drawn("elsewhere"), 1, 1)
            ],
            "a description outside the directory comes in where a navigate leads to it, but its own navigates do not"
        );
        assert_eq!(map.arrows.len(), 3);
        assert_eq!(map.place_at((WIDE + ACROSS + 1.0, 1.0)).map(|p| &p.screen), Some(&map.places[1].screen));
        assert_eq!(map.place_at((WIDE + 1.0, 1.0)), None, "between the boxes");
    }
}
