//! The command line and its manual: clap's command tree, each command's text
//! its doc comment, and the topics, the pieces of the manual that belong to
//! no single command, each in a file of its own under help/. `help()`
//! assembles them into the one Markdown manual `--help` answers.

use std::ffi::OsString;
use std::ops::RangeInclusive;
use std::path::PathBuf;

use agent_help::{Help, Topic};
use clap::builder::{PossibleValuesParser, TypedValueParser};
use clap::{CommandFactory, Parser, Subcommand};
use underdraw_format::Lang;
use underdraw_layout::{DEFAULT_WIDTHS, WIDTH_RANGE};

use crate::{FAILED, FAULTS};

/// The pixels per point `render --scale` takes.
const SCALES: RangeInclusive<u32> = 1..=4;

/// underdraw — describe a screen once, as JSON, then check it, show it to a person, and hand it off to web, iOS and Android.
///
/// A screen description is a directory holding screen.json (the screen: its view model, fixtures, conditions, node tree and its own components), usually beside platforms/*.json. How to write each file, every component and every rule: [the format](help:index).
///
/// A screen is in a project: one app's screens under one directory, sharing one design language and one domain. Every command finds it the same way:
///
/// - project root: the nearest directory at or above the screen holding domain.json; with none, the screen's own directory
/// - domain: domain.json at the project root: the types, conditions and operations every screen of the project uses by name, read by every command as if each screen declared them; a screen may add its own, never one of a name the domain declares ([domain-json](help:domain-json))
/// - components: components/<Name>.json at the project root: each a component every screen of the project uses by its name, as if it declared it in its own "components"; a screen may add its own, never one of a name the project shares. Its "fixtures", each a value of its params, show it on its own: the description components/<Name>, which check and the gallery take as they take a screen's directory ([components](help:components))
/// - tokens: tokens.json in the nearest directory at or above the screen that has one, so a screen's own tokens.json wins over one above it, and screens with none beside them share the one above; with none anywhere, no tokens
///
/// `underdraw render` answers which root and tokens.json it took.
///
/// **Needs**
///
/// System fonts to check a description; to render one or open the gallery, also a GPU (Metal on macOS, Vulkan or DirectX 12 elsewhere): `underdraw doctor` says. The gallery also needs a display.
///
/// **Start here**
///
/// - `underdraw check <dir>`: read the description and name every fault
/// - `underdraw check <root>`: every description under <root>, and where each navigate goes: one to a screen none of them declares is an assumption, a screen not drawn yet, not a fault; and every journey under it (<dir>/journeys/<name>.json) walked
/// - `underdraw gallery <root>`: then show it to the person: a window over every description under <root>, drawn as it now stands
/// - `underdraw notes <root>`: what the person said there: each note waiting on you, a conversation pinned to a node, a state or an arrow; act on it, then answer it with `underdraw notes <root> --answer play/N1 --reply "…"`, the note as the listing names it ("note"); `underdraw notes <root> --wait` in the background answers when the person next says something
/// - `underdraw handoff <dir> --platform ios`: once agreed: what to build, and what to test it against
/// - `underdraw compare <dir> --platform ios --capture c.json`: once built: how the running UI differs from it
/// - `underdraw cli <dir>`: the same operations as a command-line interface, with the golden output of each fixture
///
/// **What a command answers**
///
/// One JSON object on stdout. The exit code says which answer it is:
///
/// - 0: the answer, and nothing is wrong
/// - {const:FAULTS}: the answer names faults ("ok": false); it is printed in full
/// - {const:FAILED}: the command could not do its work: {"error": "…"}, saying what fixes it
///
/// So a gate can run `underdraw check <dir>` and stop on any non-zero exit. Exceptions: -h, --help, `underdraw help` and --version print text.
///
/// **Rules to know first**
///
/// - Strings only interpolate: {path} or {path|format}. Every decision is a named condition; a choice between values is "cases", ending in "else".
/// - No truthiness: a date or a number is not a condition; write != null, > 0.
/// - Every condition must be true in one fixture and false in another, so every state can be shown. `check` reports one that is not.
/// - A condition shared by the domain is declared on a type ("Order.cancellable") and used through a value of it ("if": "item.cancellable"); one over a screen's data stays the screen's.
/// - Loading and error are "phases", trees that read no data.
#[derive(Parser)]
#[command(name = "underdraw", version = crate::VERSION, disable_help_subcommand = true, verbatim_doc_comment)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Show one topic of this manual, or all of it
    ///
    /// `underdraw help` prints the whole manual, as `underdraw --help` does; `underdraw help <topic>` prints one topic, such as `underdraw help components`. A topic is a piece of the manual that belongs to no single command: what each part of a description is and how to write it, listed by what you want to do in [the index](help:index). It prints Markdown, not JSON; a topic that does not exist is an error naming those that do.
    #[command(next_help_heading = "Run by any agent", verbatim_doc_comment)]
    Help {
        /// The topic, such as `components`; not given, the whole manual
        #[arg(verbatim_doc_comment)]
        topic: Option<String>,
    },
    /// Check a screen description, or every one under a directory: every fault, and which fixtures each condition holds in
    ///
    /// **When**
    ///
    /// After writing or changing a screen description, and before rendering it or handing it off. Run it until it answers "ok": true.
    ///
    /// **Reads**
    ///
    /// <path>/screen.json, or <path> when it is a file, with the domain.json of its project when there is one ([domain-json](help:domain-json)) and the components the project shares, each components/<Name>.json at its root: every node's component must be one of the screen's own "components", the project's or underdraw's, and the screen's own and the project's are checked too: each has a look or nodes, a look reads only the params it declares, none uses itself, and none of the screen's own has a name the project shares, which shadows the project's ([components](help:components), [looks](help:looks)). A kinds.json beside it is a finding: its components belong in "components". Once the description has no other fault, its tokens.json too (the nearest at or above the screen, as `underdraw --help` says): every fixture and phase is laid out at each of the widths its tokens.json names ("widths", [tokens-json](help:tokens-json); 390 and 1120 when it names none), in the system's fonts as `render` does, and an overlay that runs off the page is a finding, as are "widths" that are not a list of whole numbers from 200 to 4000. Writes nothing.
    ///
    /// <path> may be <project root>/components/<Name>, the description that shows a shared component on its own: its fixtures, each held to its params ([components](help:components)), are its states.
    ///
    /// <path> may instead be a directory holding no screen.json, a root: then every description under it, at any depth, as the gallery finds them, is checked, and also the links between them. Each "navigate" names a screen, and resolves to the description under the root whose "screen" is that name. A navigate to a screen no description declares is not a fault: it is an assumption, a screen not drawn yet, for the person to confirm; `render --root` and the gallery draw its node with the orange "?". A navigate to a screen that is drawn and declares "params" gives each of them, of its type, in "with", and nothing else: a finding on the node that navigates, at "<node>.on.<event>.with" or ".with.<param>" ([params](help:params)). One description checked alone cannot see the others, so it says nothing of where it navigates, nor of what it gives.
    ///
    /// Every journey under the root, <any dir>/journeys/<name>.json, is walked too: each step's screen, by its name or its path, and its state are there, each act's node is shown in its step's state, and the act leads to the next step, by a transition or a navigate ([journeys](help:journeys)), and a navigate's "with", computed in its step's state, gives exactly the params the next step's fixture holds. A journey that does not walk is a finding on the journey, at the step that breaks.
    ///
    /// **Answers**
    ///
    /// ```
    /// {
    ///   "screen":     the screen's name, from "screen"
    ///   "ok":         true when there is no finding
    ///   "fixtures":   the fixtures' names, in the order written
    ///   "phases":     the phases' names (trees rendered before data arrives)
    ///   "conditions": [{"name", "label", "holds_in": [fixture, …]}, …]
    ///                 only those the description reads: each of its own over
    ///                 its data, and each on a type it reads through a value
    ///                 (or that one it reads uses); a domain condition it never
    ///                 reads is left out. Empty while any finding stands, except a condition that
    ///                 never varies: then the table shows which fixtures to add
    ///   "findings":   [{"at": where — a dotted path or a node id,
    ///                   "problem": what is wrong, and what is allowed instead}, …]
    ///   "notes":      {"waiting", "answered"}: how many of the person's notes
    ///                 on it (notes.json, written in the gallery) wait on the
    ///                 agent, the person having spoken last, for it to act on
    ///                 and answer, and how many the agent answered last; with
    ///                 "error" when notes.json cannot be read. Never a finding,
    ///                 but for a notes.json in the old form, from before a note
    ///                 became a conversation ("status", "thread"): that is a
    ///                 finding at "notes.json", saying to delete it, for old
    ///                 notes are not converted. `underdraw notes <dir>` reads them
    /// }
    /// ```
    ///
    /// Given a root:
    ///
    /// ```
    /// {
    ///   "root":         the directory checked
    ///   "ok":           true when no description and no journey has a
    ///                   finding; an unresolved navigate does not count
    ///   "descriptions": [{"description": its directory, relative to root,
    ///                     then everything above: "screen", "ok", "fixtures",
    ///                     "phases", "conditions", "findings", "notes"}, …]
    ///                   one that cannot be read has its one finding at screen.json
    ///   "links":        [{"description": the directory it is in,
    ///                     "from": the screen it is on, "node": the node's id,
    ///                     "to": the screen it names,
    ///                     "resolved": true when a description under root
    ///                       declares it; false: an assumption, a screen not
    ///                       drawn yet,
    ///                     "assumed": when not resolved, that assumption said
    ///                       in a whole sentence, in --lang ({const:LANGS}; en
    ///                       by default), or null}, …]
    ///   "journeys":     [{"journey": its file, relative to root,
    ///                     "name": its file's name without .json,
    ///                     "ok": true when it walks,
    ///                     "findings": [{"at": "steps.<n>.<key>", "problem"}, …]
    ///                     }, …]
    ///   "directories":  [{"directory": its path, relative to root ("." for root),
    ///                     "notes": {"waiting", "answered"}}, …]: each directory
    ///                   holding no description that keeps notes on the screens
    ///                   under it or on its journeys, in its own notes.json
    ///   "notes":        {"waiting", "answered"}, over every description and
    ///                   every such directory
    /// }
    /// ```
    ///
    /// **Exit**
    ///
    /// 0 when "ok" is true, {const:FAULTS} when there are findings (the answer above is still printed in full), {const:FAILED} when the description could not be read or parsed, or a root holds no description. Unresolved navigates alone exit 0, and so do notes, waiting or answered.
    ///
    /// **What trips you up**
    ///
    /// - Only a node a person acts on or answers about needs an "id" written: one with "on", "bind" or "assumed", or an instance of a component holding one. Leave it out of a text, a divider, a row of layout: it is given one from the node it is in, its component and the first words of what it shows ("page-text-total"), as [nodes](help:nodes) says, and a finding, the picture and a note name it so. Changing what it shows renames it, and a note on it then joins its state's note: write an id to keep a node's name through such edits.
    /// - "assumed" says what was guessed and what else it could be, in a sentence in the language you speak with the person: a word or two is refused ([assumed](help:assumed)).
    /// - A text property (text, title, subtitle, label, detail) always shows text: "{booking.total}" there needs |money. Elsewhere a lone placeholder passes the value itself.
    /// - A nullable value may be shown only under a condition that tests it != null in its top-level && chain. A negated condition guarantees nothing.
    /// - A fixture may leave out only fields with a default; T? defaults to null.
    /// - "$patch" starts a fixture from one written above it.
    /// - An operation's result holds no view state: a type with a "view" field (or one reaching such a type) cannot be returned. Wrap the domain type. Nor can one reaching a field "derived": {"app": …}: the app adds those in the wrapper, beside view state.
    /// - "derived" says who: {"app": how} or {"server": how}; a bare string is refused.
    /// - A field with "from" is of its query's result type, or wraps it in a type whose one other-than-view, other-than-app-derived field is it.
    /// - A parameter whose type is an enum takes only its members: its default, and every "with" for it, a path of that enum or a member literal.
    /// - An action that changes domain data is a "call": set and toggle write only view state (a field marked "view": true, or inside one), and every other action is UI-only (navigate, back, open, close, scrollTo, share, copy). submit, reload, remove and the like are operations.
    /// - A "call" to an operation with "requires" needs the node gated by that condition: "disabled": "!<condition>", or "if": "<condition>".
    /// - A "call" gives each parameter a value whose range lies inside the parameter's: "{code}" of "string ..20" passes "string 1..20" only under a condition testing code != "" ("disabled": "!hasCode").
    /// - A query with "pages": "cursor" answers with a page type holding one list (with its most, "Doc[] ..50") and "next": "string?". Every call to it gives "cursor": null for the first page, or "{next}" — the field filled "from": "<query>.next" — under a condition that tests it != null. Do not write cursor or limit in its params: paging adds them.
    /// - A "call" to a "destructive" operation is made only inside its confirmation: an overlay whose component has a backdrop and that a node opens with {"open": "<its id>"}. The menu item or button a person first acts on opens it instead ([operations](help:operations)).
    /// - A "call" to a bulk form (an operation's "bulk": its name) gives its list, "ids", the selection: one path to a list in view state at least one long ("string[] 1..") inside a nullable field, and the node is gated by a condition testing that field != null ("disabled": "!hasSelection"; or an "if" around it). Not its operation's "requires".
    /// - A "long" command's job type (ExportDocsJob) and the query that reads it (exportDocsJob) are derived: write neither. A field of "data" holds the job, and fixtures hold it with each "status": running, done, failed.
    /// - A component of the screen's ("components", [components](help:components)) reads only its params: a path past them, or a condition of the screen's, is refused; pass it in a param ("{hasDates}" for a bool one). Its "with" gives each param of its type, and nothing else; it declares no view state; its nodes are named by the instance's id joined to theirs (order-{index}-cancel), and a transition names them so. "ref" is gone.
    /// - An icon is a Material Symbols name (arrow_back, favorite; .fill for the solid form), in a node, a fixture or a look: fonts.google.com/icons.
    #[command(next_help_heading = "Run by the agent that writes descriptions", verbatim_doc_comment)]
    Check {
        /// The screen's directory, which holds screen.json, or the screen.json itself; or a directory holding no screen.json, to check every description under it and the navigates between them
        #[arg(verbatim_doc_comment)]
        path: PathBuf,
        /// The language underdraw words its own assumptions in, a screen not drawn yet among the links
        #[arg(long, default_value = "en", value_parser = lang(), verbatim_doc_comment)]
        lang: Lang,
    },
    /// Draw a screen in one state as a wireframe PNG, or every state on one sheet, and say where each node went
    ///
    /// **When**
    ///
    /// After `underdraw check` answers "ok": true, to read the node boxes and know where each part went, at each of the widths the project draws at: those its tokens.json names ("widths"), or a phone width ({const:PHONE_WIDTH}) and a desktop width ({const:DESKTOP_WIDTH}); with --all, every state at each of them at once. What a person looks at is the gallery (`underdraw gallery`), not these PNGs.
    ///
    /// **Reads**
    ///
    /// <path>/screen.json, its tokens.json (spaces, radii, fonts, breakpoints; the nearest at or above <path>, as `underdraw --help` says); each node is drawn as the screen's own "components" or underdraw's say. Refuses a description `check` finds faults in. With --root, also the "screen" of every description under that directory, as `check <root>` reads them: a node that navigates to a screen none of them declares is assumed, a screen not drawn yet. Without it, navigates are not resolved.
    ///
    /// **Writes**
    ///
    /// One PNG: --out, or <dir>/renders/<fixture or phase>-<width>.png. Running it again overwrites the same file.
    ///
    /// With --all, one sheet instead: --out, or <dir>/renders/all.png. A row for each state, the fixtures then the phases in the order written (as `check` lists them), each drawn at each of the project's widths side by side, in the order its tokens.json names them, each page captioned with its state and width ("default · 390"). --all takes no --fixture, --phase or --width.
    ///
    /// **Answers**
    ///
    /// ```
    /// {
    ///   "png":        the file written
    ///   "project":    the screen's project root: the nearest directory at or
    ///                 above it holding domain.json, or its own directory
    ///   "tokens":     the tokens.json it took, or null when there is none
    ///   "state":      {"fixture": name} or {"phase": name}
    ///   "breakpoint": "compact" or "regular", chosen by tokens' breakpoints and --width
    ///   "width", "height": the page in points; the page is as tall as its content
    ///   "scale":      pixels per point (lowered when the page is taller than the GPU draws)
    ///   "pixels":     [width, height] of the PNG
    ///   "nodes":      [{"id", "component", "box": {"x", "y", "width", "height"} in points,
    ///                   "label": the number tagged on it in the picture, or null,
    ///                   "assumed": what the agent guessed there, or that its component is
    ///                     not one underdraw knows, or (given --root) that it
    ///                     navigates to a screen not drawn yet, or null; what
    ///                     underdraw assumes itself is a whole sentence in
    ///                     --lang, {const:LANGS}, en by default}, …]
    /// }
    /// ```
    ///
    /// **Answers, with --all**
    ///
    /// ```
    /// {
    ///   "png", "project", "tokens", "scale", "pixels": as above, for the sheet
    ///   "width", "height": the sheet in points
    ///   "states": [{"state", "breakpoint", "width", "height", "nodes": each as
    ///                 one render of that state at that width answers it, its
    ///                 node boxes in that page's own points,
    ///               "at": {"x", "y"}, where the page's top left sits on the
    ///                 sheet, in points}, …], each state at each width in turn
    /// }
    /// ```
    ///
    /// A node's place on the sheet is "at" plus its box, times "scale" in pixels. A long sheet is drawn at a lower scale, so its pages are smaller than one render's; render the one state to read it closely.
    ///
    /// **What you see**
    ///
    /// A wireframe, in grays: boxes, text in the system font, crossed boxes for images. A red numbered circle tags everything a person can act on, in document order; the same number is "label" in nodes, so "3" in a conversation means one node. An orange dashed outline and "?" mark what was assumed, for the person to confirm, and a component underdraw does not know, drawn as a box with its name. The "?" sits at the node's top-right corner, always wholly on the page; on a node as big as the page (a full-screen overlay) it sits at the top right of what the node shows. A number sits left of its node's top. Neither covers another node's text: where the node beside it has words there, as in a tight row of buttons, the tag moves elsewhere by its node (off the corner, above, below, or to the right), so a word cut short in the picture is cut short in the layout too.
    ///
    /// What a dialog (an opaque box pinned over the page) or its backdrop covers gets no number and no "?": its "label" is null, and its "assumed" still says why it was guessed. Icons are Material Symbols, drawn outlined, or solid for a name ending in .fill.
    ///
    /// **What trips you up**
    ///
    /// - Needs a GPU (Metal on macOS); `underdraw doctor` says whether there is one.
    /// - The page shows everything at once: a sticky bar is drawn at the end of the page, not over the content, and a node's "foot" on the bottom edge of its column, after all it holds.
    #[command(next_help_heading = "Run by the agent that writes descriptions", verbatim_doc_comment)]
    Render {
        /// The screen's directory, which holds screen.json (and tokens.json), or the screen.json itself
        #[arg(verbatim_doc_comment)]
        path: PathBuf,
        /// The fixture whose data to show; the first fixture when neither this nor --phase is given
        #[arg(long, conflicts_with = "phase", verbatim_doc_comment)]
        fixture: Option<String>,
        /// A phase to show instead of data, such as loading
        #[arg(long, verbatim_doc_comment)]
        phase: Option<String>,
        /// The page width in points: 390 is a phone, 1120 a laptop browser
        ///
        /// From {const:WIDTH_LEAST} to {const:WIDTH_MOST}. Not given, the first of the widths its tokens.json names ("widths"), or {const:PHONE_WIDTH}.
        #[arg(long, value_parser = clap::value_parser!(u32).range(i64::from(*WIDTH_RANGE.start())..=i64::from(*WIDTH_RANGE.end())), verbatim_doc_comment)]
        width: Option<u32>,
        /// Every fixture and phase, each at every width its tokens.json names, captioned, in one PNG
        ///
        /// With no widths named, at {const:DEFAULT_WIDTHS}.
        #[arg(long, conflicts_with_all = ["fixture", "phase", "width"], verbatim_doc_comment)]
        all: bool,
        /// Pixels per point
        ///
        /// From {const:SCALE_LEAST} to {const:SCALE_MOST}.
        #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u32).range(i64::from(*SCALES.start())..=i64::from(*SCALES.end())), verbatim_doc_comment)]
        scale: u32,
        /// Where to write the PNG; <dir>/renders/<fixture or phase>-<width>.png when not given, or with --all <dir>/renders/all.png
        #[arg(long, verbatim_doc_comment)]
        out: Option<PathBuf>,
        /// The directory the screen descriptions are under, as `check <root>` and the gallery take it: a node that navigates to a screen none of them declares is marked assumed. Not given, navigates are not resolved
        #[arg(long, verbatim_doc_comment)]
        root: Option<PathBuf>,
        /// The language underdraw words its own assumptions in, a screen not drawn yet or a component it does not know
        #[arg(long, default_value = "en", value_parser = lang(), verbatim_doc_comment)]
        lang: Lang,
    },
    /// A window for the person: every screen description under a directory, each drawn at a phone and a desktop width
    ///
    /// **When**
    ///
    /// Whenever the person should see a design: after `underdraw check` answers "ok": true, and again after each round of edits. The gallery is what a person looks at; `render` is for the agent reading node boxes.
    ///
    /// **Reads**
    ///
    /// Every screen description under <root>: each directory, at any depth, that holds screen.json. The tree leaves out renders/ and platforms/, hidden directories, and directories leading to no description. A project's root lists the components it shares in a section of their own, components/, each a description (components/<Name>) whose states are the fixtures in its components/<Name>.json: one instance of it, drawn in each, picked, noted and copied as a screen's nodes are, its notes kept in components/<Name>/notes.json ([components](help:components)). Nothing is read from renders/: the window draws from the descriptions themselves, from the notes.json beside each, the person's notes, and from each journey under <root> (<dir>/journeys/<name>.json).
    ///
    /// **Does**
    ///
    /// Opens a window in a process of its own and exits at once; only the window keeps running, until the person closes it. There is one window per root: while it is open, running this again brings it forward instead of opening a second, so run it after every round of edits.
    ///
    /// What the window shows and what the person does in it: [the gallery window](help:gallery-window).
    ///
    /// **Answers**
    ///
    /// ```
    /// {
    ///   "root":         the directory searched
    ///   "descriptions": the directories holding a screen.json, relative to root,
    ///                   in the order the tree lists them
    ///   "faults":       those of them `underdraw check` finds faults in, in the
    ///                   same order; [] when every one can be drawn. Run
    ///                   `underdraw check <root>/<each>` to read and fix them
    ///   "opened":       true: a window was opened; false: the window over this
    ///                   root was already open, and is brought forward
    ///   "pid":          the window's process
    /// }
    /// ```
    ///
    /// Exits {const:FAILED} with {"error"} when there is no description under <root>, or no display to open a window on (over SSH, in CI): there, `underdraw render` draws a state to a PNG instead.
    ///
    /// **What trips you up**
    ///
    /// - Needs a display and a GPU; checked on macOS only, so far.
    /// - The window is the root's once the path is resolved: `drafts` and `./drafts/` are one root; `drafts` and `drafts/todo` are two windows.
    #[command(next_help_heading = "Run by the agent that writes descriptions", verbatim_doc_comment)]
    Gallery {
        /// The directory the screen descriptions are under, at any depth
        #[arg(verbatim_doc_comment)]
        root: PathBuf,
        /// The language the window says everything of its own in: its sidebar, its marks and its title, and underdraw's own assumptions, a screen not drawn yet or a component it does not know
        #[arg(long, default_value = "en", value_parser = lang(), verbatim_doc_comment)]
        lang: Lang,
        /// Run the window itself, in this process: what the command starts
        #[arg(long, hide = true, verbatim_doc_comment)]
        window: bool,
    },
    /// The person's notes on the descriptions under a directory that wait on the agent, each a conversation with where it is pinned; or one answered
    ///
    /// **When**
    ///
    /// Whenever the person may have said something in the gallery: before each round of edits, and after `underdraw check` answers "notes" with any waiting. Each note waiting on you is something the person asked for: act on it, then answer it. And between rounds, `--wait` in the background, to learn the moment the person says something (**Waiting**, below).
    ///
    /// The person starts the notes, in the gallery (`underdraw gallery --help`); the agent reads them and answers in them.
    ///
    /// **Reads**
    ///
    /// notes.json beside each screen.json under <path>: <path> itself, when it is a description's directory, or every description under it at any depth, as the gallery finds them; and the notes.json of each directory under it, <path> itself included, that holds no description: its notes on the screens under it, as a group, and on the journeys in its journeys/. A description with no notes.json has no notes. A shared component shown on its own, the description components/<Name> under its project's root, keeps them in components/<Name>/notes.json; what they are on is edited in components/<Name>.json, its fixture under "fixtures". Writes nothing, but with --answer.
    ///
    /// **Does**
    ///
    /// A note is a conversation between the person and the agent about a design, pinned where it is held, with the same fields ⌘C copies: one node of one state they clicked, as the description writes it, with the instance they pointed at as context; a whole state; a transition, as `transitions` in screen.json writes it; a navigate, as `underdraw check <root>` reports it among its "links", kept by the description it is written in; or the whole screen, the description as a whole, apart from any one state ("this screen tries to do too much"); or the screens under a directory, as a group, kept in that directory's own notes.json though it holds no screen.json ("these screens repeat the same header"); or a whole journey, the path through all its steps, kept by name in the notes.json of the directory holding its journeys/ ("cancelling takes four steps; too many"). Each is numbered N1, N2… within its description or directory, and holds its messages in the order said, each by the person or the agent, and some of the person's marked as more than talk (**Marks**, below). Only the person starts one. It is never closed: who spoke last says where it stands, read from its messages:
    ///
    /// - `waiting`: the person spoke last, and `--wait` has not handed it to the agent yet: for the agent to act on and answer
    /// - `taken`: the person spoke last, and `--wait` handed it to the agent (its "taken"): it still waits on the agent, and the gallery counts how long ago it was taken
    /// - `unread`: the agent spoke last, and the person has not seen it yet
    /// - `read`: the agent spoke last, and the person has seen it: they may say more in it
    ///
    /// `underdraw notes <path>` lists the waiting ones, each with its whole conversation. Edit what it is on in that description's screen.json: the node where "node.at" points (the id written there is "node.id", a template inside a repeat); the state's fixture or phase; the entry of `transitions` the "transition" names; or the navigate on the node "navigate.node"; for the whole screen, wherever in the description it takes; for the screens under a directory, in each description under it that it takes; for a journey, in <dir>/journeys/<"journey">.json or in the screens its steps go through; and do what each marked message asks (**Marks**, below). Then answer it by what the listing names it, its "note", from the same <path> it was listed under:
    ///
    /// ```sh
    /// underdraw notes <path> --answer play/N3 --reply "made the price wrap"
    /// ```
    ///
    /// "note" is <"description">/<"id">, or "id" alone for a note of <path>'s own ("description": "."): copy it as listed, nothing to join. From a description's own directory its notes are listed, and answered, by their id alone:
    ///
    /// ```sh
    /// underdraw notes <path>/play --answer N3 --reply "made the price wrap"
    /// ```
    ///
    /// The reply is at any length, as many lines as it takes, kept as written but for blank space at its ends: say what was done, or why not, or ask back. It is added to the conversation as the agent's message, unread, and the note is answered: its pin turns green in the window until the person reads it. Answering again with the same text as the agent's last message changes nothing more; with other text, it is one more message.
    ///
    /// A note whose node, state or arrow is gone from the description (removed or renamed) joins its state's note the next time notes are read, so nothing said is lost: its messages go into the note on the whole state it was in (a transition's: the state it is from), after a line by "underdraw" saying where they were ("on `price`, now gone"), and before what the person said there since the agent last spoke. A state with no note gets the gone note itself, with the same id; when the state is gone too, or for a navigate, the description's first fixture's note takes them. Nothing moves while the description has findings, and reading again moves nothing twice. The joined note waits on the agent while the person spoke last, like any other.
    ///
    /// ```sh
    /// underdraw notes <path> --answer play/N3 --reply "Made the price wrap.
    /// Should the date wrap too, or be cut?"
    /// ```
    ///
    /// **Marks**
    ///
    /// A message of the person's with neither key below is plain talk. One with a mark has, beside "by" and "says", exactly one of:
    ///
    /// - `"answers"`: the person's answer to the assumption on the note's node (**Assumptions**, below); "says" is what is right for a ✗, or a fixed line for a ✓ with nothing more to say
    /// - `"proposes"`: where acting on the note's node should go, chosen where nothing was declared: {"on": "<node id as written>.<event>", "to": <state>}, a transition to add to "transitions" from the note's "state", or {"on", "navigate": <screen>}, a navigate to give the node's handler. Declare it; "says" is the same in words
    ///
    /// The agent's messages and underdraw's are never marked. A note may hold several marked messages, each answered by the agent's next message as any other; the marks stay in the conversation after it is answered.
    ///
    /// **Assumptions**
    ///
    /// The person answers an assumption with a message in its node's note, its "answers" saying which and how: "assumed", the guess as the gallery showed it; "guess", what made it: "author" (the node's own `assumed`), {"undrawn": [<screen>, …]} (a navigate to a screen not drawn yet) or {"component": <component>} (a component underdraw does not know); and "answer":
    ///
    /// - `right`: ✓: the guess was right. Make it a decision: take the guess out of the description (remove the node's `assumed`; draw the screen; define the component), then answer the note saying so.
    /// - `wrong`: ✗: the guess was wrong, and the message says what is right. Change the description to that, then answer the note, as any note.
    ///
    /// **Waiting**
    ///
    /// `underdraw notes <root> --wait` blocks until a note under <root> waits on the agent, the person having spoken last in it, then answers as `underdraw notes <root>` does, every waiting note listed, and exits 0. Each message it hands over is marked taken in notes.json, with the time, so the gallery tells the person "the agent is changing it" and how long ago; a message already taken keeps its time. A note taken still waits, and is listed again until it is answered. While it blocks, the toolbar of a gallery over <root>, or a directory under it, says "Agent listening"; within two seconds of its ending, "No agent listening".
    ///
    /// When one waits already it answers at once; when none does, it reads the files again every fraction of a second, and answers as soon as the person starts a note, or says more in one, in the gallery. A message of the agent's wakes nothing. It keeps nothing between runs: kill it and run it again, and no note is lost, for what waits is read from the files.
    ///
    /// Keep it running in the background, as a command the agent's harness tells it of when it exits (in Claude Code, Bash with run_in_background). When it answers: act on each note it lists, answer every one with --answer (a reply saying why not, for one you will not act on), then start --wait again. A note left waiting makes the next --wait answer at once.
    ///
    /// ```sh
    /// underdraw notes <root> --wait
    /// ```
    ///
    /// It waits as long as it takes; to stop waiting, kill it.
    ///
    /// The agent cannot start a note: `--create` is refused, saying the person does that in the gallery. Nothing closes or reopens a note: there is no --close, --reopen or --closed. To ask the person something unprompted, write an assumption (`assumed`) in the description: the gallery shows it, for them to answer; to ask back about what they said, answer in their note.
    ///
    /// **Answers**
    ///
    /// ```
    /// {
    ///   "root":     <path>, resolved
    ///   "notes":    the notes waiting on the agent, description by description
    ///               (and directory by directory) in the tree's
    ///               order, each in the order written:
    ///               [{"note":        what --answer takes for it, from this
    ///                                same <path>: "play/N3", or "N3" for
    ///                                <path>'s own,
    ///                 "description": its directory, relative to root ("." for
    ///                                root itself),
    ///                 "id":          "N3",
    ///                 "stands":      "waiting", "taken", "unread" or "read",
    ///                                read from who spoke last (DOES, above),
    ///                 what it is on, as one of these:
    ///                 "state":       {"fixture": <name>} or {"phase": <name>},
    ///                                the state it was written in, with
    ///                 "node":        {"id": the id as written, "at": the JSON
    ///                                Pointer where it is written in
    ///                                screen.json, "instance": the element of
    ///                                each repeat around it, outermost first,
    ///                                "label": its label or null, "width": the
    ///                                width it was drawn at}; or
    ///                 "state" alone: on the whole state; or
    ///                 "transition":  {"from", "on": "<node id>.<event>",
    ///                                "outcome" (for a call only), "to"}; or
    ///                 "navigate":    {"from": the screen it is in, "node": the
    ///                                id as written, "to": the screen it names};
    ///                                or
    ///                 "whole":       "screen": on the whole screen, apart from
    ///                                any one state; or "screens": on the
    ///                                screens under the directory
    ///                                "description" names, as a group; or
    ///                 "journey":     its name: on the whole journey in
    ///                                <"description">/journeys/<name>.json,
    ///                 "messages":    [{"by": "person" or "agent", "says"}, …],
    ///                                the conversation, in the order said, the
    ///                                person's first; "says" as many lines as
    ///                                were written; a person's message handed
    ///                                over by --wait has "taken": when, in
    ///                                seconds since 1970; an agent's message
    ///                                the person has not seen has "unread":
    ///                                true; "by": "underdraw" is the
    ///                                line saying where the messages after it
    ///                                were, in a note now gone, and is neither
    ///                                person's nor agent's; a message of the
    ///                                person's may also hold "answers" or
    ///                                "proposes" (MARKS, above)
    ///                 "reference":   the same as one line, as ⌘C copies it}, …]
    ///   "waiting":  how many notes wait on the agent, taken or not
    ///   "answered": how many the agent spoke last in, read or not
    /// }
    /// ```
    ///
    /// With --answer:
    ///
    /// ```
    /// {
    ///   "note":    the note answered, as above, named from <path>
    ///   "changed": false when the agent's last message already said that
    /// }
    /// ```
    ///
    /// With --wait: the same as without it, once a note waits.
    ///
    /// Exits {const:FAILED} with {"error"} when <path> is not a directory, --wait is given with --answer, the reply is empty, notes.json is not notes as the gallery writes them or is in the old form (`underdraw check` says to delete it), or --answer names no note under <path>.
    ///
    /// **What trips you up**
    ///
    /// - --answer takes the note's "note", not its "id": ids are numbered within each description and each directory, so two can each have an N1, and "note" (play/N1, orders/N1) tells them apart. Give it with the same <path> the listing was run on.
    /// - --wait answers at once while any note waits on you: answer every note it lists before you start it again, or it never waits.
    /// - Quote a reply of many lines as one argument; the shell keeps its line breaks.
    /// - notes.json is the person's: commit it with the description, and never edit it by hand.
    /// - A note is not a fault: `underdraw check` counts the waiting and answered ("notes": {"waiting", "answered"}) and exits as it would without them. A notes.json in the old form, written before a note became a conversation, is a fault: delete it.
    #[command(next_help_heading = "Run by the agent that writes descriptions", verbatim_doc_comment)]
    Notes {
        /// A description's directory, or a directory of them at any depth
        #[arg(verbatim_doc_comment)]
        path: PathBuf,
        /// The note to answer, as `notes <path>` lists it ("note"): <description>/<id> under <path> (play/N3), or <id> alone for <path>'s own (N3)
        #[arg(long, value_name = "NOTE", requires = "reply", verbatim_doc_comment)]
        answer: Option<String>,
        /// The answer: what was done, or why not, or what you ask back; as many lines as it takes
        #[arg(long, value_name = "TEXT", verbatim_doc_comment)]
        reply: Option<String>,
        /// Block until a note waits on the agent (the person spoke last), then answer as without it: at once when one waits already
        #[arg(long, verbatim_doc_comment)]
        wait: bool,
        /// Refused: the person starts notes, in the gallery
        #[arg(long, hide = true, alias = "add", num_args = 0..=1, default_missing_value = "", verbatim_doc_comment)]
        create: Option<String>,
    },
    /// The view model as a JSON Schema, to validate API responses and other data against
    ///
    /// **When**
    ///
    /// When data the screen will show must be validated outside underdraw: an API's responses in its contract tests, fixtures or mock data written by hand, a JSON editor. Run it again whenever the view model changes; never edit the schema by hand.
    ///
    /// **Reads**
    ///
    /// <path>/screen.json. Writes nothing. Refuses a description `check` finds faults in.
    ///
    /// **Answers**
    ///
    /// ```
    /// {
    ///   "screen": the screen's name
    ///   "schema": a JSON Schema (draft 2020-12) of the root model, the object
    ///             a fixture's "data" holds: each root field under "properties",
    ///             each named type and "money" under "$defs". Save it as is,
    ///             e.g. `underdraw schema <dir> | jq .schema > model.schema.json`.
    /// }
    /// ```
    ///
    /// **How each type becomes schema**
    ///
    /// - `string, image, icon`: "type": "string"; a range bounds the length
    /// - `url`: "type": "string", "format": "uri"
    /// - `date`: a string matching YYYY-MM-DD
    /// - `int, number`: "integer" / "number"; a range bounds the value
    /// - `bool`: "boolean"
    /// - `money`: {"$ref": "#/$defs/money"}: {"amount", "currency" of 3}
    /// - `Name`: {"$ref": "#/$defs/Name"}: an object or an enum
    /// - `T[]`: an array of T; a range bounds the count
    /// - `T?`: T or null
    ///
    /// A field without a default is required; no object takes a field it does not declare. "doc" and "derived" (and who derives it) become "description"; a default becomes "default".
    ///
    /// **What trips you up**
    ///
    /// - The schema checks what `check` checks of a fixture's data, except that an icon is a known Material Symbols name: that, only `check` knows.
    /// - Validators treat "format": "uri" as a note unless told to assert it.
    /// - A field with a default may be left out, as in a fixture; data an API sends that must always carry it needs no default in the description.
    #[command(
        next_help_heading = "Run by the agent that builds the real UI, or the API that feeds it",
        verbatim_doc_comment
    )]
    Schema {
        /// The screen's directory, which holds screen.json, or the screen.json itself
        #[arg(verbatim_doc_comment)]
        path: PathBuf,
    },
    /// Everything an agent needs to build the screen on one platform: model code, tokens, nodes, states to test
    ///
    /// **When**
    ///
    /// Once the person has agreed to the description (`render`ed, discussed, assumptions confirmed), before building the screen on one platform. Run it once per platform.
    ///
    /// **Reads**
    ///
    /// <path>/screen.json, with its project's domain.json, its tokens.json (the nearest at or above <path>, as `underdraw --help` says), and platforms/<platform>.json: the platform's mapping of each component and action, its test-id form, and its token naming ("tokenNaming": {"pattern": "Theme.{group}.{name}"}). Writes nothing. Refuses a description `check` finds faults in.
    ///
    /// **Answers**
    ///
    /// ```
    /// {
    ///   "screen", "platform", "language"
    ///   "ok":         true when the mapping covers every component and action used
    ///   "findings":   [{"at", "problem"}, …]: what the mapping is missing
    ///   "rules":      the platform's conventions: stack, testId, breakpoints, elementTree
    ///   "code":       {"file", "text"}: the view model's types and every condition,
    ///                 generated in the platform's language. Save it as is.
    ///   "conditions": [{"name", "label", "holds_in": [fixture, …]}, …]
    ///   "operations": [{"name", "does": "query" or "command", "doc", "params":
    ///                   {name: type, with " = default" when it has one}, "result",
    ///                   "requires", "fails": {name: when}, "fills": [path, …],
    ///                   "pages": "cursor" for a query that pages}, …]
    ///                 the queries and commands the screen calls. "fills": the
    ///                 fields of "data" a query's result lands in, "docs" or,
    ///                 through a wrapper type, "docs[].doc"; one field of the
    ///                 result lands as "result.next → next": map the answer there,
    ///                 and derive the rest of the view model in the app. A paged
    ///                 query's params include cursor and limit; a call with a
    ///                 cursor appends the page's rows, one with null replaces them.
    ///   "tokens":     [{"token": "color.primary", "name": the platform's name, "value"}, …]
    ///   "lengths":    [{"component", "at", "length": "{depth} * space.4 + 12",
    ///                   "params": {"depth": "int 0..4"}, "tokens": ["space.4"]}, …]
    ///                 each length a component's look computes (left out when
    ///                 none does): build the expression from the instance's
    ///                 params and the tokens by their names above, never a
    ///                 number per state
    ///   "nodes":      [{"id", "component", "parent", "slot",
    ///                   "implement": how the platform builds this component,
    ///                   "test_id": the test id as the platform writes it,
    ///                   "props": what the description says, as written,
    ///                   "shown": its if / only / each / limit,
    ///                   "events": [{"on", "action", "operation", "with", "implement"}, …],
    ///                     "operation": the one it calls, when the action is "call"
    ///                   "why", "assumed"}, …] in document order
    ///   "hidden":     [{"id", "why"}, …]: nodes this platform does not build
    ///   "states":     [{"state": {"fixture"} or {"phase"}, "breakpoint",
    ///                   "present": [id, …],
    ///                   "shows": {id: {"text", "label", "value", "disabled", …}},
    ///                   "calls": {id: [{"on", "operation", "with"}, …]},
    ///                   "expressions": [{"node", "at", "expression", "branch",
    ///                                    "value"}, …]}, …]
    ///                 every fixture and phase at both breakpoints: what the real
    ///                 UI must show, to assert in its tests by test id; "calls" is
    ///                 the operation each node calls when acted on, with its
    ///                 arguments filled for that state (left out when none does).
    ///                 "expressions": every expression as that state decides it,
    ///                 in the order the tree is built, the values render drew
    ///                 from:
    ///                   {"at": "conditions.hasDates", "expression": "hasDates",
    ///                    "value": true}                 a condition over the data
    ///                                                   (a fixture only)
    ///                   {"node": "save", "at": "disabled", "expression":
    ///                    "!canSave", "value": true}     a use site: a node's "if"
    ///                                                   (a node hidden by it too),
    ///                                                   a bool prop, "with.<param>"
    ///                   {"node": "note-0", "at": "text.cases[0].if", …}
    ///                                                   a cases branch tested
    ///                   {"node": "note-0", "at": "text.cases", "expression":
    ///                    "item.done" or "else", "branch": 0, "value": "Eggs ✓"}
    ///                                                   the branch chosen, its
    ///                                                   value filled
    ///                 "node" is the test id's id; inside a component, the
    ///                 instance's, and the names are its params. A node this
    ///                 platform does not build ("hidden") is left out
    ///   "fixtures":   {name: data}: each fixture merged and defaulted, ready to
    ///                 decode into the generated model for previews and tests
    ///   "unconfirmed": [{"id", "assumed"}, …]: guesses the person has not confirmed
    /// }
    /// ```
    ///
    /// **How to use it**
    ///
    /// - Save "code" as "code.file" and use its types and conditions; do not rewrite a condition by hand.
    /// - Build each node with "implement", give it "test_id", and wire each event with its "implement"; an event with "operation" calls that operation, given "with", through the app's data layer.
    /// - Write one preview or test per state: load its fixture from "fixtures", and assert every "present" id and every value in "shows". Act on each node in "calls" and assert the app calls that operation with "with" (`underdraw compare` checks both, from a capture).
    /// - "expressions" is the test oracle for the code behind the views: write one unit test per fixture that decodes its data into the generated model and asserts each value. A condition is the generated function of its name; a use site, a bool param and a chosen branch are what the view decides at that node: the same value, or the platform build reads the description differently from the reference, whatever its pictures look like.
    /// - Ask the person about each "unconfirmed" item before building it.
    ///
    /// **What trips you up**
    ///
    /// - Texts in "props" are templates; "shows" has them filled for each state, formatted as the description says (money ¥104,300, date 2026/11/12).
    /// - A node with "each" stands for one node per item; its test id contains {index}, and inside a repeat inside one, the outer's index by the name its "index" gives it ({row}).
    #[command(
        next_help_heading = "Run by the agent that builds the real UI, or the API that feeds it",
        verbatim_doc_comment
    )]
    Handoff {
        /// The screen's directory, which holds screen.json, tokens.json and platforms/<platform>.json
        #[arg(verbatim_doc_comment)]
        path: PathBuf,
        /// The platform to build for: the name of a file in <dir>/platforms, such as web, ios or android
        #[arg(long, verbatim_doc_comment)]
        platform: String,
    },
    /// How a running UI or CLI differs from the description in one state: a capture of it against what that state shows or prints
    ///
    /// **When**
    ///
    /// After building a state of the screen on a platform, and after every change to it: is the real UI what the person agreed to? Once per state you can put the UI in (each fixture, each phase), at the width you ran it at. Or, for a CLI derived with `underdraw cli`: once per command run in a fixture, is what it printed the golden output? (**A CLI's capture**, below.)
    ///
    /// **Reads**
    ///
    /// <path>/screen.json, its tokens.json (the nearest at or above <path>, as `underdraw --help` says), platforms/<platform>.json, and the capture file. Writes nothing. Refuses a description `check` finds faults in. A capture holding "exit" is a CLI's, and needs no --platform.
    ///
    /// **The capture**
    ///
    /// The UI's elements that have a test id, read by the platform's UI test tool ("elementTree" in the platform file says which) and written as:
    ///
    /// ```
    /// {
    ///   "width": 390,                     the width the UI ran at, in points
    ///                                     (CSS pixels on the web)
    ///   "elements": [
    ///     { "id": "reserve-button",       the test id, which is the node's id
    ///       "x": 16, "y": 780, "width": 358, "height": 48,
    ///                                     the frame, all four or none
    ///       "text": "预订",               the words it shows, if the tool reads them
    ///       "enabled": true },            whether it can be used, if the tool reads it
    ///     …
    ///   ],
    ///   "acted": [                        optional: each node the test acted on
    ///     { "id": "confirm-delete-confirm", its test id
    ///       "on": "tap",                  the event, as in handoff's "events";
    ///                                     "tap" when left out
    ///       "calls": [                    every operation call the app made in
    ///                                     answer, [] for none
    ///         { "operation": "deleteDoc", the operation's name
    ///           "with": {"id": "c"} } ] } its arguments by parameter name; {} for none
    ///     …
    ///   ]
    /// }
    /// ```
    ///
    /// Every frame in one coordinate space: the page's, with its top-left at 0,0. A check that needs something the capture leaves out is skipped, not failed. Record the calls at the app's data layer (a fake of the operations that logs each call), act on one node at a time, and reset the state between.
    ///
    /// **Answers**
    ///
    /// ```
    /// {
    ///   "screen", "platform"
    ///   "ok":         true when nothing differs
    ///   "state":      {"fixture"} or {"phase"}
    ///   "breakpoint": the breakpoint "width" falls in; "width"
    ///   "checked":    {"present", "absent", "text", "enabled", "order", "inside",
    ///                  "calls"}:
    ///                 how many of each check were made
    ///   "findings":   [{"at": id, "check", "problem"}, …], where check is
    ///     present     a node the state shows has no element
    ///     duplicate   two elements have one node's id
    ///     absent      an element the state does not show, or the platform hides
    ///     text        the words differ (one value: exactly; several: each is in it)
    ///     enabled     enabled where the description says disabled, or the reverse
    ///     order       a node is not below, or right of, the sibling before it,
    ///                 where the wireframe puts it
    ///     inside      a node's frame is outside the nearest element holding it
    ///     call-missing  acting on the node did not call an operation the
    ///                 description says that event calls (handoff's "calls")
    ///     call-extra  acting on the node called an operation the description
    ///                 does not say it calls
    ///     call-with   it called the right operation with other arguments
    ///   "unknown":    [id, …]: ids no node of the description has
    /// }
    /// ```
    ///
    /// Exit {const:FAULTS} when there is a finding.
    ///
    /// **A CLI's capture**
    ///
    /// One command run, in one fixture (--fixture; the first when not given):
    ///
    /// ```
    /// {
    ///   "command": ["delete-doc", "b", "--yes"],  the command line after the CLI's name
    ///   "exit": 1,                        the code it exited with
    ///   "stdout": "{\"error\": …}",       what it printed, as text (or the
    ///                                     JSON value it parsed to)
    ///   "failure": "notFound"             optional: the named failure the test
    ///                                     made the fake server fail with; then
    ///                                     the fixture and arguments do not matter,
    ///                                     but for a bulk form's (below)
    /// }
    /// ```
    ///
    /// The command line is read as `underdraw cli` says the command takes it, defaults filled in, and matched to the golden output of that command, in that fixture, given the same parameters (and, for a destructive command, with --yes or without it, and for a long one, with --no-wait or without it, as the run was). A capture with "failure" is of that named failure; a bulk form's, of its partial failure: the fixture and the ids must be the golden output's too. Exit 2 when it is not one the derived CLI takes, or has no golden output (the error lists the ones there are).
    ///
    /// **Answers**
    ///
    /// ```
    /// {
    ///   "screen", "command"
    ///   "ok":       true when it exited with the golden code and printed the
    ///               golden stdout
    ///   "state":    {"fixture"} or {"failure"}
    ///   "golden":   the golden output compared with
    ///   "checked":  {"exit", "stdout"}: how many values were compared
    ///   "findings": [{"at", "check", "problem"}, …], where check is
    ///     exit      it exited with another code; at is "exit"
    ///     stdout    a value differs, is missing or is extra; at is its JSON
    ///               path, as stdout.result[2].title
    /// }
    /// ```
    ///
    /// Exit {const:FAULTS} when there is a finding.
    ///
    /// **How to use it**
    ///
    /// - In a UI test: load a fixture from `underdraw handoff`'s "fixtures", show the screen, dump its elements into a capture, run compare with that --fixture, and fail the test on exit 1, printing "findings".
    /// - To check what the UI calls: in the same state, act on each node that handoff's state lists under "calls" (and any other you wire to the data layer), record what the app called, and put it in "acted". A node acted on that is not in "acted" is not checked.
    /// - In a CLI's test: give the CLI a fake server holding a fixture's data (`underdraw cli`'s "fixtures", which needs no platform file), run each command `underdraw cli` lists a golden output for, write each run as a capture, and fail on exit 1.
    /// - Fix the UI, not the capture. When the UI is right and the description is not, change the description, `render` it, and show the person.
    ///
    /// **What trips you up**
    ///
    /// - Lazy containers (LazyVStack, LazyColumn, virtualised lists) build only what is on screen: what is not built is "present" as missing. Capture with the content built, or scroll and capture the parts into one page.
    /// - On the web, getBoundingClientRect is relative to the viewport: add scrollX and scrollY so the frames are the page's.
    /// - Order and nesting are checked; sizes and spacing are not: the wireframe's are not the real UI's.
    /// - Pinned bars (StickyBar), a node's "foot" and overlays are not checked for order, and nothing is checked for nesting inside a node that scrolls.
    #[command(
        next_help_heading = "Run by the agent that builds the real UI or the CLI, from its tests",
        verbatim_doc_comment
    )]
    Compare {
        /// The screen's directory, which holds screen.json, tokens.json and platforms/<platform>.json
        #[arg(verbatim_doc_comment)]
        path: PathBuf,
        /// The platform the UI runs on: the name of a file in <dir>/platforms; not given for a CLI's capture
        #[arg(long, verbatim_doc_comment)]
        platform: Option<String>,
        /// The capture: a JSON file of the UI's elements, or of a CLI's run, written as `underdraw compare --help` says
        #[arg(long, verbatim_doc_comment)]
        capture: PathBuf,
        /// The fixture whose data the UI showed, or the CLI answered from; the first fixture when neither this nor --phase is given
        #[arg(long, conflicts_with = "phase", verbatim_doc_comment)]
        fixture: Option<String>,
        /// The phase the UI was in instead, such as loading
        #[arg(long, verbatim_doc_comment)]
        phase: Option<String>,
    },
    /// The command-line interface the screen's operations derive: commands, stdout schemas, failures, help drafts, golden outputs
    ///
    /// **When**
    ///
    /// Once the screen's operations are agreed, when the same queries and commands are also wanted on the command line: the screen and its CLI come from one description, so they cannot drift apart. Run it again after every change to the operations or the fixtures.
    ///
    /// **Reads**
    ///
    /// <path>/screen.json: its "operations", "types", "conditions", "fixtures" and each query's "from", with those its project's domain.json declares, as if the screen declared them (`underdraw --help`). Writes nothing. Refuses a description `check` finds faults in.
    ///
    /// **How each operation becomes a command**
    ///
    /// - name: the operation's whole name in kebab case, so it says what it acts on: listDocs is `list-docs`, deleteDoc is `delete-doc`, setQuantity is `set-quantity`.
    /// - arguments: each parameter, in the order written. One without a default is positional (`delete-doc <id>`); one with a default, a nullable one or a list is a flag (`--sort <updated|…>`, a list's flag given once per value). A parameter of an enum takes one of its choices, and a default carries over: "sort": "SortKey = \"updated\"" is `--sort`, default updated. A query that pages ("pages": "cursor") takes --cursor <cursor> (the first page when left out) and --limit <limit> (default: the most its page's list holds), after its own. A parameter with a range ("string 1..20") carries it as "range": "1..20" — a number's value, a string's length, a list's count — and the command refuses a value outside it, as it refuses any argument that is not one of its own.
    /// - `--yes`: a "destructive" operation's command does nothing without it (`delete-doc <id> --yes`): it cannot be undone, so the CLI asks as the screen's confirmation does. Without --yes it exits {const:CLI_REFUSED} and prints what it would do, as a dry run:
    ///
    ///   ```json
    ///   {"error": "needs --yes",
    ///    "dryRun": {"operation": "deleteDoc", "does": "Deletes a document…",
    ///               "with": {"id": "c"}}}
    ///   ```
    ///
    /// - bulk: a bulk form ("bulk": "deleteDocs" on deleteDoc, [operations](help:operations)) is a command of its own, its whole name in kebab case, `delete-docs`, beside deleteDoc's `delete-doc`. It takes its ids as positional arguments, one or more, then any flag:
    ///
    ///   ```sh
    ///   delete-docs c e --yes
    ///   ```
    ///
    ///   and does each, one by one, answering for every item in the order given, the error on each one not done:
    ///
    ///   ```json
    ///   {"results": [{"id": "c", "error": "notFound"}, {"id": "e"}]}
    ///   ```
    ///
    ///   It exits 0 when every item was done, {const:CLI_FAILED} when any was not (refused by the precondition, which it asks of each item, or failed); {const:CLI_REFUSED} only when it tried none: without --yes.
    /// - `--no-wait`: a "long" operation's command waits for its job by default, printing its progress on stderr, one JSON object a line,
    ///
    ///   ```json
    ///   {"job": "j42", "status": "running", "progress": 40}
    ///   ```
    ///
    ///   and then prints its result as any command does; a job that fails exits {const:CLI_FAILED} with its failure. Given --no-wait it prints the job at once instead, {"result": <its job type>}, exit 0.
    /// - `<name>-job`: the query derived to read a long command's job (exportDocsJob) is the long command's name and -job: `export-docs-job <id>` prints the job as it stands, {"result": {"id", "status", "progress", "result", "error"}}, running, done or failed alike (exit 0).
    /// - stdout: one JSON object: {"result": <the operation's result>}, or {} for an operation that answers with nothing. A paged query's result is its page: {"result": {"docs": […], "next": "p2"}}, next null on the last page. Its JSON Schema holds the result's type and every type it reaches, so no view state and nothing the app derives.
    /// - exit codes: the same as underdraw's own:
    ///   - 0: the result
    ///   - {const:CLI_REFUSED}: refused before it was tried: its "requires" condition does not hold, {"error": "requires canDelete"}; or, after that, a destructive command given no --yes ({"error": "needs --yes", "dryRun": …})
    ///   - {const:CLI_FAILED}: tried, and failed in one of its "fails". {"error": "notFound"}
    ///
    /// **Answers**
    ///
    /// ```
    /// {
    ///   "screen"
    ///   "commands": [{"name", "operation", "does": "query" or "command", "doc",
    ///                 "destructive": true (only when it is),
    ///                 "bulkOf": "deleteDoc" (for a bulk form: what it does to each),
    ///                 "long": {"job": its job type, "poll": "export-docs-job <id>",
    ///                          "noWait": the JSON Schema of what --no-wait prints,
    ///                          "progress": what it prints on stderr} (only when
    ///                          it is long),
    ///                 "jobOf": the long command whose job it reads (only then),
    ///                 "usage": "delete-doc <id> --yes",
    ///                 "args": [{"param", "flag" (none when positional), "type",
    ///                           "choices", "range", "default", "repeated"}, …],
    ///                 "stdout": the JSON Schema of what it prints on exit 0,
    ///                 "fails": [{"error", "when", "exit", "stdout"}, …]:
    ///                          its precondition first, then its dry run when it
    ///                          is destructive, then each named failure; a
    ///                          bulk form's precondition and failures are an
    ///                          item's, exit 2, "stdout" one item's result,
    ///                 "help": a draft of the command's --help}, …]
    ///   "golden":   [{"command", "fixture" or "failure", "args", "with", "yes",
    ///                 "noWait", "exit", "stdout"}, …]: what a faithful CLI prints:
    ///     for each fixture, each query that fills "data" and needs no argument,
    ///               given no argument: the result that fixture holds, projected
    ///               through what the query fills (docs[].doc: each row's doc);
    ///               a result filled field by field ("from": "listDocs.next")
    ///               is put together from them, when each of its fields is
    ///               filled: a paged query's first page, its "next" null in a
    ///               fixture of the last page and set in one with more to come;
    ///     for each fixture, each command a node of the screen calls there,
    ///               given what that node passes: exit 1 where its precondition
    ///               does not hold in the fixture, {} where it holds and the
    ///               command answers with nothing (a command that answers with
    ///               something has no golden output: the fixture does not hold it).
    ///               A destructive command has two: without --yes, its dry run
    ///               where the precondition holds; with --yes ("yes": true), as
    ///               above. Where it does not hold, both are refused.
    ///               A bulk form called with ids, exit 0: each item done,
    ///               {"results": [{"id": "c"}, {"id": "e"}]} (its precondition is
    ///               asked of each item: the fixture's selection holds for it).
    ///               A long command's come from the job the fixture holds (none
    ///               where it holds none): waiting, the result where it is done
    ///               and exit 2 with its error where it failed (none where it is
    ///               still running); with --no-wait ("noWait": true), the job;
    ///     for each fixture that holds a long command's job, its -job command
    ///               given the job's id: the job, whatever its status;
    ///     for each named failure, whatever the command is given: exit 2. A
    ///               bulk form's are partial failures, each in the first fixture
    ///               whose call gives two ids or more, given those: the first
    ///               item fails that way ("failure": "notFound", or "requires
    ///               canDelete" for one refused), the others are done, exit 2.
    ///     "args" are the arguments after the command's name, defaults left
    ///     out; "with" the parameters they give, defaults filled in.
    ///
    ///   "fixtures": {name: data}: each fixture with its "$patch" applied and
    ///               every default filled in, as `underdraw handoff` resolves
    ///               them, with no platform needed: the data a fake server holds
    ///               to run the CLI in that fixture.
    /// }
    /// ```
    ///
    /// **How to use it**
    ///
    /// - Build the CLI from "commands": its names, arguments, help and exit codes. Implement each command once, in the same domain layer the screen's operations call.
    /// - Validate each command's stdout against its "stdout" schema.
    /// - Test it with the golden outputs: run the CLI on a fixture's data (a fake of the server that answers with "fixtures"[<the golden's fixture>]), capture what it printed and its exit code, and give it to `underdraw compare` (see its --help, **A CLI's capture**). For a named failure, make the fake fail that way.
    ///
    /// **What trips you up**
    ///
    /// - The fixtures are the screen's: a query's golden output exists only for a query whose result fills a field of "data" ("from").
    /// - A precondition is a condition over the view model: the CLI must answer the same question the screen asks (does the person own the document?) before it calls the command. One declared on a type ("requires Order.cancellable") is asked of the value the command names, the order of the id it is given: its golden output is refused where that order's condition does not hold.
    /// - A destructive command checks its precondition first, then --yes: a dry run says only what would be done, never what would be refused.
    /// - A bulk form never stops at the first item that is not done: it tries each, and says of each. A failed item is in "results", not left out.
    /// - A long command's progress goes to stderr, never stdout: stdout stays one JSON object, the result or, with --no-wait, the job. compare reads stdout and the exit code only.
    #[command(next_help_heading = "Run by the agent that builds the CLI", verbatim_doc_comment)]
    Cli {
        /// The screen's directory, which holds screen.json, or the screen.json itself
        #[arg(verbatim_doc_comment)]
        path: PathBuf,
    },
    /// Read what underdraw depends on, change nothing, and name the fix of each finding
    ///
    /// **When**
    ///
    /// After installing underdraw, or when a command fails for a reason outside the files it was given.
    ///
    /// **Reads**
    ///
    /// What underdraw depends on. Checking depends on nothing outside its files; rendering, once built, will need a GPU and fonts. Changes nothing.
    ///
    /// **Answers**
    ///
    /// ```
    /// {"ok": true when nothing is missing, "version": this build,
    ///  "findings": [{"problem", "fix"}, …]}
    /// ```
    #[command(
        next_help_heading = "Run by whoever installs underdraw, or the agent on their behalf",
        verbatim_doc_comment
    )]
    Doctor,
}

/// The languages `--lang` takes, each by its name.
fn lang() -> impl TypedValueParser<Value = Lang> {
    PossibleValuesParser::new(Lang::all().map(Lang::name))
        .map(|name| Lang::parse(&name).expect("one of the names offered"))
}

/// Each topic, its Markdown in help/<name>.md, as a type a message names it
/// by; and `with_topics`, which adds them all to a manual in this order.
macro_rules! topics {
    ($($name:tt => $type:ident),* $(,)?) => {
        $(
            #[doc = include_str!(concat!("help/", $name, ".md"))]
            #[derive(Topic)]
            #[topic(name = $name)]
            pub(crate) struct $type;
        )*

        fn with_topics(help: Help) -> Help {
            help$(.topic::<$type>())*
        }
    };
}

topics! {
    "index" => Index,
    "files" => Files,
    "screen-json" => ScreenJson,
    "data-and-types" => DataAndTypes,
    "fixtures" => Fixtures,
    "params" => Params,
    "conditions" => Conditions,
    "templates-and-formats" => TemplatesAndFormats,
    "cases" => Cases,
    "expressions" => Expressions,
    "nodes" => Nodes,
    "trees" => Trees,
    "assumed" => Assumed,
    "layout-on-a-node" => LayoutOnANode,
    "actions" => Actions,
    "operations" => Operations,
    "paging" => Paging,
    "components" => Components,
    "popups" => Popups,
    "phases" => Phases,
    "transitions" => Transitions,
    "journeys" => Journeys,
    "responsive" => Responsive,
    "built-in-components" => BuiltInComponents,
    "domain-json" => DomainJson,
    "tokens-json" => TokensJson,
    "looks" => Looks,
    "a-whole-screen" => AWholeScreen,
    "gallery-window" => GalleryWindow,
}

/// The manual: the tool's text and each command's, then every topic, the
/// numbers and lists they quote filled in from the code that holds them.
pub(crate) fn help() -> Help {
    with_topics(Help::new(Cli::command()))
        .constant("FAULTS", FAULTS)
        .constant("FAILED", FAILED)
        .constant("WIDTH_LEAST", WIDTH_RANGE.start())
        .constant("WIDTH_MOST", WIDTH_RANGE.end())
        .constant("PHONE_WIDTH", DEFAULT_WIDTHS[0])
        .constant("DESKTOP_WIDTH", DEFAULT_WIDTHS[1])
        .constant("DEFAULT_WIDTHS", and(DEFAULT_WIDTHS))
        .constant("SCALE_LEAST", SCALES.start())
        .constant("SCALE_MOST", SCALES.end())
        .constant("LANGS", or(Lang::all().map(Lang::name)))
        .constant("LENGTH_MOST", underdraw_format::length::MOST)
        .constant("CLI_REFUSED", underdraw_handoff::REFUSED)
        .constant("CLI_FAILED", underdraw_handoff::FAILED)
}

/// `items` as a sentence lists them all: "390 and 1120", "a, b and c".
fn and(items: impl IntoIterator<Item = impl ToString>) -> String {
    joined(items, "and")
}

/// `items` as a sentence offers them: "en, zh or ja".
fn or(items: impl IntoIterator<Item = impl ToString>) -> String {
    joined(items, "or")
}

fn joined(items: impl IntoIterator<Item = impl ToString>, last_by: &str) -> String {
    let items: Vec<String> = items.into_iter().map(|item| item.to_string()).collect();
    match items.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} {last_by} {last}", rest.join(", ")),
        None => String::new(),
    }
}

/// Whether `args` ask for `-h`, the one-screen listing, which clap writes
/// from the command tree itself: `--help` is the manual.
pub(crate) fn short_help(args: &[OsString]) -> bool {
    args.iter().skip(1).take_while(|arg| *arg != "--").any(|arg| arg == "-h")
}

/// The command tree `-h` is answered from: clap's own listing, each
/// subcommand's arguments under clap's headings, not under the group that
/// says who runs it.
pub(crate) fn short() -> clap::Command {
    let mut command = Cli::command();
    let names: Vec<String> = command.get_subcommands().map(|sub| sub.get_name().to_string()).collect();
    for name in names {
        command = command.mut_subcommand(name, |sub| {
            let ids: Vec<clap::Id> = sub.get_arguments().map(|arg| arg.get_id().clone()).collect();
            ids.into_iter().fold(sub, |sub, id| sub.mut_arg(id, |arg| arg.help_heading(None)))
        });
    }
    command
}

#[cfg(test)]
mod tests {
    /// Every command has text and a group, and every placeholder and topic
    /// link names something registered.
    #[test]
    fn the_manual_is_whole() {
        agent_help::check(&super::help()).unwrap();
    }
}
