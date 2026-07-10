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
                    if r.mixed_nonnumeric == 1 { "it" } else { "them" },
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

        let fill = if col.total == 0 {
            100
        } else {
            col.nonblank * 100 / col.total
        };
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

/// One referral: a class of finding and the family tool that treats it.
pub struct Referral {
    pub trigger: &'static str,
    pub tool: &'static str,
    pub action: &'static str,
}

/// The opt-in referral (`--refer`): map the findings present to the family tool
/// that treats them. Off by default — the primary user already knows the family;
/// this waits to be asked. Empty when there is nothing to hand off.
pub fn referral(scan: &Scan) -> Vec<Referral> {
    let has_rows = !scan.ragged.is_empty() || !scan.total_rows.is_empty();
    let mut spacer = false;
    let mut leading = false;
    let mut currency = false;
    let mut mixed = false;
    for col in &scan.columns {
        let r = resolve(col);
        match r.class {
            Class::LeadingZero => leading = true,
            Class::Currency => currency = true,
            Class::Empty if col.header.trim().is_empty() => spacer = true,
            _ => {}
        }
        if r.mixed_nonnumeric > 0 || r.bool_mixed {
            mixed = true;
        }
    }

    let mut refs = Vec::new();
    if has_rows || spacer {
        refs.push(Referral {
            trigger: "ragged / total / spacer rows",
            tool: "xled",
            action: "crop to the real table, drop the summary line",
        });
    }
    if leading || currency {
        refs.push(Referral {
            trigger: "leading-zero / currency text",
            tool: "xled",
            action: "keep IDs as text; round(num(),2) only at math time",
        });
    }
    if currency || mixed {
        refs.push(Referral {
            trigger: "numbers trapped as text",
            tool: "xql",
            action: "filter or aggregate once those columns are clean",
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
