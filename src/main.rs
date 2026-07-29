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

    /// Header row (1-based); 0 = no header. Omit to auto-detect a buried header.
    #[arg(long, value_name = "ROW")]
    header: Option<usize>,

    /// Treat the first row as data, not a header (same as --header 0).
    #[arg(long, conflicts_with = "header")]
    no_header: bool,

    /// Field delimiter (use \t for tab). Omit to sniff it from the file.
    #[arg(short, long, value_name = "CHAR", value_parser = parse_delim)]
    delim: Option<u8>,

    /// When to colourise: auto (default), always, or never.
    #[arg(long, value_name = "WHEN", default_value = "auto")]
    color: ColorWhen,
}

/// Parse a `--delim` value: one ASCII character, or the escape `\t` for tab.
/// The escape earns its keep because a literal tab is awkward to type and most
/// shells swallow it; the rest of the family accepts it for the same reason.
fn parse_delim(s: &str) -> Result<u8, String> {
    let c = if s == "\\t" || s == "\t" {
        '\t'
    } else {
        let mut chars = s.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => c,
            _ => {
                return Err(format!(
                    "expected one character (or \\t for tab), got {s:?}"
                ))
            }
        }
    };
    if !c.is_ascii() {
        return Err(format!("expected an ASCII character, got {c:?}"));
    }
    Ok(c as u8)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    // --no-header is the family spelling for --header 0; clap already rejects
    // the two together, so there is no disagreement left to resolve here.
    let header = if cli.no_header { Some(0) } else { cli.header };
    match scan::scan(&cli.file, header, cli.delim) {
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
            eprintln!("xray: {}: {e}", cli.file.display());
            ExitCode::FAILURE
        }
    }
}
