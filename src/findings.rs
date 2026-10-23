// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

//! The findings register: the diagnostic problem list, ranked most-severe
//! first. Reports damage; never fixes it (that's xled) and never filters it
//! (that's xql). Every finding names what will bite a later step.
//!
//! Grounded in the corpus taxonomy (`~/xled-corpus/CORPUS-FINDINGS.md`). Buried
//! headers are detected in the scan; stacked/side-by-side tables,
//! smart-punct/HTML-entities, and multi-value newline cells are layered in
//! during the corpus-tuning pass.

use std::path::Path;

use crate::resolve::{col_letter, date_layout, resolve, Class, DateLayout};
use crate::scan::{Column, LineEndings, Scan};

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
    /// Snake-case form of the label, for JSON keys that must be stable.
    pub fn key(self) -> &'static str {
        match self {
            Group::Correctness => "correctness",
            Group::TypeSafety => "type_safety",
            Group::Structure => "structure",
        }
    }
    /// The two-level severity the glyph already encodes: `!` warns, `·` notes.
    /// Spelled out for a machine reader so it can triage without a glyph table.
    pub fn severity(self) -> &'static str {
        match self {
            Group::Structure => "note",
            _ => "warn",
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

/// The rows above a buried header: "row 1" or "rows 1–3".
fn preamble_rows(n: usize) -> String {
    if n == 1 {
        "row 1".into()
    } else {
        format!("rows 1–{n}")
    }
}

/// The same span as the subject of a sentence: "row 1 is" or "rows 1–3 are".
fn preamble_span(n: usize) -> String {
    format!("{} {}", preamble_rows(n), if n == 1 { "is" } else { "are" })
}

/// A column as findings and referrals name it, on one line: a header holding
/// an Alt+Enter break shows it as ⏎.
fn col_name(header: &str, letter: &str) -> String {
    if header.trim().is_empty() {
        format!("column {letter}")
    } else {
        let flat = header.replace("\r\n", "⏎").replace(['\r', '\n'], "⏎");
        format!("{flat} ({letter})")
    }
}

/// A cell value as a finding quotes it: in quotes, so padding shows, and on
/// one line.
fn quoted(value: &str) -> String {
    format!(
        "\"{}\"",
        value.replace("\r\n", "⏎").replace(['\r', '\n'], "⏎")
    )
}

/// Groups of values that differ only in case, most frequent spelling first,
/// most frequent group first. Padding is set aside first, so `Acme ` and
/// `acme` count as a case variant. An ID column, and any value holding a
/// digit, is left out: `001A000001abcDE` and `001A000001ABCde` are two
/// case-sensitive keys, not one spelled twice.
fn case_variants(col: &Column) -> Vec<Vec<String>> {
    if is_id_header(&col.header) {
        return Vec::new();
    }
    let mut groups: std::collections::HashMap<String, Vec<(&str, usize)>> =
        std::collections::HashMap::new();
    for (value, n) in &col.freq {
        let v = value.trim();
        if v.bytes().any(|b| b.is_ascii_digit()) {
            continue;
        }
        let forms = groups.entry(v.to_lowercase()).or_default();
        match forms.iter_mut().find(|(f, _)| *f == v) {
            Some((_, total)) => *total += n,
            None => forms.push((v, *n)),
        }
    }
    let mut out: Vec<(usize, Vec<String>)> = groups
        .into_values()
        .filter(|forms| forms.len() > 1)
        .map(|mut forms| {
            forms.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
            let total = forms.iter().map(|(_, n)| n).sum();
            (
                total,
                forms.into_iter().map(|(f, _)| f.to_string()).collect(),
            )
        })
        .collect();
    out.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    out.into_iter().map(|(_, forms)| forms).collect()
}

/// The xled format a date column converts under, when one format reads every
/// non-ISO cell: a proven order, one separator, one year width, no times.
fn date_format(col: &Column) -> Option<String> {
    let d = &col.dates;
    if d.timed {
        return None;
    }
    let (a, b) = match date_layout(col) {
        DateLayout::DayFirst => ("D", "M"),
        DateLayout::MonthFirst => ("M", "D"),
        _ => return None,
    };
    let ([sep], [digits]) = (&d.separators[..], &d.year_digits[..]) else {
        return None;
    };
    let sep = *sep as char;
    let year = if *digits == 4 { "YYYY" } else { "YY" };
    Some(format!("{a}{sep}{b}{sep}{year}"))
}

pub fn findings(scan: &Scan) -> Vec<Finding> {
    let mut out = Vec::new();
    let width = scan.columns.len();

    // ---- correctness (row-level, not column-scoped) ----
    // Nothing under the header is the one finding that pre-empts the rest: with
    // no rows every column is empty, and a column-by-column list of that would
    // bury the only fact that matters.
    if scan.data_rows == 0 {
        out.push(Finding {
            group: Group::Correctness,
            kind: "no_data",
            column: None,
            subject: "no data rows".into(),
            detail: if scan.columns.is_empty() {
                "the input is empty".into()
            } else {
                format!(
                    "a header of {width} column{} and nothing under it",
                    if width == 1 { "" } else { "s" }
                )
            },
        });
        return out;
    }
    if !scan.utf8 {
        out.push(Finding {
            group: Group::Correctness,
            kind: "non_utf8",
            column: None,
            subject: "not UTF-8".into(),
            detail: "bytes that are not valid UTF-8, shown here as �; likely a Windows-1252 export — transcode before anything reads it".into(),
        });
    }
    if scan.preamble > 0 {
        out.push(Finding {
            group: Group::Correctness,
            kind: "buried_header",
            column: None,
            subject: format!("buried header — row {}", scan.header_row),
            detail: format!(
                "{} preamble above the header; xled must read it with --no-header (without it every row address is off by one), crop from row {} and promote it",
                preamble_span(scan.preamble),
                scan.header_row,
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

    // ---- structure (row-level) ----
    if let LineEndings::Mixed { crlf, lf } = scan.line_endings {
        out.push(Finding {
            group: Group::Structure,
            kind: "mixed_line_endings",
            column: None,
            subject: "mixed line endings".into(),
            detail: format!(
                "{crlf} row{} end CRLF and {lf} end LF — a line-based tool sees a stray carriage return on some rows",
                if crlf == 1 { "" } else { "s" }
            ),
        });
    }
    if scan.bom {
        out.push(Finding {
            group: Group::Structure,
            kind: "bom",
            column: None,
            subject: "UTF-8 BOM".into(),
            detail: "three invisible bytes before the header; a reader that does not strip them glues U+FEFF onto column A's name".into(),
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
            Class::Scientific => out.push(Finding {
                group: Group::TypeSafety,
                kind: "scientific_notation",
                column: at.clone(),
                subject: format!("{name} is in E-notation"),
                detail: format!(
                    "{}; Excel writes a long number this way on export and the trailing digits are gone — an ID here is unrecoverable",
                    r.detail
                ),
            }),
            Class::Date => {
                let d = &col.dates;
                let layout = date_layout(col);
                let example = d.example.as_deref().unwrap_or_default();
                let finding = match layout {
                    DateLayout::Iso => None,
                    DateLayout::DayFirst | DateLayout::MonthFirst => {
                        let (proof, puts, other) = if layout == DateLayout::DayFirst {
                            (&d.day_first, "day", "M/D")
                        } else {
                            (&d.month_first, "month", "D/M")
                        };
                        Some((
                            "non_iso_date",
                            format!("{name} is {} dates, not ISO", layout.label()),
                            format!(
                                "{} puts the {puts} first; a reader that assumes {other} swaps day and month or rejects the cell — convert to ISO",
                                proof.as_deref().unwrap_or_default()
                            ),
                        ))
                    }
                    DateLayout::Ambiguous => Some((
                        "ambiguous_date",
                        format!("{name} has dates that read two ways"),
                        format!(
                            "no day or month is over 12, so {example} is both D/M and M/D — confirm the layout at the source before parsing"
                        ),
                    )),
                    DateLayout::Mixed => {
                        let detail = match (&d.day_first, &d.month_first) {
                            (Some(dm), Some(md)) => format!(
                                "{dm} puts the day first and {md} the month — two sources merged; one parse reads half of them wrong"
                            ),
                            _ => format!(
                                "{} ISO cell{} beside {} like {example}; one parse reads only one layout",
                                d.iso,
                                if d.iso == 1 { "" } else { "s" },
                                if d.numeric == 1 { "one" } else { "others" },
                            ),
                        };
                        Some((
                            "mixed_date_format",
                            format!("{name} mixes date layouts"),
                            detail,
                        ))
                    }
                };
                if let Some((kind, subject, detail)) = finding {
                    out.push(Finding {
                        group: Group::TypeSafety,
                        kind,
                        column: at.clone(),
                        subject,
                        detail,
                    });
                }
            }
            Class::Currency => {
                let seen = match (col.currency_symbol, col.currency_grouping) {
                    (true, true) => "$ and thousands commas",
                    (true, false) => "a $ sign",
                    _ => "thousands commas",
                };
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
                    detail: format!("{seen}{noise}; de-currency before math"),
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
        if r.mixed > 0 {
            let n = r.mixed;
            let (s, them) = if n == 1 { ("", "it") } else { ("s", "them") };
            let detail = if r.class == Class::Bool {
                format!("boolean with {n} non-boolean value{s} — a stray sentinel breaks logic; normalize {them} first")
            } else if r.class == Class::Date {
                format!("date with {n} non-date value{s} — a date parse skips {them}")
            } else {
                format!(
                    "{} numeric with {n} non-numeric value{s} — num() skips {them}",
                    r.class.as_str()
                )
            };
            out.push(Finding {
                group: Group::TypeSafety,
                kind: "mixed_type",
                column: at.clone(),
                subject: format!("{name} mixes types"),
                detail,
            });
        }

        if col.padded > 0 {
            let n = col.padded;
            out.push(Finding {
                group: Group::TypeSafety,
                kind: "whitespace_pad",
                column: at.clone(),
                subject: format!("{name} has padded values"),
                detail: format!(
                    "{n} value{} with leading or trailing whitespace, like {} — an equality test or join misses {}",
                    if n == 1 { "" } else { "s" },
                    quoted(col.pad_example.as_deref().unwrap_or_default()),
                    if n == 1 { "it" } else { "them" },
                ),
            });
        }
        if matches!(r.class, Class::Text | Class::Categorical) {
            let variants = case_variants(col);
            if let Some(first) = variants.first() {
                let shown: Vec<String> = first.iter().map(|v| quoted(v)).collect();
                let more = if variants.len() > 1 {
                    format!(" (+{} more)", variants.len() - 1)
                } else {
                    String::new()
                };
                out.push(Finding {
                    group: Group::TypeSafety,
                    kind: "case_variant",
                    column: at.clone(),
                    subject: format!("{name} spells a value more than one way"),
                    detail: format!(
                        "{}{more} differ only in case — a group-by or join splits them",
                        shown.join(" / ")
                    ),
                });
            }
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

/// Wrap `s` in single quotes, closing and reopening around any quote it holds.
fn single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
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
        single_quote(path)
    }
}

/// An xled invocation that runs `script` against the table as xray read it.
///
/// xled assumes a comma (tab for a `.tsv`) and a header on row 1, so the
/// command carries `-d` when the delimiter differs, and `--no-header` when the
/// header is elsewhere or absent. Over a buried header the script first crops
/// to the table and promotes its header, so a column named in `script` exists
/// by the time it runs. The crop runs to the last row (`$`) and discards only
/// the preamble; a total row below the table stays for its own referral.
fn xled_command(scan: &Scan, path: &str, script: &[&str]) -> String {
    let assumed = match Path::new(path).extension().and_then(|e| e.to_str()) {
        Some("tsv") => b'\t',
        _ => b',',
    };
    let mut cmd = String::from("xled");
    match scan.delimiter {
        d if d == assumed => {}
        b'\t' => cmd.push_str(r" -d '\t'"),
        d => cmd.push_str(&format!(" -d {}", shell_quote(&(d as char).to_string()))),
    }
    let mut lines = Vec::new();
    if scan.header_row != 1 {
        cmd.push_str(" --no-header");
    }
    if scan.preamble > 0 {
        lines.push(format!("{}:$ crop", scan.header_row));
        lines.push("1 header".into());
    }
    lines.extend(script.iter().map(|l| l.to_string()));
    format!(
        "{cmd} {} {}",
        single_quote(&lines.join("\n")),
        shell_quote(path)
    )
}

/// How to address a column in an emitted command: by header name when it has
/// one, falling back to the spreadsheet letter when the name is blank or
/// carries the `]` that would close the bracket early or a line break that
/// would end the xled command mid-name. The name goes in as the
/// file spells it, padding included: xled matches it exactly, and an
/// assignment to a near miss creates a new column instead of failing.
fn address(header: &str, letter: &str) -> String {
    if header.trim().is_empty() || header.contains([']', '\n', '\r']) {
        letter.to_string()
    } else {
        format!("[{header}]")
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

    // ---- structure: the table is not where the addresses say it is ----
    if scan.preamble > 0 {
        refs.push(Referral {
            trigger: format!("{} above the header", preamble_rows(scan.preamble)),
            tool: "xled",
            action: format!(
                "read with --no-header, crop to the table, promote row {}",
                scan.header_row
            ),
            command: path.map(|p| xled_command(scan, p, &[])),
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
    let mut leading_zero: Vec<String> = Vec::new();
    let mut long_id: Vec<String> = Vec::new();
    let mut padded: Vec<(String, String)> = Vec::new();
    let mut cased: Vec<String> = Vec::new();
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
                    command: path.map(|p| xled_command(scan, p, &[&format!("{addr} s/[$,]//g")])),
                });
            }
            Class::Date => {
                let layout = date_layout(col);
                let (trigger, action) = match layout {
                    DateLayout::Iso => (None, ""),
                    DateLayout::DayFirst | DateLayout::MonthFirst => (
                        Some(format!("{label} is {} dates", layout.label())),
                        "convert to ISO with date() under that layout",
                    ),
                    DateLayout::Ambiguous => (
                        Some(format!("{label} reads D/M or M/D")),
                        "confirm the layout at the source, then convert with date()",
                    ),
                    DateLayout::Mixed => (
                        Some(format!("{label} mixes date layouts")),
                        "separate the layouts first; date() reads one at a time",
                    ),
                };
                if let Some(trigger) = trigger {
                    // An expression takes a column only by bracketed name.
                    let script = date_format(col)
                        .filter(|_| addr.starts_with('['))
                        .map(|fmt| format!("{addr} = date({addr}, \"{fmt}\")"));
                    refs.push(Referral {
                        trigger,
                        tool: "xled",
                        action: action.into(),
                        command: script
                            .and_then(|script| path.map(|p| xled_command(scan, p, &[&script]))),
                    });
                }
            }
            Class::LeadingZero => leading_zero.push(label.clone()),
            Class::LongId => long_id.push(label.clone()),
            _ => {}
        }
        if col.padded > 0 {
            padded.push((label.clone(), addr.clone()));
        }
        if matches!(r.class, Class::Text | Class::Categorical) && !case_variants(col).is_empty() {
            cased.push(label.clone());
        }
        if (r.mixed > 0 && r.class != Class::Date) || r.bool_mixed {
            trapped = true;
        }
    }

    // Collapsed into one line per reason: the instruction is identical for
    // every such column, and it is the one case where the correct action is to
    // do nothing. The two reasons stay apart because a cast damages them
    // differently — it strips a leading zero, but rounds a 16+-digit ID.
    if !leading_zero.is_empty() {
        refs.push(Referral {
            trigger: format!("{} stays text", leading_zero.join(", ")),
            tool: "xled",
            action: "a numeric cast strips the zeros — cast at math time, not in the file".into(),
            command: None,
        });
    }
    if !long_id.is_empty() {
        refs.push(Referral {
            trigger: format!("{} stays text", long_id.join(", ")),
            tool: "xled",
            action: "a numeric cast rounds the digits away — keep it text, never cast it".into(),
            command: None,
        });
    }
    // One trim script for every padded column: the repair is the same and
    // unambiguous for each, so one invocation does all of it.
    if !padded.is_empty() {
        let labels: Vec<&str> = padded.iter().map(|(l, _)| l.as_str()).collect();
        let script: Vec<String> = padded
            .iter()
            .map(|(_, a)| format!(r"{a} s/^\s+|\s+$//g"))
            .collect();
        let script: Vec<&str> = script.iter().map(String::as_str).collect();
        refs.push(Referral {
            trigger: format!("{} padded", labels.join(", ")),
            tool: "xled",
            action: "trim before any join or comparison".into(),
            command: path.map(|p| xled_command(scan, p, &script)),
        });
    }
    if !cased.is_empty() {
        refs.push(Referral {
            trigger: format!("{} case variants", cased.join(", ")),
            tool: "xled",
            action: "choose the spelling, then normalize with s/// or upper()/lower()".into(),
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

/// The order the groups are reported in, most severe first.
pub const GROUPS: [Group; 3] = [Group::Correctness, Group::TypeSafety, Group::Structure];

/// One-line breakdown for the verdict header, e.g. "2 correctness · 3 type safety".
pub fn verdict(findings: &[Finding]) -> String {
    if findings.is_empty() {
        return "clean — nothing flagged".into();
    }
    let mut parts = Vec::new();
    for g in GROUPS {
        let n = findings.iter().filter(|f| f.group == g).count();
        if n > 0 {
            parts.push(format!("{n} {}", g.label()));
        }
    }
    parts.join(" · ")
}
