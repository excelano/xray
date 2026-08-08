//! xray — a read-only profiler for tabular data.
//!
//! Films a delimited file — or piped data — and reports what it is (see
//! DESIGN.md). xray never writes: it observes, so xled can clean and xql can
//! query.

mod findings;
mod json;
mod render;
mod resolve;
mod scan;
mod skill;
mod theme;

use std::fs::File;
use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
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
    /// The CSV/DSV file to profile. Omit it, or give `-`, to read stdin.
    file: Option<PathBuf>,

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

    /// Install xray's Claude Code skill into ~/.claude/skills/xray and exit.
    #[arg(long)]
    install_skill: bool,

    /// Remove the installed Claude Code skill and exit.
    #[arg(long)]
    uninstall_skill: bool,
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

/// What the render header and the JSON `file` field show for piped input. The
/// parentheses keep it from being read as a filename that actually exists.
const STDIN_NAME: &str = "(stdin)";

/// The short label for a file: its bare name, so the header line stays short.
fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    // Terminal actions: they touch the user's skills directory and nothing
    // else, so they run before any input is read or any file is opened.
    if cli.install_skill {
        return ExitCode::from(skill::install() as u8);
    }
    if cli.uninstall_skill {
        return ExitCode::from(skill::uninstall() as u8);
    }
    // --no-header is the family spelling for --header 0; clap already rejects
    // the two together, so there is no disagreement left to resolve here.
    let header = if cli.no_header { Some(0) } else { cli.header };

    // `-` is the explicit spelling for stdin and omitting the file means the
    // same thing — except that a bare `xray` at a terminal has no input coming,
    // so it says so rather than sitting silently on a stdin nobody is writing
    // to. An explicit `-` still blocks, the way `cat -` does: that was asked for.
    let path = match cli.file.as_deref() {
        Some(p) if p != Path::new("-") => Some(p),
        Some(_) => None,
        None if std::io::stdin().is_terminal() => {
            eprintln!("xray: no input — give a file, or pipe data in");
            return ExitCode::from(2);
        }
        None => None,
    };

    // Two labels, because they answer different questions: errors want the path
    // as given so you can find the file, the render wants the bare name.
    let source = path.map_or_else(|| STDIN_NAME.to_string(), |p| p.display().to_string());
    let name = path.map_or_else(|| STDIN_NAME.to_string(), display_name);

    let reader: Box<dyn Read> = match path {
        Some(p) => match File::open(p) {
            Ok(f) => Box::new(f),
            Err(e) => {
                eprintln!("xray: {source}: {e}");
                return ExitCode::FAILURE;
            }
        },
        None => Box::new(std::io::stdin().lock()),
    };

    match scan::scan(reader, header, cli.delim) {
        Ok(s) => {
            if cli.json {
                let value = json::to_json(&name, path.map(|_| source.as_str()), &s, cli.refer);
                println!("{}", serde_json::to_string_pretty(&value).unwrap());
                return ExitCode::SUCCESS;
            }
            let choice = match cli.color {
                ColorWhen::Auto => ColorChoice::Auto,
                ColorWhen::Always => ColorChoice::Always,
                ColorWhen::Never => ColorChoice::Never,
            };
            let text = render::render(&name, path.map(|_| source.as_str()), &s, cli.refer);
            let mut out = AutoStream::new(std::io::stdout(), choice);
            let _ = out.write_all(text.as_bytes());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("xray: {source}: {e}");
            ExitCode::FAILURE
        }
    }
}
