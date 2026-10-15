//! The effect layer: reads the screen description from disk, asks the pure
//! crates what is wrong with it (`underdraw-format`), where everything goes
//! (`underdraw-layout`), what to hand off (`underdraw-handoff`) and how a
//! capture differs (`underdraw-compare`), draws on the GPU, and prints the
//! answer as one JSON object.

mod act;
mod draw;
mod flow;
mod gallery;
mod gpu;
mod journeys;
mod links;
mod notes;
mod paint;
mod project;
mod reference;
mod text;
mod window;

use std::ffi::OsString;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, bail};
use clap::error::ErrorKind;
use clap::{CommandFactory, Parser, Subcommand};
use draw::{Shown, Wanted};
use serde_json::{Value, json};
use underdraw_format::ScreenParams;
use underdraw_format::{Lang, State};

use crate::project::{Project, TOKENS};

/// What this binary was built from, as `--version` says it: stamped by
/// `build.rs`.
pub const VERSION: &str = env!("UNDERDRAW_VERSION");

/// The file a screen's directory holds its description in.
const SCREEN: &str = "screen.json";
/// Where a project's own components were written before they moved into
/// screen.json's "components" (ADR 0021): `check` refuses one still there.
const KINDS: &str = "kinds.json";

/// `-h` is the one-screen listing; `--help` adds `help.txt`, the guide an
/// agent reads to learn the tool. Each subcommand's `--help` adds
/// `help/<name>.txt`; `check --help` also adds `help/format.txt`, the
/// reference for writing a description.
#[derive(Parser)]
#[command(name = "underdraw", version = VERSION, about, long_about = include_str!("help.txt"), disable_help_subcommand = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check a screen description, or every one under a directory: every fault, and which fixtures each condition holds in
    #[command(after_long_help = concat!(include_str!("help/check.txt"), include_str!("help/format.txt")))]
    Check {
        /// The screen's directory, which holds screen.json, or the screen.json itself; or a directory holding no screen.json, to check every description under it and the navigates between them
        path: PathBuf,
        /// The language underdraw words its own assumptions in, a screen not drawn yet among the links: en or zh
        #[arg(long, default_value = "en", value_parser = lang)]
        lang: Lang,
    },
    /// Draw a screen in one state as a wireframe PNG, and say where each node went
    #[command(after_long_help = include_str!("help/render.txt"))]
    Render {
        /// The screen's directory, which holds screen.json (and tokens.json), or the screen.json itself
        path: PathBuf,
        /// The fixture whose data to show; the first fixture when neither this nor --phase is given
        #[arg(long, conflicts_with = "phase")]
        fixture: Option<String>,
        /// A phase to show instead of data, such as loading
        #[arg(long)]
        phase: Option<String>,
        /// The page width in points: 390 is a phone, 1120 a laptop browser
        #[arg(long, default_value_t = 390, value_parser = clap::value_parser!(u32).range(200..=4000))]
        width: u32,
        /// Pixels per point
        #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u32).range(1..=4))]
        scale: u32,
        /// Where to write the PNG; <dir>/renders/<fixture or phase>-<width>.png when not given
        #[arg(long)]
        out: Option<PathBuf>,
        /// The directory the screen descriptions are under, as `check <root>` and the gallery take it: a node that navigates to a screen none of them declares is marked assumed. Not given, navigates are not resolved
        #[arg(long)]
        root: Option<PathBuf>,
        /// The language underdraw words its own assumptions in, a screen not drawn yet or a component it does not know: en or zh
        #[arg(long, default_value = "en", value_parser = lang)]
        lang: Lang,
    },
    /// A window for the person: every screen description under a directory, each drawn at a phone and a desktop width
    #[command(after_long_help = include_str!("help/gallery.txt"))]
    Gallery {
        /// The directory the screen descriptions are under, at any depth
        root: PathBuf,
        /// The language underdraw words its own assumptions in, a screen not drawn yet or a component it does not know: en or zh
        #[arg(long, default_value = "en", value_parser = lang)]
        lang: Lang,
        /// Run the window itself, in this process: what the command starts
        #[arg(long, hide = true)]
        window: bool,
    },
    /// The person's open notes on the descriptions under a directory, each with where it is pinned; or one answered
    #[command(after_long_help = include_str!("help/notes.txt"))]
    Notes {
        /// A description's directory, or a directory of them at any depth
        path: PathBuf,
        /// The note to answer, by its id (N3), in the description at <path>
        #[arg(long, value_name = "ID", requires = "reply")]
        answer: Option<String>,
        /// The answer: one line saying what was done, or why not
        #[arg(long, value_name = "LINE")]
        reply: Option<String>,
        /// Block until a note is open, then answer as without it: at once when one is open already
        #[arg(long)]
        wait: bool,
        /// List the closed notes instead of the open, each closed by the person or, saying why, by itself
        #[arg(long)]
        closed: bool,
        /// Refused: the person writes notes, in the gallery
        #[arg(long, hide = true, alias = "add", num_args = 0..=1, default_missing_value = "")]
        create: Option<String>,
        /// Refused: the person closes a note, in the gallery
        #[arg(long, hide = true, num_args = 0..=1, default_missing_value = "")]
        close: Option<String>,
        /// Refused: the person reopens a note, in the gallery
        #[arg(long, hide = true, num_args = 0..=1, default_missing_value = "")]
        reopen: Option<String>,
    },
    /// The view model as a JSON Schema, to validate API responses and other data against
    #[command(after_long_help = include_str!("help/schema.txt"))]
    Schema {
        /// The screen's directory, which holds screen.json, or the screen.json itself
        path: PathBuf,
    },
    /// Everything an agent needs to build the screen on one platform: model code, tokens, nodes, states to test
    #[command(after_long_help = include_str!("help/handoff.txt"))]
    Handoff {
        /// The screen's directory, which holds screen.json, tokens.json and platforms/<platform>.json
        path: PathBuf,
        /// The platform to build for: the name of a file in <dir>/platforms, such as web, ios or android
        #[arg(long)]
        platform: String,
    },
    /// The command-line interface the screen's operations derive: commands, stdout schemas, failures, help drafts, golden outputs
    #[command(after_long_help = include_str!("help/cli.txt"))]
    Cli {
        /// The screen's directory, which holds screen.json, or the screen.json itself
        path: PathBuf,
    },
    /// How a running UI or CLI differs from the description in one state: a capture of it against what that state shows or prints
    #[command(after_long_help = include_str!("help/compare.txt"))]
    Compare {
        /// The screen's directory, which holds screen.json, tokens.json and platforms/<platform>.json
        path: PathBuf,
        /// The platform the UI runs on: the name of a file in <dir>/platforms; not given for a CLI's capture
        #[arg(long)]
        platform: Option<String>,
        /// The capture: a JSON file of the UI's elements, or of a CLI's run, written as `underdraw compare --help` says
        #[arg(long)]
        capture: PathBuf,
        /// The fixture whose data the UI showed, or the CLI answered from; the first fixture when neither this nor --phase is given
        #[arg(long, conflicts_with = "phase")]
        fixture: Option<String>,
        /// The phase the UI was in instead, such as loading
        #[arg(long)]
        phase: Option<String>,
    },
    /// Read what underdraw depends on, change nothing, and name the fix of each finding
    #[command(after_long_help = include_str!("help/doctor.txt"))]
    Doctor,
}

/// Exit 1: the answer names faults (`check` with findings). A gate or script
/// can stop on the exit code alone; the full answer is still printed.
const FAULTS: u8 = 1;
/// Exit 2: the command could not do its work, and answered `{"error"}`.
const FAILED: u8 = 2;

/// Runs the command `args` names and prints its answer: one JSON object on
/// stdout, except for `--help` and `--version`, which print text.
pub fn main(args: impl IntoIterator<Item = OsString>) -> ExitCode {
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(e) if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) => {
            print!("{e}");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            let usage = e.render().to_string();
            println!("{}", json!({ "error": usage.trim_end(), "fix": "underdraw --help" }));
            return ExitCode::from(FAILED);
        }
    };
    let answer = match cli.command {
        Command::Gallery { root, lang, window: true } => {
            window::run(&root, lang);
            return ExitCode::SUCCESS;
        }
        Command::Gallery { root, lang, window: false } => gallery::gallery(&root, lang),
        Command::Notes { create: Some(_), .. }
        | Command::Notes { close: Some(_), .. }
        | Command::Notes { reopen: Some(_), .. } => Err(anyhow::anyhow!(notes::THE_PERSONS)),
        Command::Notes { path, answer, reply, wait, closed, .. } => {
            notes::notes(&path, answer.as_deref(), reply.as_deref(), wait, closed)
        }
        Command::Check { path, lang } => check(&path, lang),
        Command::Render { path, fixture, phase, width, scale, out, root, lang } => {
            render(&path, &Wanted { fixture, phase, width, scale }, out, root.as_deref(), lang)
        }
        Command::Schema { path } => schema(&path),
        Command::Cli { path } => derive_cli(&path),
        Command::Handoff { path, platform } => handoff(&path, &platform),
        Command::Compare { path, platform, capture, fixture, phase } => {
            compare(&path, platform.as_deref(), &capture, fixture, phase)
        }
        Command::Doctor => Ok(doctor()),
    };
    match answer {
        Ok(answer) => {
            println!("{answer}");
            match answer.get("ok") {
                Some(Value::Bool(false)) => ExitCode::from(FAULTS),
                _ => ExitCode::SUCCESS,
            }
        }
        Err(e) => {
            println!("{}", json!({ "error": format!("{e:#}") }));
            ExitCode::from(FAILED)
        }
    }
}

/// One description, or every one under a directory that is none.
fn check(path: &Path, lang: Lang) -> anyhow::Result<Value> {
    if path.is_dir() && !describes(path) {
        return links::check_root(path, lang);
    }
    let mut answer = serde_json::to_value(check_at(path, &ScreenParams::new())?)?;
    let dir = if describes(path) { path } else { path.parent().unwrap_or(Path::new(".")) };
    answer["notes"] = notes::counted(dir, dir);
    Ok(answer)
}

/// Whether `dir` is a description's directory: it holds screen.json, or
/// it shows a component the project shares on its own.
fn describes(dir: &Path) -> bool {
    dir.join(SCREEN).is_file() || project::showcased(dir).is_some()
}

/// What `check` says of the description at `path` (a directory or its
/// screen.json), among the screens under its root whose params `others`
/// holds.
fn check_at(path: &Path, others: &ScreenParams) -> anyhow::Result<underdraw_format::Checked> {
    let (screen, dir) = read(path)?;
    checked_among(&screen, &dir, others)
}

/// Every fault in the description, overlays the picture would cut among
/// them: laid out in the system's fonts, as `render` does.
fn checked(screen: &Value, dir: &Path) -> anyhow::Result<underdraw_format::Checked> {
    checked_among(screen, dir, &ScreenParams::new())
}

/// What [`checked`] finds, and each navigate to a screen in `others` that
/// does not give its params, and a kinds.json still beside it.
fn checked_among(
    screen: &Value,
    dir: &Path,
    others: &ScreenParams,
) -> anyhow::Result<underdraw_format::Checked> {
    let tokens = Project::of(dir)?.tokens;
    let locale = screen.get("locale").and_then(Value::as_str);
    let mut typesetter = text::Typesetter::new(locale).map_err(anyhow::Error::msg)?;
    let mut checked =
        underdraw_layout::check(screen, &tokens, others, &mut typesetter).map_err(anyhow::Error::msg)?;
    if dir.join(KINDS).exists() {
        checked.ok = false;
        checked.conditions.clear();
        checked.findings.push(underdraw_format::Finding {
            at: KINDS.to_string(),
            problem: "kinds.json is no longer read: move each of its components into screen.json's \"components\", declaring the params its look reads, then delete kinds.json (`underdraw check --help`, COMPONENTS)".to_string(),
        });
    }
    Ok(checked)
}

/// The screen at `path` (a directory or its screen.json), with the domain
/// and components its project shares, and the directory; or, at
/// `components/<Name>` under the project's root, the description showing
/// that shared component on its own.
fn read(path: &Path) -> anyhow::Result<(Value, PathBuf)> {
    if let Some((name, file)) = project::showcased(path) {
        let shown = underdraw_format::showcase(&name, &project::read_json(&file)?);
        return Ok((Project::of(path)?.describe(shown), path.to_path_buf()));
    }
    let file = if path.is_dir() { path.join(SCREEN) } else { path.to_path_buf() };
    let text = std::fs::read_to_string(&file).with_context(|| {
        format!(
            "cannot read {}: pass a screen's directory, which holds {SCREEN}, or the {SCREEN} itself",
            file.display()
        )
    })?;
    let screen = serde_json::from_str(&text).with_context(|| {
        format!("{} is not JSON: fix the line and column named here, then check again", file.display())
    })?;
    let dir = file.parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    Ok((Project::of(&dir)?.describe(screen), dir))
}

fn schema(path: &Path) -> anyhow::Result<Value> {
    let (screen, _) = read(path)?;
    match underdraw_format::model(&screen) {
        Ok(model) => {
            Ok(json!({ "screen": model.screen(), "schema": underdraw_handoff::json_schema(&model) }))
        }
        Err(checked) => bail!(
            "the description has {} finding(s), so a schema derived from it would be wrong: run `underdraw check {}` and fix them first",
            checked.findings.len(),
            path.display()
        ),
    }
}

fn derive_cli(path: &Path) -> anyhow::Result<Value> {
    let (screen, _) = read(path)?;
    match underdraw_format::model(&screen) {
        Ok(model) => Ok(serde_json::to_value(underdraw_handoff::cli(&model))?),
        Err(checked) => bail!(
            "the description has {} finding(s), so a CLI derived from it would be wrong: run `underdraw check {}` and fix them first",
            checked.findings.len(),
            path.display()
        ),
    }
}

fn handoff(path: &Path, platform: &str) -> anyhow::Result<Value> {
    let (screen, dir) = read(path)?;
    let tokens = Project::of(&dir)?.tokens;
    let mapping = read_platform(&dir, platform)?;
    match underdraw_handoff::handoff(&screen, &mapping, &tokens) {
        Ok(handoff) => Ok(serde_json::to_value(handoff)?),
        Err(checked) => bail!(
            "the description has {} finding(s), so what is handed off would be wrong: run `underdraw check {}` and fix them first",
            checked.findings.len(),
            path.display()
        ),
    }
}

fn compare(
    path: &Path,
    platform: Option<&str>,
    capture: &Path,
    fixture: Option<String>,
    phase: Option<String>,
) -> anyhow::Result<Value> {
    let (screen, dir) = read(path)?;
    let checked = underdraw_format::check(&screen);
    if !checked.ok {
        bail!(
            "the description has {} finding(s), so it cannot say what the UI or CLI should show: run `underdraw check {}` and fix them first",
            checked.findings.len(),
            path.display()
        );
    }
    let (state, name) = pick(&checked, &fixture, &phase)?;
    let text = std::fs::read_to_string(capture).with_context(|| {
        format!(
            "cannot read {}: pass the capture file `underdraw compare --help` describes",
            capture.display()
        )
    })?;
    let capture_value: Value = serde_json::from_str(&text)
        .with_context(|| format!("{} is not JSON: fix the line and column named here", capture.display()))?;
    // A CLI's run says how it exited; a UI's capture never does.
    if capture_value.get("exit").is_some() {
        if let State::Phase(_) = state {
            bail!("a CLI answers from data, not in a phase: pass --fixture, or no --phase");
        }
        let comparison =
            underdraw_compare::compare_cli(&screen, &name, &capture_value).map_err(anyhow::Error::msg)?;
        return Ok(serde_json::to_value(comparison)?);
    }
    let Some(platform) = platform else {
        bail!(
            "a UI's capture is compared on a platform: pass --platform, one of the files in {}",
            dir.join("platforms").display()
        );
    };
    let tokens = Project::of(&dir)?.tokens;
    let mapping = read_platform(&dir, platform)?;
    let locale = screen.get("locale").and_then(Value::as_str);
    let mut typesetter = text::Typesetter::new(locale).map_err(anyhow::Error::msg)?;
    let comparison =
        underdraw_compare::compare(&screen, &mapping, &tokens, state, &capture_value, &mut typesetter)
            .map_err(anyhow::Error::msg)?;
    Ok(serde_json::to_value(comparison)?)
}

/// platforms/<platform>.json beside the description; refused with the
/// platforms there are, when it is not one of them.
fn read_platform(dir: &Path, platform: &str) -> anyhow::Result<Value> {
    let platforms = dir.join("platforms");
    let file = platforms.join(format!("{platform}.json"));
    let text = match std::fs::read_to_string(&file) {
        Ok(text) => text,
        Err(_) => {
            let mut known = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&platforms) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if let Some(stem) = name.strip_suffix(".json") {
                        known.push(stem.to_string());
                    }
                }
            }
            known.sort();
            bail!("there is no {}: the platforms here are {known:?}", file.display());
        }
    };
    serde_json::from_str(&text).with_context(|| format!("{} is not JSON", file.display()))
}

fn render(
    path: &Path,
    wanted: &Wanted,
    out: Option<PathBuf>,
    root: Option<&Path>,
    lang: Lang,
) -> anyhow::Result<Value> {
    let drawn = match root {
        Some(root) if !root.is_dir() => {
            bail!(
                "{} is not a directory: pass the directory the screen descriptions are under",
                root.display()
            )
        }
        Some(root) => Some(links::drawn(root)),
        None => None,
    };
    let laid = draw::lay_out(path, wanted, drawn.as_ref(), lang)?;
    let mut gpu = gpu::Gpu::open()?;
    let drawing = laid.draw(wanted.scale, gpu.max_side);
    let [pixels_wide, pixels_high] = drawing.pixels;
    let pixels = gpu.render(&drawing.scene, pixels_wide, pixels_high)?;

    let name = drawing.shown.name();
    let out = out.unwrap_or_else(|| drawing.dir.join("renders").join(format!("{name}-{}.png", wanted.width)));
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("cannot create {}", parent.display()))?;
    }
    let file = File::create(&out).with_context(|| format!("cannot write {}", out.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), pixels_wide, pixels_high);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&pixels)?;

    let page = &drawing.page;
    let project = Project::of(&drawing.dir)?;
    Ok(json!({
        "png": out,
        "project": project.root,
        "tokens": project.tokens_file,
        "state": match drawing.shown { Shown::Fixture(_) => json!({ "fixture": name }), Shown::Phase(_) => json!({ "phase": name }) },
        "breakpoint": drawing.breakpoint,
        "width": page.width,
        "height": page.height,
        "scale": drawing.scale,
        "pixels": drawing.pixels,
        "nodes": page.nodes,
    }))
}

/// The state `--fixture` or `--phase` names, and its name; the first
/// fixture when neither does.
fn pick<'a>(
    checked: &'a underdraw_format::Checked,
    fixture: &'a Option<String>,
    phase: &'a Option<String>,
) -> anyhow::Result<(State<'a>, String)> {
    Ok(match (phase, fixture) {
        (Some(phase), _) if checked.phases.contains(phase) => (State::Phase(phase), phase.clone()),
        (Some(phase), _) => bail!("there is no phase {phase:?}; the phases are {:?}", checked.phases),
        (None, Some(fixture)) if checked.fixtures.contains(fixture) => {
            (State::Fixture(fixture), fixture.clone())
        }
        (None, Some(fixture)) => {
            bail!("there is no fixture {fixture:?}; the fixtures are {:?}", checked.fixtures)
        }
        (None, None) => match checked.fixtures.first() {
            Some(first) => (State::Fixture(first), first.clone()),
            None => bail!("the description has no fixture; add one under \"fixtures\""),
        },
    })
}

/// What underdraw depends on. Checking needs nothing outside the files it is
/// given; rendering needs a GPU, which is opened here as `render` opens it.
fn doctor() -> Value {
    let (gpu, findings) = match gpu::Gpu::open() {
        Ok(gpu) => (json!({ "name": gpu.name, "backend": gpu.backend, "max_side": gpu.max_side }), json!([])),
        Err(e) => (
            Value::Null,
            json!([{ "problem": format!("{e:#}"), "fix": "render on a machine with a GPU; check needs none" }]),
        ),
    };
    json!({ "ok": gpu.is_object(), "version": VERSION, "gpu": gpu, "findings": findings })
}

/// The subcommands that exist, by name.
pub fn commands() -> Vec<String> {
    Cli::command().get_subcommands().map(|c| c.get_name().to_string()).collect()
}

/// The language `--lang` names.
fn lang(name: &str) -> Result<Lang, String> {
    Lang::parse(name).ok_or_else(|| format!("write one of {}", Lang::NAMES.join(", ")))
}
