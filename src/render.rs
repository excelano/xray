//! Human render of the film, reading, and findings registers. Plain text for
//! now; colour (anstyle/anstream) and --refer / --json layer on later.

use crate::findings::{self, Group};
use crate::resolve::{col_letter, resolve, Class};
use crate::scan::Scan;

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

pub fn render(name: &str, scan: &Scan) -> String {
    let mut out = String::new();
    out.push_str(&format!("xray · {name}\n\n"));

    // ---- FILM ----
    out.push_str("FILM\n");
    out.push_str(&format!(
        "  {} columns × {} rows       header: row 1       {}\n",
        scan.columns.len(),
        scan.data_rows,
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
    out.push_str("\nREADING\n");
    let reads: Vec<_> = scan.columns.iter().map(resolve).collect();
    let name_w = scan
        .columns
        .iter()
        .map(|c| c.header.chars().count().max(1))
        .max()
        .unwrap_or(6)
        .max(6);
    let ty_w = reads.iter().map(|r| r.label.len()).max().unwrap_or(4).max(4);

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
        let fill = if col.total == 0 {
            0
        } else {
            (col.nonblank * 100 + col.total / 2) / col.total
        };
        let distinct = if col.distinct_capped {
            format!("{}+", col.distinct_count())
        } else {
            col.distinct_count().to_string()
        };
        // A candidate key is useful context, not a problem — surface it here in
        // the reading rather than in the findings damage list.
        let key_eligible = matches!(read.class, Class::LeadingZero | Class::Int | Class::Text);
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
        let detail = match &read.flag {
            Some(f) => format!("{:<24}  ! {}", base_detail, f),
            None => base_detail,
        };
        out.push_str(&format!(
            "  {:<3}  {:<name_w$}  {:<ty_w$}  {:>3}%  {:>8}  {}\n",
            col_letter(i),
            header,
            read.label,
            fill,
            distinct,
            detail.trim_end(),
        ));
    }

    // ---- FINDINGS ----
    let fs = findings::findings(scan);
    out.push_str(&format!(
        "\nFINDINGS  ({})   {}\n",
        fs.len(),
        findings::verdict(&fs),
    ));
    let mut group: Option<Group> = None;
    for (n, f) in fs.iter().enumerate() {
        if group != Some(f.group) {
            out.push_str(&format!("  {}\n", f.group.label()));
            group = Some(f.group);
        }
        out.push_str(&format!(
            "  {:>2}  {} {} — {}\n",
            n + 1,
            f.group.glyph(),
            f.subject,
            f.detail,
        ));
    }

    out
}
