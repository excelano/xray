//! The findings register: the diagnostic problem list, ranked most-severe
//! first. Reports damage; never fixes it (that's xled) and never filters it
//! (that's xql). Every finding names what will bite a later step.
//!
//! Grounded in the corpus taxonomy (`~/xled-corpus/CORPUS-FINDINGS.md`). Buried
//! headers are detected in the scan; stacked/side-by-side tables, whitespace
//! pad, smart-punct/HTML-entities, and multi-value newline cells are layered in
//! during the corpus-tuning pass.

use crate::resolve::{col_letter, resolve, Class};
use crate::scan::Scan;

/// Severity group. Also selects the glyph and the colour: Correctness and
/// TypeSafety warn with `!`, Structure notes with `·`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Correctness,
    TypeSafety,
    Structure,
}

impl Group {
    pub fn label(self) -> &'static str {
        match self {
            Group::Correctness => "correctness",
            Group::TypeSafety => "type safety",
            Group::Structure => "structure",
        }
    }
    pub fn glyph(self) -> char {
        match self {
            Group::Structure => '·',
            _ => '!',
        }
    }
}

pub struct Finding {
    pub group: Group,
    /// Stable machine tag for the --json view (e.g. "leading_zero").
    pub kind: &'static str,
    /// Column letter this finding is about, if it is column-scoped.
    pub column: Option<String>,
    pub subject: String,
    pub detail: String,
}

/// Whether a header names an identifier column — at a word boundary, so "Paid"
/// and "decode" don't match the way a bare `ends_with("id")` would. Matches
/// "id"/"code"/"key"/"no"/"number" as the whole name, after a separator, or as a
/// camelCase/UPPER suffix ("userId", "OrderID").
fn is_id_header(header: &str) -> bool {
    let h = header.trim();
    let hl = h.to_ascii_lowercase();
    const WORDS: [&str; 5] = ["id", "code", "key", "number", "no"];
    WORDS.iter().any(|w| {
        hl == *w
            || hl.ends_with(&format!("_{w}"))
            || hl.ends_with(&format!(" {w}"))
            || hl.ends_with(&format!("-{w}"))
    }) || h.ends_with("ID")
        || h.ends_with("Id")
        || h.ends_with("Code")
        || h.ends_with("No")
}

fn col_name(header: &str, letter: &str) -> String {
    if header.trim().is_empty() {
        format!("column {letter}")
    } else {
        format!("{header} ({letter})")
    }
}

pub fn findings(scan: &Scan) -> Vec<Finding> {
    let mut out = Vec::new();
    let width = scan.columns.len();

    // ---- correctness (row-level, not column-scoped) ----
    if scan.preamble > 0 {
        out.push(Finding {
            group: Group::Correctness,
            kind: "buried_header",
            column: None,
            subject: format!("buried header — row {}", scan.header_row),
            detail: format!(
                "row{} 1–{} {} preamble above the header; crop with xled before profiling",
                if scan.preamble == 1 { "" } else { "s" },
                scan.preamble,
                if scan.preamble == 1 { "is" } else { "are" },
            ),
        });
    }
    if !scan.ragged.is_empty() {
        let (row, fields) = scan.ragged[0];
        let more = if scan.ragged.len() > 1 {
            format!(" (+{} more)", scan.ragged.len() - 1)
        } else {
            String::new()
        };
        out.push(Finding {
            group: Group::Correctness,
            kind: "ragged_row",
            column: None,
            subject: format!("ragged row{}", if scan.ragged.len() == 1 { "" } else { "s" }),
            detail: format!(
                "row {row} has {fields} fields; table is {width} wide{more} — likely stray commas in an unquoted cell"
            ),
        });
    }
    for (row, sample) in &scan.total_rows {
        out.push(Finding {
            group: Group::Correctness,
            kind: "total_row",
            column: None,
            subject: format!("total row {row}"),
            detail: format!("pre-aggregated \"{sample}\"; a summary line, not data"),
        });
    }

    // ---- per-column: type safety + structure ----
    let mut seen_headers: Vec<String> = Vec::new();
    for (i, col) in scan.columns.iter().enumerate() {
        let letter = col_letter(i);
        let at = Some(letter.clone());
        let name = col_name(&col.header, &letter);
        let r = resolve(col);

        match r.class {
            Class::Empty => {
                out.push(Finding {
                    group: Group::Structure,
                    kind: if col.header.trim().is_empty() {
                        "spacer_column"
                    } else {
                        "empty_column"
                    },
                    column: at.clone(),
                    subject: if col.header.trim().is_empty() {
                        format!("spacer column {letter}")
                    } else {
                        format!("empty column {letter}")
                    },
                    detail: if col.header.trim().is_empty() {
                        "blank header, entirely empty".into()
                    } else {
                        format!("\"{}\" — entirely empty", col.header)
                    },
                });
                continue; // an empty column has nothing more to say
            }
            Class::LeadingZero => out.push(Finding {
                group: Group::TypeSafety,
                kind: "leading_zero",
                column: at.clone(),
                subject: format!("{name} is leading-zero text"),
                detail: format!("{}; a numeric cast strips the zeros", r.detail),
            }),
            Class::LongId => out.push(Finding {
                group: Group::TypeSafety,
                kind: "long_id",
                column: at.clone(),
                subject: format!("{name} is a long numeric ID"),
                detail: format!(
                    "{}; 16+ digits exceed exact number range — keep as text",
                    r.detail
                ),
            }),
            Class::Currency => {
                let noise = if r.float_noise {
                    " plus float-precision noise"
                } else {
                    ""
                };
                out.push(Finding {
                    group: Group::TypeSafety,
                    kind: "currency_text",
                    column: at.clone(),
                    subject: format!("{name} is currency text, not a number"),
                    detail: format!("$ and thousands commas{noise}; de-currency before math"),
                });
            }
            _ => {}
        }
        if r.bool_mixed {
            out.push(Finding {
                group: Group::TypeSafety,
                kind: "mixed_bool",
                column: at.clone(),
                subject: format!("{name} mixes boolean forms"),
                detail: format!("{} — normalize before logic", col.bool_reprs.join(" / ")),
            });
        }
        if r.mixed_nonnumeric > 0 {
            out.push(Finding {
                group: Group::TypeSafety,
                kind: "mixed_type",
                column: at.clone(),
                subject: format!("{name} mixes types"),
                detail: format!(
                    "{} numeric with {} non-numeric value{} — num() skips {}",
                    r.label.trim_end_matches(" · MIXED"),
                    r.mixed_nonnumeric,
                    if r.mixed_nonnumeric == 1 { "" } else { "s" },
                    if r.mixed_nonnumeric == 1 {
                        "it"
                    } else {
                        "them"
                    },
                ),
            });
        }

        // schema notes
        let distinct = col.distinct_count();
        let id_like = r.class == Class::LeadingZero || is_id_header(&col.header);

        // A candidate key is not a problem — it's useful context, so it lives in
        // the reading (see render), not this damage list. Constant and
        // duplicate-in-an-ID-column are mild hazards and stay.
        if distinct == 1 && col.nonblank > 1 {
            out.push(Finding {
                group: Group::Structure,
                kind: "constant_column",
                column: at.clone(),
                subject: format!("{name} is constant"),
                detail: format!("one value across {} rows", col.nonblank),
            });
        } else if id_like && distinct < col.nonblank && distinct * 10 >= col.nonblank * 9 {
            // Only when the column is *near*-unique (≥90% distinct): that reads
            // as a key with a few stray duplicates — a real hazard. A low-
            // cardinality id-like column is a repeating reference, not a broken
            // key, so it isn't flagged.
            let dups = col.nonblank - distinct;
            out.push(Finding {
                group: Group::Structure,
                kind: "duplicate_key",
                column: at.clone(),
                subject: format!("{name} looks like a key but has duplicates"),
                detail: format!(
                    "{dups} duplicate value{} across {} rows ({distinct} distinct)",
                    if dups == 1 { "" } else { "s" },
                    col.nonblank
                ),
            });
        }

        let fill = crate::resolve::fill_pct(col.nonblank, col.total);
        if fill > 0 && fill < 40 {
            out.push(Finding {
                group: Group::Structure,
                kind: "sparse_column",
                column: at.clone(),
                subject: format!("{name} is mostly blank"),
                detail: format!("{} of {} rows filled ({fill}%)", col.nonblank, col.total),
            });
        }

        // duplicate / blank header names
        let h = col.header.trim();
        if !h.is_empty() {
            let lower = h.to_ascii_lowercase();
            if seen_headers.contains(&lower) {
                out.push(Finding {
                    group: Group::Structure,
                    kind: "duplicate_header",
                    column: at.clone(),
                    subject: format!("duplicate header \"{h}\""),
                    detail: format!("column {letter} repeats an earlier header name"),
                });
            }
            seen_headers.push(lower);
        }
    }

    // stable order: correctness, then type safety, then structure; original
    // discovery order preserved within each group.
    out.sort_by_key(|f| match f.group {
        Group::Correctness => 0,
        Group::TypeSafety => 1,
        Group::Structure => 2,
    });
    out
}

/// One referral: a class of finding, the family tool that treats it, and — when
/// the repair reduces to a single invocation — the command to run.
pub struct Referral {
    pub trigger: String,
    pub tool: &'static str,
    pub action: String,
    /// A ready-to-run command, when one exists.
    ///
    /// Most referrals carry none, and that is not a gap to fill later. A command
    /// is only emitted where the repair is unambiguous from what xray can see:
    /// which boolean spelling should win, what a duplicate header ought to be
    /// renamed to, and whether a sparse column is merged cells or genuinely
    /// optional data are all judgements the profiler is not entitled to make.
    /// Emitting a plausible guess there would be worse than emitting nothing,
    /// because a command that runs is a command that gets trusted.
    ///
    /// The command reads rather than writes — no `-i`. xray never changes a
    /// byte, and it should not hand over something that does it by proxy; the
    /// preview is the step where the operator confirms the transform is right.
    pub command: Option<String>,
}

/// Quote a path for the emitted command when the shell would otherwise mangle it.
fn shell_quote(path: &str) -> String {
    if !path.is_empty()
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'))
    {
        path.to_string()
    } else {
        format!("'{}'", path.replace('\'', r"'\''"))
    }
}

/// How to address a column in an emitted command: by header name when it has
/// one, falling back to the spreadsheet letter when the name is blank or
/// carries the `]` that would close the bracket early.
fn address(header: &str, letter: &str) -> String {
    let h = header.trim();
    if h.is_empty() || h.contains(']') {
        letter.to_string()
    } else {
        format!("[{h}]")
    }
}

/// The opt-in referral (`--refer`): hand each class of finding to the family
/// tool that treats it, naming the column it is about and, where the repair is
/// a single unambiguous invocation, the command itself.
///
/// Off by default — the primary user already knows the family; this waits to be
/// asked. Empty when there is nothing to hand off. `path` is the input file as
/// the caller named it, and is `None` for piped stdin, where no command can be
/// written because there is no file to name.
pub fn referral(scan: &Scan, path: Option<&str>) -> Vec<Referral> {
    let mut refs = Vec::new();
    let file = path.map(shell_quote);

    // ---- structure: the table is not where the addresses say it is ----
    if scan.preamble > 0 {
        refs.push(Referral {
            trigger: format!("rows 1–{} above the header", scan.preamble),
            tool: "xled",
            action: format!(
                "read with --no-header, crop to the table, promote row {}",
                scan.header_row
            ),
            // Three interacting pieces (--no-header, a crop range, a header
            // promotion) whose correct spelling depends on where the data ends.
            // Left as prose deliberately: a wrong crop silently discards rows.
            command: None,
        });
    }
    if !scan.total_rows.is_empty() {
        let rows: Vec<String> = scan.total_rows.iter().map(|(r, _)| r.to_string()).collect();
        refs.push(Referral {
            trigger: format!(
                "pre-aggregated row{} {}",
                if rows.len() == 1 { "" } else { "s" },
                rows.join(", ")
            ),
            tool: "xled",
            action: "crop past the summary line before aggregating".into(),
            command: None,
        });
    }
    if !scan.ragged.is_empty() {
        refs.push(Referral {
            trigger: format!(
                "{} ragged row{}",
                scan.ragged.len(),
                if scan.ragged.len() == 1 { "" } else { "s" }
            ),
            tool: "xled",
            action: "repair the stray delimiters; every later address depends on the width".into(),
            command: None,
        });
    }

    // ---- per column ----
    let mut protect: Vec<String> = Vec::new();
    let mut trapped = false;
    for (i, col) in scan.columns.iter().enumerate() {
        let r = resolve(col);
        let letter = col_letter(i);
        let addr = address(&col.header, &letter);
        let label = col_name(&col.header, &letter);

        match r.class {
            Class::Currency => {
                trapped = true;
                refs.push(Referral {
                    trigger: format!("{label} is currency text"),
                    tool: "xled",
                    action: "strip the formatting before any math".into(),
                    command: file
                        .as_ref()
                        .map(|f| format!("xled '{addr} s/[$,]//g' {f}")),
                });
            }
            Class::LeadingZero | Class::LongId => protect.push(label.clone()),
            _ => {}
        }
        if r.mixed_nonnumeric > 0 || r.bool_mixed {
            trapped = true;
        }
    }

    // Collapsed into one line: the instruction is identical for every such
    // column, and it is the one case where the correct action is to do nothing.
    if !protect.is_empty() {
        refs.push(Referral {
            trigger: format!("{} stays text", protect.join(", ")),
            tool: "xled",
            action: "a numeric cast strips the zeros — cast at math time, not in the file".into(),
            command: None,
        });
    }
    if trapped {
        refs.push(Referral {
            trigger: "numbers trapped as text".into(),
            tool: "xql",
            action: "filter or aggregate once those columns are clean".into(),
            command: None,
        });
    }
    refs
}

/// One-line breakdown for the verdict header, e.g. "2 correctness · 3 type safety".
pub fn verdict(findings: &[Finding]) -> String {
    if findings.is_empty() {
        return "clean — nothing flagged".into();
    }
    let mut parts = Vec::new();
    for g in [Group::Correctness, Group::TypeSafety, Group::Structure] {
        let n = findings.iter().filter(|f| f.group == g).count();
        if n > 0 {
            parts.push(format!("{n} {}", g.label()));
        }
    }
    parts.join(" · ")
}
