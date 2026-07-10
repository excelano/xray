//! Single streaming pass over a delimited file, accumulating a fixed profile.
//!
//! Read-only and O(1)-ish in memory: per-column accumulators plus distinct and
//! frequency sets bounded by a cardinality cap, so xray profiles files far
//! larger than xled can hold. Nothing here mutates or filters — it observes.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Distinct values tracked exactly up to this many per column; beyond it the
/// count is reported as `CAP+`. A corpus-tuning knob (DESIGN.md, open item 3).
pub const CARDINALITY_CAP: usize = 10_000;

/// How a single non-blank cell classifies. Stringly-typed throughout: a value
/// is text until it unambiguously isn't, and a leading zero keeps it text.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Kind {
    Int,
    LeadingZero,
    Decimal,
    Currency,
    Bool,
    Text,
}

/// Classify one raw cell value (already known non-blank).
pub fn classify(raw: &str) -> Kind {
    let s = raw.trim();
    if s.is_empty() {
        return Kind::Text;
    }

    // Currency: a leading $ or thousands-separated digits like 1,200.00.
    if looks_like_currency(s) {
        return Kind::Currency;
    }

    // Boolean literals, case-insensitive.
    match s.to_ascii_lowercase().as_str() {
        "y" | "n" | "yes" | "no" | "true" | "false" | "t" | "f" => return Kind::Bool,
        _ => {}
    }

    let body = s.strip_prefix(['+', '-']).unwrap_or(s);
    if !body.is_empty() && body.bytes().all(|b| b.is_ascii_digit()) {
        // All digits. A leading zero on a multi-digit run is an ID, not a number.
        if body.len() > 1 && body.starts_with('0') {
            return Kind::LeadingZero;
        }
        return Kind::Int;
    }
    if is_plain_decimal(body) {
        return Kind::Decimal;
    }
    Kind::Text
}

fn looks_like_currency(s: &str) -> bool {
    let t = s.strip_prefix('$').map(|r| r.trim()).unwrap_or(s);
    let had_symbol = s.starts_with('$');
    let stripped: String = t.chars().filter(|&c| c != ',').collect();
    let has_comma_grouping = t.contains(',') && t.bytes().any(|b| b.is_ascii_digit());
    if !had_symbol && !has_comma_grouping {
        return false;
    }
    let body = stripped.strip_prefix(['+', '-']).unwrap_or(&stripped);
    !body.is_empty() && is_plain_decimal(body)
}

fn is_plain_decimal(body: &str) -> bool {
    let mut dots = 0;
    let mut digits = 0;
    for b in body.bytes() {
        match b {
            b'0'..=b'9' => digits += 1,
            b'.' => dots += 1,
            _ => return false,
        }
    }
    dots <= 1 && digits > 0
}

/// Whether a decimal string carries float-precision noise (e.g. 449.29999999999995).
pub fn is_float_noise(s: &str) -> bool {
    if let Some((_, frac)) = s.trim().split_once('.') {
        frac.len() >= 12 && frac.bytes().all(|b| b.is_ascii_digit())
    } else {
        false
    }
}

/// Running accumulator for one column.
pub struct Column {
    pub header: String,
    pub total: usize,
    pub nonblank: usize,
    pub kinds: HashMap<Kind, usize>,
    distinct: HashMap<String, ()>,
    pub distinct_capped: bool,
    pub freq: HashMap<String, usize>,
    freq_capped: bool,
    pub num_min: Option<f64>,
    pub num_max: Option<f64>,
    pub len_min: Option<usize>,
    pub len_max: Option<usize>,
    pub examples: Vec<String>,
    pub bool_reprs: Vec<String>,
    pub float_noise: bool,
}

impl Column {
    fn new(header: String) -> Self {
        Column {
            header,
            total: 0,
            nonblank: 0,
            kinds: HashMap::new(),
            distinct: HashMap::new(),
            distinct_capped: false,
            freq: HashMap::new(),
            freq_capped: false,
            num_min: None,
            num_max: None,
            len_min: None,
            len_max: None,
            examples: Vec::new(),
            bool_reprs: Vec::new(),
            float_noise: false,
        }
    }

    /// Distinct count, saturating at the cap. Pair with `distinct_capped`.
    pub fn distinct_count(&self) -> usize {
        self.distinct.len()
    }

    fn observe(&mut self, raw: &str) {
        self.total += 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return;
        }
        self.nonblank += 1;

        let kind = classify(raw);
        *self.kinds.entry(kind).or_insert(0) += 1;

        if !self.distinct_capped {
            if self.distinct.len() < CARDINALITY_CAP {
                self.distinct.entry(raw.to_string()).or_insert(());
            } else if !self.distinct.contains_key(raw) {
                self.distinct_capped = true;
            }
        }
        if !self.freq_capped {
            if self.freq.len() < CARDINALITY_CAP {
                *self.freq.entry(raw.to_string()).or_insert(0) += 1;
            } else if let Some(c) = self.freq.get_mut(raw) {
                *c += 1;
            } else {
                self.freq_capped = true;
            }
        }

        match kind {
            Kind::Int | Kind::Decimal => {
                if let Ok(v) = trimmed.parse::<f64>() {
                    self.num_min = Some(self.num_min.map_or(v, |m| m.min(v)));
                    self.num_max = Some(self.num_max.map_or(v, |m| m.max(v)));
                }
                if kind == Kind::Decimal && is_float_noise(trimmed) {
                    self.float_noise = true;
                }
            }
            Kind::Bool => {
                let r = trimmed.to_string();
                if !self.bool_reprs.contains(&r) {
                    self.bool_reprs.push(r);
                }
            }
            Kind::Currency => {
                let body: String = trimmed
                    .trim_start_matches('$')
                    .chars()
                    .filter(|&c| c != ',')
                    .collect();
                if is_float_noise(body.trim()) {
                    self.float_noise = true;
                }
            }
            _ => {}
        }

        let len = trimmed.chars().count();
        self.len_min = Some(self.len_min.map_or(len, |m| m.min(len)));
        self.len_max = Some(self.len_max.map_or(len, |m| m.max(len)));

        if self.examples.len() < 3 && !self.examples.iter().any(|e| e == raw) {
            self.examples.push(raw.to_string());
        }
    }
}

/// The whole-file reading produced by one streaming pass.
pub struct Scan {
    pub columns: Vec<Column>,
    pub data_rows: usize,
    pub ragged: Vec<(usize, usize)>, // (1-based file row, field count) where count != header width
    pub total_rows: Vec<(usize, String)>, // (1-based file row, a filled value) for summary/total lines
    pub header_row: usize, // 1-based file row of the header; 0 = no header
    pub preamble: usize,   // junk rows above a buried header (0 for a clean row-1 header)
    pub delimiter: u8,
    pub crlf: bool,
    pub bom: bool,
    pub utf8: bool,
    pub bytes: u64,
}

/// Where the header sits: None = auto-detect, Some(0) = the file has no header
/// row, Some(n) = force the header to 1-based file row n.
pub type HeaderChoice = Option<usize>;

/// Sniff the delimiter from a byte sample: the candidate giving the most
/// consistent field count (> 1) across the first lines wins.
fn sniff_delimiter(sample: &[u8]) -> u8 {
    const CANDIDATES: [u8; 4] = [b',', b'\t', b';', b'|'];
    let text = String::from_utf8_lossy(sample);
    let lines: Vec<&str> = text.lines().take(20).collect();
    let mut best = (b',', 0usize, 0usize); // (delim, modal_count, lines_agreeing)
    for &d in &CANDIDATES {
        let mut counts: HashMap<usize, usize> = HashMap::new();
        for line in &lines {
            let fields = line.matches(d as char).count() + 1;
            if fields > 1 {
                *counts.entry(fields).or_insert(0) += 1;
            }
        }
        if let Some((&modal, &agree)) = counts.iter().max_by_key(|(_, &n)| n) {
            if agree > best.2 || (agree == best.2 && modal > best.1) {
                best = (d, modal, agree);
            }
        }
    }
    best.0
}

/// Count of non-blank fields in a record.
fn filled(rec: &csv::StringRecord) -> usize {
    rec.iter().filter(|c| !c.trim().is_empty()).count()
}

/// The modal record shape across a sample: the (fill-count, field-count) pair
/// that recurs most, weighted toward rows that look like a real table body.
fn modal_shape(sample: &[csv::StringRecord]) -> (usize, usize) {
    let mut fill_freq: HashMap<usize, usize> = HashMap::new();
    let mut width_freq: HashMap<usize, usize> = HashMap::new();
    for rec in sample {
        let f = filled(rec);
        if f >= 2 {
            *fill_freq.entry(f).or_insert(0) += 1;
        }
        if rec.len() >= 2 {
            *width_freq.entry(rec.len()).or_insert(0) += 1;
        }
    }
    let modal_fill = fill_freq.iter().max_by_key(|(_, &n)| n).map(|(&w, _)| w);
    let modal_width = width_freq.iter().max_by_key(|(_, &n)| n).map(|(&w, _)| w);
    (modal_fill.unwrap_or(0), modal_width.unwrap_or(0))
}

/// Locate the header row: the first row whose fill count reaches the table's
/// modal width, per the corpus heuristic. Preamble rows are narrower and sit
/// above it. Returns the 0-based index within the sample (0 = clean row-1
/// header). Advisory — the `--header` override always wins.
fn detect_header(sample: &[csv::StringRecord]) -> usize {
    let (modal_fill, _) = modal_shape(sample);
    if modal_fill < 2 {
        return 0;
    }
    for (i, rec) in sample.iter().enumerate() {
        if filled(rec) >= modal_fill {
            return i;
        }
    }
    0
}

/// Observe one data row: fold its cells into the column accumulators and flag
/// raggedness and total-row signatures. `file_row` is 1-based.
fn process_row(
    rec: &csv::StringRecord,
    width: usize,
    file_row: usize,
    columns: &mut [Column],
    data_rows: &mut usize,
    ragged: &mut Vec<(usize, usize)>,
    total_rows: &mut Vec<(usize, String)>,
) {
    *data_rows += 1;
    if rec.len() != width {
        ragged.push((file_row, rec.len()));
    }

    // Total/summary-row signature: nearly all cells blank, but at least one
    // numeric-looking cell filled (a "Total: $…" line masquerading as data).
    let mut blanks = width.saturating_sub(rec.len());
    let mut sample = String::new();
    let mut has_number = false;
    for field in rec.iter() {
        let t = field.trim();
        if t.is_empty() {
            blanks += 1;
        } else {
            if sample.is_empty() {
                sample = t.to_string();
            }
            if matches!(classify(field), Kind::Int | Kind::Decimal | Kind::Currency) {
                has_number = true;
                sample = t.to_string();
            }
        }
    }
    if width >= 4 && has_number && blanks >= width.saturating_sub(2) && total_rows.len() < 64 {
        total_rows.push((file_row, sample));
    }

    for (c, field) in rec.iter().enumerate() {
        if c < columns.len() {
            columns[c].observe(field);
        }
    }
    // Fields beyond the header width still count toward raggedness above.
    for col in columns.iter_mut().skip(rec.len()) {
        col.total += 1; // short row: missing cells count as blank
    }
}

/// Run the streaming pass. Buffers a bounded look-ahead (preamble is always near
/// the top) to locate the header, then streams the remainder as data.
pub fn scan(path: &Path, header_choice: HeaderChoice) -> std::io::Result<Scan> {
    /// How far to look for a buried header before giving up. Bounds the buffer.
    const LOOKAHEAD: usize = 1000;

    let mut raw = Vec::new();
    File::open(path)?.read_to_end(&mut raw)?;
    let bytes = raw.len() as u64;

    let bom = raw.starts_with(&[0xEF, 0xBB, 0xBF]);
    let body = if bom { &raw[3..] } else { &raw[..] };
    let utf8 = std::str::from_utf8(body).is_ok();
    let crlf = body.windows(2).take(4096).any(|w| w == b"\r\n");

    let sample = &body[..body.len().min(16 * 1024)];
    let delimiter = sniff_delimiter(sample);

    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .has_headers(false)
        .from_reader(body);

    let empty = |header_row, preamble| Scan {
        columns: Vec::new(),
        data_rows: 0,
        ragged: Vec::new(),
        total_rows: Vec::new(),
        header_row,
        preamble,
        delimiter,
        crlf,
        bom,
        utf8,
        bytes,
    };

    // Buffer the look-ahead window.
    let mut records = rdr.records();
    let mut buf: Vec<csv::StringRecord> = Vec::new();
    for rec in records.by_ref() {
        buf.push(rec?);
        if buf.len() >= LOOKAHEAD {
            break;
        }
    }
    if buf.is_empty() {
        return Ok(empty(0, 0));
    }

    // Resolve the header position from the choice.
    let (header_present, header_idx) = match header_choice {
        Some(0) => (false, 0),
        Some(n) => (true, (n - 1).min(buf.len() - 1)),
        None => (true, detect_header(&buf)),
    };

    let (header_fields, width): (Vec<String>, usize) = if header_present {
        let h = &buf[header_idx];
        (h.iter().map(|s| s.to_string()).collect(), h.len())
    } else {
        let (_, w) = modal_shape(&buf);
        let w = w.max(buf[0].len());
        (vec![String::new(); w], w)
    };

    let mut columns: Vec<Column> = header_fields.into_iter().map(Column::new).collect();
    let header_row = if header_present { header_idx + 1 } else { 0 };
    let preamble = if header_present { header_idx } else { 0 };

    let mut data_rows = 0usize;
    let mut ragged = Vec::new();
    let mut total_rows: Vec<(usize, String)> = Vec::new();

    // Buffered data rows (everything after the header within the window).
    let data_start = if header_present { header_idx + 1 } else { 0 };
    for pos in data_start..buf.len() {
        process_row(
            &buf[pos],
            width,
            pos + 1,
            &mut columns,
            &mut data_rows,
            &mut ragged,
            &mut total_rows,
        );
    }
    // Streamed remainder (absolute index continues past the buffer).
    for (offset, rec) in records.enumerate() {
        let rec = rec?;
        process_row(
            &rec,
            width,
            buf.len() + offset + 1,
            &mut columns,
            &mut data_rows,
            &mut ragged,
            &mut total_rows,
        );
    }

    Ok(Scan {
        columns,
        data_rows,
        ragged,
        total_rows,
        header_row,
        preamble,
        delimiter,
        crlf,
        bom,
        utf8,
        bytes,
    })
}
