//! xray — a read-only profiler for tabular data.
//!
//! Scaffold only. The profile battery, output format, and flag surface are
//! being settled — see DESIGN.md. xray never writes: it observes a file and
//! reports what it is, so xled can clean it and xql can query it.

use std::path::PathBuf;

use clap::Parser;

/// Profile a delimited file: columns, types, blanks, cardinality, top values.
#[derive(Parser)]
#[command(name = "xray", version, about, long_about = None)]
struct Cli {
    /// The CSV/DSV file to profile (omit to read from stdin — not yet wired).
    file: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();
    match cli.file {
        Some(path) => {
            eprintln!("xray: scaffold — profiling of {} is not implemented yet.", path.display());
            eprintln!("See DESIGN.md; the profile battery and output format are being settled.");
        }
        None => {
            eprintln!("xray: give me a file to profile (stdin not yet wired). See DESIGN.md.");
        }
    }
}
