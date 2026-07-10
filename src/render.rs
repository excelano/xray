//! Human render of the film and reading registers. Plain text for now;
//! colour (anstyle/anstream) and the findings register layer on later.

use crate::scan::{Column, Kind, Scan};

/// Spreadsheet column letter for a zero-based index: 0→A, 25→Z, 26→AA.
pub fn col_letter(mut idx: usize) -> String {
    let mut s = Vec::new();
    loop {
        s.push(b'A' + (idx % 26) as u8);
        if idx < 26 {
            break;
        }
        idx = idx / 26 - 1;
    }
    s.reverse();
    String::from_utf8(s).unwrap()
}

/// The resolved read of a column: its type label, a one-line detail, and an
/// optional inline `!` flag (the type-safety warnings surfaced in the reading).
struct Read {
    ty: String,
    detail: String,
    flag: Option<String>,
}

fn count(col: &Column, k: Kind) -> usize {
    col.kinds.get(&k).copied().unwrap_or(0)
}

fn top_frequencies(col: &Column, n: usize) -> String {
    let mut pairs: Vec<(&String, &usize)> = col.freq.iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    pairs
        .iter()
        .take(n)
        .map(|(v, c)| format!("{} ×{}", v, c))
        .collect::<Vec<_>>()
        .join(" · ")
}

fn resolve(col: &Column) -> Read {
    if col.nonblank == 0 {
        return Read {
            ty: "empty".into(),
            detail: "spacer column — entirely empty".into(),
            flag: None,
        };
    }

    let leading = count(col, Kind::LeadingZero);
    let ints = count(col, Kind::Int);
    let decimals = count(col, Kind::Decimal);
    let currency = count(col, Kind::Currency);
    let booleans = count(col, Kind::Bool);
    let text = count(col, Kind::Text);
    let numeric = ints + decimals;

    let examples = col.examples.join(" … ");
    let num_range = || match (col.num_min, col.num_max) {
        (Some(a), Some(b)) => format!("{} … {}", trim_num(a), trim_num(b)),
        _ => examples.clone(),
    };

    // Leading zeros anywhere dominate the keep-as-text decision.
    if leading > 0 && leading + ints >= text {
        return Read {
            ty: "text · leading-0".into(),
            detail: examples,
            flag: Some("keep as text".into()),
        };
    }
    // Currency-formatted numbers are text, not numbers.
    if currency > 0 && currency >= numeric && currency >= text {
        let mut flag = String::from("not numeric");
        if col.float_noise {
            flag.push_str(" · float-noise");
        }
        return Read {
            ty: "text · currency".into(),
            detail: examples,
            flag: Some(flag),
        };
    }
    // Booleans, flagged when the file mixes representations (Y / yes / true).
    if booleans >= numeric && booleans >= text && booleans > 0 {
        let ty = if col.bool_reprs.len() > 1 {
            "bool · mixed-repr"
        } else {
            "bool"
        };
        return Read {
            ty: ty.into(),
            detail: col.bool_reprs.join(" · "),
            flag: None,
        };
    }
    // Numeric-dominant. Any non-blank text is a mixed-type hazard.
    if numeric > 0 && numeric >= text {
        let base = if decimals > ints { "decimal" } else { "int" };
        if text > 0 {
            return Read {
                ty: format!("{base} · MIXED"),
                detail: num_range(),
                flag: Some(format!(
                    "{text} non-numeric value{}",
                    if text == 1 { "" } else { "s" }
                )),
            };
        }
        return Read {
            ty: base.into(),
            detail: num_range(),
            flag: None,
        };
    }
    // Text. Low cardinality reads as categorical and gets a top-N breakdown.
    let distinct = col.distinct_count();
    if !col.distinct_capped && distinct <= 20 && distinct * 2 <= col.nonblank {
        return Read {
            ty: "text · categorical".into(),
            detail: top_frequencies(col, 4),
            flag: None,
        };
    }
    Read {
        ty: "text".into(),
        detail: examples,
        flag: None,
    }
}

fn trim_num(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

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
    if let Some(&(row, fields)) = scan.ragged.first() {
        out.push_str(&format!(
            "  ! {} ragged row{} — row {} has {} fields; table is {} wide\n",
            scan.ragged.len(),
            if scan.ragged.len() == 1 { "" } else { "s" },
            row,
            fields,
            scan.columns.len(),
        ));
    }

    // ---- READING ----
    out.push_str("\nREADING\n");
    let reads: Vec<Read> = scan.columns.iter().map(resolve).collect();
    let name_w = scan
        .columns
        .iter()
        .map(|c| c.header.chars().count().max(1))
        .max()
        .unwrap_or(6)
        .max(6);
    let ty_w = reads.iter().map(|r| r.ty.len()).max().unwrap_or(4).max(4);

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
        let detail = match &read.flag {
            Some(f) => format!("{:<24}  ! {}", read.detail, f),
            None => read.detail.clone(),
        };
        out.push_str(&format!(
            "  {:<3}  {:<name_w$}  {:<ty_w$}  {:>3}%  {:>8}  {}\n",
            col_letter(i),
            header,
            read.ty,
            fill,
            distinct,
            detail.trim_end(),
        ));
    }

    out
}
