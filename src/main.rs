//! xray — a read-only profiler for tabular data.
//!
//! Films a delimited file and reports what it is (see DESIGN.md). xray never
//! writes: it observes, so xled can clean and xql can query. This build renders
//! the film and reading registers; findings, --refer, colour, and --json follow.

mod findings;
mod json;
mod render;
mod resolve;
mod scan;
mod theme;

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use anstream::{AutoStream, ColorChoice};
use clap::{Parser, ValueEnum};

/// When to colourise the output.
#[derive(Clone, Copy, ValueEnum)]
enum ColorWhen {
    /// Colour for a terminal, plain when piped or read by a program (honours NO_COLOR).
    Auto,
    Always,
    Never,
}

/// Profile a delimited file: columns, types, blanks, cardinality, top values.
#[derive(Parser)]
#[command(name = "xray", version, about, long_about = None)]
struct Cli {
    /// The CSV/DSV file to profile.
    file: PathBuf,

    /// Also suggest which family tool treats each finding (off by default).
    #[arg(long)]
    refer: bool,

    /// Emit the profile as JSON instead of the human render.
    #[arg(long)]
    json: bool,

    /// When to colourise: auto (default), always, or never.
    #[arg(long, value_name = "WHEN", default_value = "auto")]
    color: ColorWhen,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match scan::scan(&cli.file) {
        Ok(s) => {
            let name = cli
                .file
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| cli.file.display().to_string());
            if cli.json {
                let value = json::to_json(&name, &s, cli.refer);
                println!("{}", serde_json::to_string_pretty(&value).unwrap());
                return ExitCode::SUCCESS;
            }
            let choice = match cli.color {
                ColorWhen::Auto => ColorChoice::Auto,
                ColorWhen::Always => ColorChoice::Always,
                ColorWhen::Never => ColorChoice::Never,
            };
            let text = render::render(&name, &s, cli.refer);
            let mut out = AutoStream::new(std::io::stdout(), choice);
            let _ = out.write_all(text.as_bytes());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("xray: cannot read {}: {e}", cli.file.display());
            ExitCode::FAILURE
        }
    }
}
