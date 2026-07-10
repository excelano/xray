//! xray — a read-only profiler for tabular data.
//!
//! Films a delimited file and reports what it is (see DESIGN.md). xray never
//! writes: it observes, so xled can clean and xql can query. This build renders
//! the film and reading registers; findings, --refer, colour, and --json follow.

mod render;
mod scan;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

/// Profile a delimited file: columns, types, blanks, cardinality, top values.
#[derive(Parser)]
#[command(name = "xray", version, about, long_about = None)]
struct Cli {
    /// The CSV/DSV file to profile.
    file: PathBuf,
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
            print!("{}", render::render(&name, &s));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("xray: cannot read {}: {e}", cli.file.display());
            ExitCode::FAILURE
        }
    }
}
