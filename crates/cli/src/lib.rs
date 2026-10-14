//! The effect layer: reads the screen description from disk, asks the pure
//! crates what is wrong with it (`underdraw-format`) and where everything
//! goes (`underdraw-layout`), draws it on the GPU, and prints the answer as
//! one JSON object.

mod gpu;
mod paint;
mod text;

use std::ffi::OsString;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, bail};
use clap::error::ErrorKind;
use clap::{CommandFactory, Parser, Subcommand};
use serde_json::{Value, json};
use underdraw_format::{Breakpoint, State};

/// What this binary was built from, as `--version` says it: stamped by
/// `build.rs`.
pub const VERSION: &str = env!("UNDERDRAW_VERSION");

/// The file a screen's directory holds its description in.
const SCREEN: &str = "screen.json";
/// The design tokens beside it.
const TOKENS: &str = "tokens.json";

/// `-h` is the one-screen listing; `--help` adds `help.txt`, the guide an
/// agent reads to learn the tool. Each subcommand's `--help` adds
/// `help/<name>.txt`.
#[derive(Parser)]
#[command(name = "underdraw", version = VERSION, about, long_about = include_str!("help.txt"), disable_help_subcommand = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check a screen description: every fault, and which fixtures each condition holds in
    #[command(after_long_help = include_str!("help/check.txt"))]
    Check {
        /// The screen's directory, which holds screen.json, or the screen.json itself
        path: PathBuf,
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
        Command::Check { path } => check(&path),
        Command::Render { path, fixture, phase, width, scale, out } => {
            render(&path, Wanted { fixture, phase, width, scale, out })
        }
        Command::Handoff { path, platform } => handoff(&path, &platform),
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

fn check(path: &Path) -> anyhow::Result<Value> {
    let (screen, _) = read(path)?;
    Ok(serde_json::to_value(underdraw_format::check(&screen))?)
}

/// The screen at `path` (a directory or its screen.json), and the directory.
fn read(path: &Path) -> anyhow::Result<(Value, PathBuf)> {
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
    Ok((screen, dir))
}

fn handoff(path: &Path, platform: &str) -> anyhow::Result<Value> {
    let (screen, dir) = read(path)?;
    let tokens = read_tokens(&dir)?;
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
    let mapping: Value =
        serde_json::from_str(&text).with_context(|| format!("{} is not JSON", file.display()))?;
    match underdraw_handoff::handoff(&screen, &mapping, &tokens) {
        Ok(handoff) => Ok(serde_json::to_value(handoff)?),
        Err(checked) => bail!(
            "the description has {} finding(s), so what is handed off would be wrong: run `underdraw check {}` and fix them first",
            checked.findings.len(),
            path.display()
        ),
    }
}

/// tokens.json beside the description, or no tokens.
fn read_tokens(dir: &Path) -> anyhow::Result<Value> {
    match std::fs::read_to_string(dir.join(TOKENS)) {
        Ok(text) => {
            serde_json::from_str(&text).with_context(|| format!("{} is not JSON", dir.join(TOKENS).display()))
        }
        Err(_) => Ok(json!({})),
    }
}

/// What `render` was asked for.
struct Wanted {
    fixture: Option<String>,
    phase: Option<String>,
    width: u32,
    scale: u32,
    out: Option<PathBuf>,
}

fn render(path: &Path, wanted: Wanted) -> anyhow::Result<Value> {
    let (screen, dir) = read(path)?;
    let tokens = read_tokens(&dir)?;
    let checked = underdraw_format::check(&screen);
    if !checked.ok {
        bail!(
            "the description has {} finding(s), so a picture of it would mislead: run `underdraw check {}` and fix them first",
            checked.findings.len(),
            path.display()
        );
    }
    let (state, name) = match (&wanted.phase, &wanted.fixture) {
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
            None => bail!("the description has no fixture to draw; add one under \"fixtures\""),
        },
    };
    let breakpoint = Breakpoint::for_width(&tokens, wanted.width);
    let root = underdraw_format::instantiate(&screen, state, breakpoint).map_err(anyhow::Error::msg)?;
    let locale = screen.get("locale").and_then(Value::as_str);
    let mut typesetter = text::Typesetter::new(locale).map_err(anyhow::Error::msg)?;
    let page = underdraw_layout::layout(&root, &tokens, wanted.width as f32, &mut typesetter)
        .map_err(anyhow::Error::msg)?;

    let gpu = gpu::Gpu::open()?;
    // A long page is drawn at a lower scale rather than not at all.
    let tallest = f64::from(gpu.max_side) / f64::from(page.height.max(page.width).max(1.0));
    let scale = f64::from(wanted.scale).min(tallest);
    let pixels_wide = (f64::from(page.width) * scale).ceil() as u32;
    let pixels_high = (f64::from(page.height) * scale).ceil() as u32;
    let scene = paint::scene(&page, &mut typesetter, scale);
    let pixels = gpu.render(&scene, pixels_wide, pixels_high)?;

    let out = wanted.out.unwrap_or_else(|| dir.join("renders").join(format!("{name}-{}.png", wanted.width)));
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("cannot create {}", parent.display()))?;
    }
    let file = File::create(&out).with_context(|| format!("cannot write {}", out.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), pixels_wide, pixels_high);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&pixels)?;

    Ok(json!({
        "png": out,
        "state": match state { State::Fixture(_) => json!({ "fixture": name }), State::Phase(_) => json!({ "phase": name }) },
        "breakpoint": breakpoint,
        "width": page.width,
        "height": page.height,
        "scale": scale,
        "pixels": [pixels_wide, pixels_high],
        "nodes": page.nodes,
    }))
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
