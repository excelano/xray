//! Human render of the film, reading, and findings registers. Plain text for
//! now; colour (anstyle/anstream) and --refer / --json layer on later.

use crate::findings::{self, Group};
use crate::resolve::{col_letter, resolve, Class};
use crate::scan::Scan;
use crate::theme::{self, paint};

fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn delim_name(d: u8) -> String {
    match d {
        b',' => "comma".into(),
        b'\t' => "tab".into(),
        b';' => "semicolon".into(),
        b'|' => "pipe".into(),
        other => format!("{:?}", other as char),
    }
}

pub fn render(name: &str, scan: &Scan, refer: bool) -> String {
    let mut out = String::new();
    out.push_str(&format!("{} · {name}\n\n", paint(theme::HEADER, "xray")));

    // ---- FILM ----
    out.push_str(&paint(theme::HEADER, "FILM"));
    out.push('\n');
    let header_desc = match scan.header_row {
        0 => "header: none".to_string(),
        1 => "header: row 1".to_string(),
        n => format!(
            "header: row {n} ({} preamble row{})",
            scan.preamble,
            if scan.preamble == 1 { "" } else { "s" }
        ),
    };
    out.push_str(&format!(
        "  {} columns × {} rows       {}       {}\n",
        scan.columns.len(),
        scan.data_rows,
        header_desc,
        human_size(scan.bytes),
    ));
    out.push_str(&format!(
        "  delimiter {}   encoding {}{}   line endings {}\n",
        delim_name(scan.delimiter),
        if scan.utf8 { "utf-8" } else { "non-utf-8" },
        if scan.bom { " (BOM)" } else { "" },
        if scan.crlf { "CRLF" } else { "LF" },
    ));

    // ---- READING ----
    out.push('\n');
    out.push_str(&paint(theme::HEADER, "READING"));
    out.push('\n');
    let reads: Vec<_> = scan.columns.iter().map(resolve).collect();
    let name_w = scan
        .columns
        .iter()
        .map(|c| c.header.chars().count().max(1))
        .max()
        .unwrap_or(6)
        .max(6);
    let ty_w = reads
        .iter()
        .map(|r| r.label.len())
        .max()
        .unwrap_or(4)
        .max(4);

    out.push_str(&format!(
        "  col  {:<name_w$}  {:<ty_w$}  fill  distinct  detail\n",
        "header", "type",
    ));
    for (i, (col, read)) in scan.columns.iter().zip(&reads).enumerate() {
        let header = if col.header.trim().is_empty() {
            "‹blank›".to_string()
        } else {
            col.header.clone()
        };
        let fill = crate::resolve::fill_pct(col.nonblank, col.total);
        let distinct = if col.distinct_capped {
            format!("{}+", col.distinct_count())
        } else {
            col.distinct_count().to_string()
        };
        // A candidate key is useful context, not a problem — surface it here in
        // the reading rather than in the findings damage list.
        let key_eligible = matches!(
            read.class,
            Class::LeadingZero | Class::LongId | Class::Int | Class::Text
        );
        let is_key = key_eligible
            && !col.distinct_capped
            && col.distinct_count() == col.nonblank
            && col.nonblank == scan.data_rows
            && scan.data_rows > 1;
        let base_detail = if is_key {
            format!("{} · unique key", read.detail)
        } else {
            read.detail.clone()
        };
        // Colour only the flag (line end) and the letter (padded outside the
        // colour), never inside the width-aligned fields — so alignment holds
        // whether anstream keeps the codes or strips them.
        let detail = match &read.flag {
            Some(f) => format!(
                "{:<24}  {}",
                base_detail,
                paint(theme::WARN, &format!("! {f}"))
            ),
            None => base_detail,
        };
        let letter = col_letter(i);
        let letter_cell = format!(
            "{}{}",
            paint(theme::ACCENT, &letter),
            " ".repeat(3usize.saturating_sub(letter.chars().count())),
        );
        out.push_str(&format!(
            "  {}  {:<name_w$}  {:<ty_w$}  {:>3}%  {:>8}  {}\n",
            letter_cell,
            header,
            read.label,
            fill,
            distinct,
            detail.trim_end(),
        ));
    }

    // ---- FINDINGS ----
    let fs = findings::findings(scan);
    out.push('\n');
    out.push_str(&format!(
        "{}  ({})   {}\n",
        paint(theme::HEADER, "FINDINGS"),
        fs.len(),
        paint(theme::FAINT, &findings::verdict(&fs)),
    ));
    let mut group: Option<Group> = None;
    for (n, f) in fs.iter().enumerate() {
        if group != Some(f.group) {
            out.push_str(&format!("  {}\n", paint(theme::FAINT, f.group.label())));
            group = Some(f.group);
        }
        let sev = match f.group {
            Group::Correctness => theme::CRIT,
            Group::TypeSafety => theme::WARN,
            Group::Structure => theme::NOTE,
        };
        out.push_str(&format!(
            "  {:>2}  {} — {}\n",
            n + 1,
            paint(sev, &format!("{} {}", f.group.glyph(), f.subject)),
            f.detail,
        ));
    }

    // ---- REFERRAL (opt-in) ----
    if refer {
        let refs = findings::referral(scan);
        if !refs.is_empty() {
            out.push('\n');
            out.push_str(&paint(theme::HEADER, "REFERRAL"));
            out.push('\n');
            for r in refs {
                out.push_str(&format!(
                    "  {:<32}→ {:<6} {}\n",
                    r.trigger, r.tool, r.action
                ));
            }
        }
    }

    out
}
