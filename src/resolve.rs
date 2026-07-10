//! One shared read of a column's type, used by both the reading render and the
//! findings register so they never disagree. Stringly-typed: a value is text
//! until it unambiguously isn't, and leading zeros keep it text.

use crate::scan::{Column, Kind};

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

/// The resolved classification of a column.
#[derive(PartialEq, Eq)]
pub enum Class {
    Empty,
    LeadingZero,
    Currency,
    Bool,
    Int,
    Decimal,
    Categorical,
    Text,
}

impl Class {
    /// Stable machine name for the --json view.
    pub fn as_str(&self) -> &'static str {
        match self {
            Class::Empty => "empty",
            Class::LeadingZero => "leading_zero",
            Class::Currency => "currency",
            Class::Bool => "bool",
            Class::Int => "int",
            Class::Decimal => "decimal",
            Class::Categorical => "categorical",
            Class::Text => "text",
        }
    }
}

/// A column's resolved read: its type class and label, a one-line detail, and
/// the diagnostic facts the findings register needs.
pub struct Resolved {
    pub class: Class,
    pub label: String,
    pub detail: String,
    pub flag: Option<String>,
    pub mixed_nonnumeric: usize,
    pub bool_mixed: bool,
    pub float_noise: bool,
}

fn count(col: &Column, k: Kind) -> usize {
    col.kinds.get(&k).copied().unwrap_or(0)
}

fn trim_num(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

pub fn top_frequencies(col: &Column, n: usize) -> String {
    let mut pairs: Vec<(&String, &usize)> = col.freq.iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    pairs
        .iter()
        .take(n)
        .map(|(v, c)| format!("{} ×{}", v, c))
        .collect::<Vec<_>>()
        .join(" · ")
}

pub fn resolve(col: &Column) -> Resolved {
    let base = Resolved {
        class: Class::Text,
        label: "text".into(),
        detail: String::new(),
        flag: None,
        mixed_nonnumeric: 0,
        bool_mixed: false,
        float_noise: col.float_noise,
    };

    if col.nonblank == 0 {
        return Resolved {
            class: Class::Empty,
            label: "empty".into(),
            detail: "spacer column — entirely empty".into(),
            ..base
        };
    }

    let leading = count(col, Kind::LeadingZero);
    let ints = count(col, Kind::Int);
    let decimals = count(col, Kind::Decimal);
    let currency = count(col, Kind::Currency);
    let booleans = count(col, Kind::Bool);
    let text = count(col, Kind::Text);
    let numeric = ints + decimals;

    let examples = col.examples.join(", ");
    let num_range = || match (col.num_min, col.num_max) {
        (Some(a), Some(b)) => format!("{} … {}", trim_num(a), trim_num(b)),
        _ => examples.clone(),
    };

    // Leading zeros anywhere dominate the keep-as-text decision.
    if leading > 0 && leading + ints >= text {
        return Resolved {
            class: Class::LeadingZero,
            label: "text · leading-0".into(),
            detail: examples,
            flag: Some("keep as text".into()),
            ..base
        };
    }
    // Currency-formatted numbers are text, not numbers.
    if currency > 0 && currency >= numeric && currency >= text {
        let mut flag = String::from("not numeric");
        if col.float_noise {
            flag.push_str(" · float-noise");
        }
        return Resolved {
            class: Class::Currency,
            label: "text · currency".into(),
            detail: examples,
            flag: Some(flag),
            ..base
        };
    }
    // Booleans, flagged when the file mixes representations (Y / yes / true).
    if booleans > 0 && booleans >= numeric && booleans >= text {
        let mixed = col.bool_reprs.len() > 1;
        return Resolved {
            class: Class::Bool,
            label: if mixed { "bool · mixed-repr" } else { "bool" }.into(),
            detail: col.bool_reprs.join(" · "),
            bool_mixed: mixed,
            ..base
        };
    }
    // Numeric-dominant. Any non-blank text is a mixed-type hazard.
    if numeric > 0 && numeric >= text {
        let (class, name) = if decimals > ints {
            (Class::Decimal, "decimal")
        } else {
            (Class::Int, "int")
        };
        if text > 0 {
            return Resolved {
                class,
                label: format!("{name} · MIXED"),
                detail: num_range(),
                flag: Some(format!(
                    "{text} non-numeric value{}",
                    if text == 1 { "" } else { "s" }
                )),
                mixed_nonnumeric: text,
                ..base
            };
        }
        return Resolved {
            class,
            label: name.into(),
            detail: num_range(),
            ..base
        };
    }
    // Text. Low cardinality reads as categorical and gets a top-N breakdown.
    let distinct = col.distinct_count();
    if !col.distinct_capped && distinct <= 20 && distinct * 2 <= col.nonblank {
        return Resolved {
            class: Class::Categorical,
            label: "text · categorical".into(),
            detail: top_frequencies(col, 4),
            ..base
        };
    }
    Resolved {
        detail: examples,
        ..base
    }
}
