//! The effect layer: reads the screen description from disk, asks
//! `underdraw-format` what is wrong with it, and prints the answer as one JSON
//! object.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Context;
use clap::error::ErrorKind;
use clap::{CommandFactory, Parser, Subcommand};
use serde_json::{Value, json};

/// What this binary was built from, as `--version` says it: stamped by
/// `build.rs`.
pub const VERSION: &str = env!("UNDERDRAW_VERSION");

/// The file a screen's directory holds its description in.
const SCREEN: &str = "screen.json";

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
    let checked = underdraw_format::check(&screen);
    Ok(serde_json::to_value(checked)?)
}

/// What underdraw depends on. Checking needs nothing outside the files it is
/// given; rendering, when it lands, will need a GPU and fonts, and they will
/// be read here.
fn doctor() -> Value {
    json!({ "ok": true, "version": VERSION, "findings": [] })
}

/// The subcommands that exist, by name.
pub fn commands() -> Vec<String> {
    Cli::command().get_subcommands().map(|c| c.get_name().to_string()).collect()
}
