//! What a node means inside an instance of a component (docs/adr/0021),
//! said once for every walk over a description: `check`, instantiation,
//! handoff, and the index of the nodes a description writes. Each walk
//! asks [`Instances::met`] what a node is, keeps its own state where an
//! instance is in `T`, and does its own work on each node; this module
//! does the rest.
//!
//! An instance is its component's root, named by the instance's id; each
//! node inside is named by that id joined to its own (`order-3-cancel`),
//! raw and as written. A `{"slot": …}` node inside stands for what the
//! instance gives there, read where the instance is: the walk takes the
//! instance off with [`Instances::leave`], walks what it gives in the
//! state it kept, and puts it back with [`Instances::resume`]. A component
//! that uses itself is refused rather than walked into.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::subtrees::{self, Subtree, Subtrees};
use crate::template;

/// What a node is, where a walk meets it.
pub enum Met<'c, 'a> {
    /// A node of its own.
    Node,
    /// An instance of `component`: the walk enters it, walks its root (its
    /// `nodes`) and leaves it.
    Instance(&'c Subtree<'a>),
    /// A `{"slot": key}` node: inside an instance, what the instance gives
    /// under `key`; outside any, a fault.
    Slot(String),
}

/// One instance whose nodes are being walked.
pub struct Frame<'a, T> {
    /// Its component's name.
    pub name: &'a str,
    /// The instance's id, and its id as written: what each node inside is
    /// named by, joined to its own.
    pub id: String,
    pub written_id: String,
    /// Each param the instance gives a path whole (`"order": "{item}"`),
    /// with that path, read where the instance is.
    pub paths: BTreeMap<String, String>,
    instance: Map<String, Value>,
    /// What the walk keeps where the instance is: the scope a slot's nodes
    /// are read in, among others.
    pub at: T,
}

impl<T> Frame<'_, T> {
    /// What the instance gives in the slot `key`, if it gives anything.
    pub fn given(&self, key: &str) -> Option<&Value> {
        self.instance.get(key)
    }
}

/// The instances a walk is inside, the outermost first.
pub struct Instances<'c, 'a, T> {
    components: &'c Subtrees<'a>,
    frames: Vec<Frame<'a, T>>,
}

impl<'c, 'a, T> Instances<'c, 'a, T> {
    /// Outside any instance, with the screen's `components`.
    pub fn new(components: &'c Subtrees<'a>) -> Self {
        Instances { components, frames: Vec::new() }
    }

    /// What `node` is: a `{"slot": …}` node, an instance of one of the
    /// screen's components whose body is nodes, or a node of its own.
    pub fn met(&self, node: &Map<String, Value>) -> Met<'c, 'a> {
        if subtrees::is_slot(node) {
            return Met::Slot(node.get("slot").and_then(Value::as_str).unwrap_or_default().to_string());
        }
        match self.components.of(node) {
            Some(component) => Met::Instance(component),
            None => Met::Node,
        }
    }

    /// The id of a node here whose id is `filled` (its template filled, for
    /// a walk that fills them) and `written` as written: each joined to the
    /// innermost instance's, `(id, written_id)`.
    pub fn ids(&self, written: &str, filled: &str) -> (String, String) {
        let inner = self.frames.last();
        (
            join(inner.map(|frame| frame.id.as_str()), filled),
            join(inner.map(|frame| frame.written_id.as_str()), written),
        )
    }

    /// The fault of walking into `component` here, when it uses itself: it
    /// is one of the instances already walked into.
    pub fn uses_itself(&self, component: &Subtree<'a>) -> Option<String> {
        let around = self.frames.iter().position(|frame| frame.name == component.name)?;
        let mut through: Vec<&str> = self.frames.iter().skip(around).map(|frame| frame.name).collect();
        through.push(component.name);
        Some(format!("component {} uses itself: {}", component.name, through.join(" → ")))
    }

    /// Walks into `instance`, an instance of `component` named `id` and
    /// `written_id`, keeping `at` there; refused when the component uses
    /// itself. The walk leaves it once its root is walked.
    pub fn enter(
        &mut self,
        component: &Subtree<'a>,
        instance: &Map<String, Value>,
        id: String,
        written_id: String,
        at: T,
    ) -> Result<(), String> {
        if let Some(fault) = self.uses_itself(component) {
            return Err(fault);
        }
        let mut paths = BTreeMap::new();
        for (param, _) in &component.params {
            let given = instance.get("with").and_then(|with| with.get(param));
            if let Some(path) = given.and_then(Value::as_str).and_then(template::whole) {
                paths.insert(param.clone(), path);
            }
        }
        self.frames.push(Frame {
            name: component.name,
            id,
            written_id,
            paths,
            instance: instance.clone(),
            at,
        });
        Ok(())
    }

    /// Leaves the innermost instance: once its root is walked, or, for a
    /// `{"slot": …}` node inside it, while what it gives is walked where it
    /// is, after which [`resume`](Self::resume) puts it back.
    pub fn leave(&mut self) -> Option<Frame<'a, T>> {
        self.frames.pop()
    }

    /// Goes back into `frame`, taken off by [`leave`](Self::leave).
    pub fn resume(&mut self, frame: Frame<'a, T>) {
        self.frames.push(frame);
    }

    /// The instance the walk is in, if any.
    pub fn innermost(&self) -> Option<&Frame<'a, T>> {
        self.frames.last()
    }

    /// `path`, read inside the innermost instance, as the path it is
    /// outside every instance: a param given a path whole stands for that
    /// path, read in turn where its instance is. Any other path is the
    /// instance's own (`item` of an `each` inside), and stays as it is.
    pub fn through(&self, path: &str) -> String {
        let mut path = path.to_string();
        for frame in self.frames.iter().rev() {
            let (first, rest) = match path.split_once('.') {
                Some((first, rest)) => (first.to_string(), Some(rest.to_string())),
                None => (path.clone(), None),
            };
            let Some(given) = frame.paths.get(&first) else {
                break;
            };
            path = match rest {
                Some(rest) => format!("{given}.{rest}"),
                None => given.clone(),
            };
        }
        path
    }
}

/// The id of a node written `id` inside the instance named `prefix`:
/// `order-3` and `cancel` make `order-3-cancel`. Outside any, its own.
pub fn join(prefix: Option<&str>, id: &str) -> String {
    match prefix {
        Some(prefix) => format!("{prefix}-{id}"),
        None => id.to_string(),
    }
}
