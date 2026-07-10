//! The --json view: the same three registers as the human render, as structured
//! data. One model, two renderings — a machine reader branches on `class` and
//! finding `kind` instead of parsing prose. Always plain (no colour).

use serde_json::{json, Value};

use crate::findings;
use crate::resolve::{col_letter, resolve, Class};
use crate::scan::Scan;

fn top_values(col: &crate::scan::Column, n: usize) -> Vec<Value> {
    let mut pairs: Vec<(&String, &usize)> = col.freq.iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    pairs
        .iter()
        .take(n)
        .map(|(v, c)| json!({ "value": v, "count": c }))
        .collect()
}

pub fn to_json(name: &str, scan: &Scan, refer: bool) -> Value {
    // ---- reading ----
    let reading: Vec<Value> = scan
        .columns
        .iter()
        .enumerate()
        .map(|(i, col)| {
            let r = resolve(col);
            let fill_pct = if col.total == 0 {
                0
            } else {
                (col.nonblank * 100 + col.total / 2) / col.total
            };
            let key_eligible =
                matches!(r.class, Class::LeadingZero | Class::Int | Class::Text);
            let candidate_key = key_eligible
                && !col.distinct_capped
                && col.distinct_count() == col.nonblank
                && col.nonblank == scan.data_rows
                && scan.data_rows > 1;
            let top = if r.class == Class::Categorical {
                top_values(col, 10)
            } else {
                Vec::new()
            };
            json!({
                "letter": col_letter(i),
                "header": col.header,
                "type": r.label,
                "class": r.class.as_str(),
                "fill_pct": fill_pct,
                "nonblank": col.nonblank,
                "total": col.total,
                "distinct": col.distinct_count(),
                "distinct_capped": col.distinct_capped,
                "candidate_key": candidate_key,
                "flag": r.flag,
                "min": col.num_min,
                "max": col.num_max,
                "examples": col.examples,
                "top": top,
            })
        })
        .collect();

    // ---- findings ----
    let fs = findings::findings(scan);
    let findings_json: Vec<Value> = fs
        .iter()
        .map(|f| {
            json!({
                "group": f.group.label(),
                "kind": f.kind,
                "column": f.column,
                "subject": f.subject,
                "detail": f.detail,
            })
        })
        .collect();

    let mut root = json!({
        "file": name,
        "film": {
            "columns": scan.columns.len(),
            "rows": scan.data_rows,
            "bytes": scan.bytes,
            "delimiter": (scan.delimiter as char).to_string(),
            "encoding": if scan.utf8 { "utf-8" } else { "non-utf-8" },
            "bom": scan.bom,
            "line_endings": if scan.crlf { "CRLF" } else { "LF" },
            "header_row": scan.header_row,
            "preamble": scan.preamble,
            "ragged_rows": scan.ragged.len(),
        },
        "reading": reading,
        "findings": findings_json,
        "verdict": findings::verdict(&fs),
    });

    if refer {
        let refs: Vec<Value> = findings::referral(scan)
            .iter()
            .map(|r| json!({ "trigger": r.trigger, "tool": r.tool, "action": r.action }))
            .collect();
        root["referral"] = Value::Array(refs);
    }

    root
}
