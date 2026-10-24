//! The effect layer: reads the screen description from disk, asks the pure
//! crates what is wrong with it (`underdraw-format`), where everything goes
//! (`underdraw-layout`), what to hand off (`underdraw-handoff`) and how a
//! capture differs (`underdraw-compare`), draws on the GPU, and prints the
//! answer as one JSON object.

mod act;
mod area;
mod chrome;
mod draw;
mod field;
mod flow;
mod gallery;
mod gpu;
mod inbox;
mod journeys;
mod links;
mod manual;
mod notes;
mod paint;
mod pictures;
mod project;
mod reference;
mod scrollbar;
mod sheet;
mod sidebar;
mod text;
mod window;

use std::ffi::OsString;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use agent_help::Topic;
use anyhow::{Context, bail};
use clap::error::ErrorKind;
use clap::{CommandFactory, FromArgMatches};
use draw::{Shown, Wanted};
use serde_json::{Value, json};
use underdraw_format::{Lang, Screen, ScreenParams, State};

use crate::manual::{Cli, Command};
use crate::project::{Project, TOKENS};

// The binary every topic's reference names: `underdraw help <topic>`.
agent_help::binary!("underdraw");

/// What this binary was built from, as `--version` says it: stamped by
/// `build.rs`.
pub const VERSION: &str = env!("UNDERDRAW_VERSION");

/// The file a screen's directory holds its description in.
const SCREEN: &str = "screen.json";
/// Where a project's own components were written before they moved into
/// screen.json's "components" (ADR 0021): `check` refuses one still there.
const KINDS: &str = "kinds.json";

/// Exit 1: the answer names faults (`check` with findings). A gate or script
/// can stop on the exit code alone; the full answer is still printed.
pub(crate) const FAULTS: u8 = 1;
/// Exit 2: the command could not do its work, and answered `{"error"}`.
pub(crate) const FAILED: u8 = 2;

/// Runs the command `args` names and prints its answer: one JSON object on
/// stdout, except for `-h`, `--help`, `help` and `--version`, which print
/// text.
pub fn main(args: impl IntoIterator<Item = OsString>) -> ExitCode {
    let args: Vec<OsString> = args.into_iter().collect();
    let help = manual::help();
    let command = if manual::short_help(&args) {
        manual::short()
    } else {
        match help.clap() {
            Ok(command) => command,
            Err(e) => {
                println!("{}", json!({ "error": format!("the manual is broken: {e}") }));
                return ExitCode::from(FAILED);
            }
        }
    };
    let parsed = command.try_get_matches_from(args).and_then(|matches| Cli::from_arg_matches(&matches));
    let cli = match parsed {
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
        Command::Help { topic: None } => {
            print!("{}", help.manual().expect("checked by clap()"));
            return ExitCode::SUCCESS;
        }
        Command::Help { topic: Some(topic) } => match help.topic_help(&topic) {
            Ok(markdown) => {
                print!("{markdown}");
                return ExitCode::SUCCESS;
            }
            Err(e) => Err(anyhow::anyhow!("{e}")),
        },
        Command::Gallery { root, lang, window: true } => {
            window::run(&root, lang);
            return ExitCode::SUCCESS;
        }
        Command::Gallery { root, lang, window: false } => gallery::gallery(&root, lang),
        Command::Notes { create: Some(_), .. } => Err(anyhow::anyhow!(notes::THE_PERSONS)),
        Command::Notes { path, answer, reply, wait, .. } => {
            notes::notes(&path, answer.as_deref(), reply.as_deref(), wait)
        }
        Command::Check { path, lang } => check(&path, lang),
        Command::Render { path, scale, out, root, lang, all: true, .. } => {
            render_all(&path, scale, out, root.as_deref(), lang)
        }
        Command::Render { path, fixture, phase, width, scale, out, root, lang, all: false } => {
            let wanted = |first| Wanted { fixture, phase, width: width.unwrap_or(first), scale };
            render(&path, wanted, out, root.as_deref(), lang)
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
    answer["notes"] = notes::counted(dir);
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
fn checked(screen: &Screen, dir: &Path) -> anyhow::Result<underdraw_format::Checked> {
    checked_among(screen, dir, &ScreenParams::new())
}

/// What [`checked`] finds, and each navigate to a screen in `others` that
/// does not give its params, a kinds.json still beside it, and a
/// notes.json in the old form.
fn checked_among(
    screen: &Screen,
    dir: &Path,
    others: &ScreenParams,
) -> anyhow::Result<underdraw_format::Checked> {
    checked_inside(screen, dir, others, &[])
}

/// What [`checked_among`] finds, of a description drawn inside each of
/// the screens `outer` names in turn, outermost first: a node that draws
/// one of them, or itself, draws itself, which is refused.
fn checked_inside(
    screen: &Screen,
    dir: &Path,
    others: &ScreenParams,
    outer: &[String],
) -> anyhow::Result<underdraw_format::Checked> {
    let project = Project::of(dir)?;
    let tokens = project.tokens;
    let mut drawing = outer.to_vec();
    drawing.push(screen.name().to_string());
    let album = pictures::Album::new(&project.root, drawing, Lang::default());
    let locale = screen.description().get("locale").and_then(Value::as_str);
    let mut typesetter = text::Typesetter::new(locale).map_err(anyhow::Error::msg)?;
    let mut checked = underdraw_layout::check(screen, &tokens, others, &mut typesetter, &album)
        .map_err(anyhow::Error::msg)?;
    if dir.join(KINDS).exists() {
        checked.fault(underdraw_format::Finding {
            at: KINDS.to_string(),
            problem: format!(
                "kinds.json is no longer read: move each of its components into screen.json's \"components\", declaring the params its look reads, then delete kinds.json (`{}`)",
                manual::Components::REFERENCE
            ),
        });
    }
    if let Some(old) = notes::old_form(dir) {
        checked.fault(underdraw_format::Finding { at: notes::NOTES.to_string(), problem: old });
    }
    Ok(checked)
}

/// The screen at `path` (a directory or its screen.json), read with the
/// domain and components its project shares, and the directory; or, at
/// `components/<Name>` under the project's root, the description showing
/// that shared component on its own.
fn read(path: &Path) -> anyhow::Result<(Screen, PathBuf)> {
    if let Some((name, file)) = project::showcased(path) {
        let shown = underdraw_format::showcase(&name, &project::read_json(&file)?);
        return Ok((Project::of(path)?.screen(&shown), path.to_path_buf()));
    }
    let file = if path.is_dir() { path.join(SCREEN) } else { path.to_path_buf() };
    let text = std::fs::read_to_string(&file).with_context(|| {
        format!(
            "cannot read {}: pass a screen's directory, which holds {SCREEN}, or the {SCREEN} itself",
            file.display()
        )
    })?;
    let screen: Value = serde_json::from_str(&text).with_context(|| {
        format!("{} is not JSON: fix the line and column named here, then check again", file.display())
    })?;
    let dir = file.parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    Ok((Project::of(&dir)?.screen(&screen), dir))
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
    let locale = screen.description().get("locale").and_then(Value::as_str);
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

/// The screens drawn under `--root`, when it is given.
fn drawn_under(root: Option<&Path>) -> anyhow::Result<Option<std::collections::BTreeSet<String>>> {
    Ok(match root {
        Some(root) if !root.is_dir() => {
            bail!(
                "{} is not a directory: pass the directory the screen descriptions are under",
                root.display()
            )
        }
        Some(root) => Some(links::drawn(root)),
        None => None,
    })
}

/// `pixels`, `width` × `height`, written to `out`, its directory made.
fn write_out(out: &Path, width: u32, height: u32, pixels: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("cannot create {}", parent.display()))?;
    }
    write_png(out, width, height, pixels)
}

/// A state as `render` answers it.
fn state_json(shown: &Shown) -> Value {
    match shown {
        Shown::Fixture(name) => json!({ "fixture": name }),
        Shown::Phase(name) => json!({ "phase": name }),
    }
}

/// The state of the description at `path` that `wanted` names, given the
/// first of the widths its tokens name, drawn to a PNG.
fn render(
    path: &Path,
    wanted: impl FnOnce(u32) -> Wanted,
    out: Option<PathBuf>,
    root: Option<&Path>,
    lang: Lang,
) -> anyhow::Result<Value> {
    let drawn = drawn_under(root)?;
    let description = draw::Description::read(path)?;
    let wanted = &wanted(description.widths()[0]);
    let laid = description.lay_out(wanted, drawn.as_ref(), lang)?;
    let mut gpu = gpu::Gpu::open()?;
    let mut drawing = laid.draw(wanted.scale, gpu.max_side);
    let [pixels_wide, pixels_high] = drawing.pixels;
    drawing.scene.append(&drawing.marks, None);
    let pixels = gpu.render(&drawing.scene, pixels_wide, pixels_high)?;

    let name = drawing.shown.name();
    let out = out.unwrap_or_else(|| drawing.dir.join("renders").join(format!("{name}-{}.png", wanted.width)));
    write_out(&out, pixels_wide, pixels_high, &pixels)?;

    let page = &drawing.page;
    let project = Project::of(&drawing.dir)?;
    Ok(json!({
        "png": out,
        "project": project.root,
        "tokens": project.tokens_file,
        "state": state_json(&drawing.shown),
        "breakpoint": drawing.breakpoint,
        "width": page.width,
        "height": page.height,
        "scale": drawing.scale,
        "pixels": drawing.pixels,
        "nodes": page.nodes,
    }))
}

/// Every state of the description at `path`, each at both widths, on one
/// sheet written to `out`, and where each node went in each.
fn render_all(
    path: &Path,
    scale: u32,
    out: Option<PathBuf>,
    root: Option<&Path>,
    lang: Lang,
) -> anyhow::Result<Value> {
    let drawn = drawn_under(root)?;
    let description = draw::Description::read(path)?;
    let mut sheet = sheet::Sheet::lay_out(&description, drawn.as_ref(), lang)?;
    let mut gpu = gpu::Gpu::open()?;
    let drawing = sheet.draw(scale, gpu.max_side)?;
    let [pixels_wide, pixels_high] = drawing.pixels;
    let pixels = gpu.render(&drawing.scene, pixels_wide, pixels_high)?;
    let out = out.unwrap_or_else(|| description.dir().join("renders").join("all.png"));
    write_out(&out, pixels_wide, pixels_high, &pixels)?;

    let project = Project::of(description.dir())?;
    let states: Vec<Value> = sheet
        .pages
        .iter()
        .map(|placed| {
            let laid = &placed.laid;
            json!({
                "state": state_json(&laid.shown),
                "breakpoint": laid.breakpoint,
                "width": laid.page.width,
                "height": laid.page.height,
                "at": { "x": placed.x, "y": placed.y },
                "nodes": laid.page.nodes,
            })
        })
        .collect();
    Ok(json!({
        "png": out,
        "project": project.root,
        "tokens": project.tokens_file,
        "width": sheet.width,
        "height": sheet.height,
        "scale": drawing.scale,
        "pixels": drawing.pixels,
        "states": states,
    }))
}

/// `pixels`, RGBA rows `width` × `height`, written to `out` as a PNG.
fn write_png(out: &Path, width: u32, height: u32, pixels: &[u8]) -> anyhow::Result<()> {
    let file = File::create(out).with_context(|| format!("cannot write {}", out.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(pixels)?;
    Ok(())
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
