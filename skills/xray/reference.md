# xray reference

Complete reference for the `xray` CLI. Load this when `SKILL.md` isn't specific enough —
every flag, every column class, the full findings taxonomy, the detection heuristics, and
the `--json` schema. xray reads a delimited file and reports on it; it never writes.

## Invocation and flags

```
xray [OPTIONS] <FILE>
```

`<FILE>` is required — xray profiles a file, not stdin (a rewindable source is needed for
the header look-ahead, so piping is not wired yet). Human output goes to stdout; there is
no in-place mode and no output file, because xray does not mutate.

| Flag | Meaning |
|---|---|
| `--refer` | also print the REFERRAL block: which family tool (xled / xql) treats each class of finding present. Off by default — the profile stands on its own |
| `--json` | emit the profile as JSON instead of the human render. Always plain (no colour). Stable `class` / `kind` / `column` keys for a machine reader |
| `--header <ROW>` | set the header row explicitly, 1-based. `0` means the file has no header (row 1 is data). Out of range is an error, not a clamp. Omit to auto-detect a buried header |
| `--color <WHEN>` | `auto` (default) colours a terminal and goes plain when piped or read by a program (honours `NO_COLOR`); `always` forces colour; `never` forces plain |
| `-V`, `--version` / `-h`, `--help` | standard |

## The three registers

### FILM — the whole-file shot

Column count × row count (data rows, header excluded), the header row number and how many
preamble rows sit above it, byte size, delimiter (quote-aware sniff, not char-counting),
encoding (`utf-8` or `non-utf-8` — a non-UTF-8 file is decoded lossily so the profile
still runs), BOM presence, and line endings (`LF` / `CRLF`).

### READING — one row per column

| Field | Meaning |
|---|---|
| `col` | spreadsheet letter (A, B, … past Z: AA, AB) |
| `header` | the header text, or `‹blank›` |
| `type` | the resolved class label (see below), e.g. `text · leading-0`, `int · MIXED` |
| `fill` | percent of rows non-blank |
| `distinct` | distinct non-blank values (exact up to the cardinality cap of 10 000, then `N+`) |
| `detail` | example values, a `· unique key` tag for a candidate key, and any `!`-flag |

A **candidate key** (every value distinct and present across all data rows) is surfaced
here as `· unique key` context — it is *not* a finding, because it is good news.

### FINDINGS — the ranked problem list

Ordered by group severity, most severe first, discovery order preserved within a group.
The header line tallies the groups, e.g. `FINDINGS  (7)   2 correctness · 4 type safety ·
1 structure`. A clean file prints `FINDINGS  (0)   clean — nothing flagged`.

Glyphs: `!` for correctness and type-safety items (they will corrupt data), `·` for
structure notes (shape smells).

### REFERRAL — opt-in hand-off (`--refer` only)

Maps the findings present to the treating tool. Empty when there is nothing to hand off.

| Trigger | Tool | Action |
|---|---|---|
| ragged / total / spacer rows | xled | crop to the real table, drop the summary line |
| leading-zero / currency text | xled | keep IDs as text; `round(num(),2)` only at math time |
| numbers trapped as text | xql | filter or aggregate once those columns are clean |

## Column classes

The classifier is **stringly-typed**: a value stays text until it is unambiguously not.
The `class` (the stable JSON value) and its human label:

| class | label | What it is |
|---|---|---|
| `empty` | `empty` | no non-blank values; a spacer if the header is also blank |
| `leading_zero` | `text · leading-0` | all-digit values with a significant leading zero — flagged `keep as text` (a cast strips the zeros) |
| `long_id` | `text · long-id` | an all-digit run of 16+ digits — exceeds exact numeric range, so it stays text and reports null min/max |
| `currency` | `text · currency` | `$` and thousands-comma money — text until de-currencied; may flag `float-noise` |
| `bool` | `bool` (or `bool · mixed-repr`) | boolean-valued; `mixed-repr` when more than one spelling family appears (Y/N vs yes/no vs true/false) |
| `int` | `int` (or `int · MIXED`) | integers; `MIXED` when a few non-numeric values contaminate the column |
| `decimal` | `decimal` | real numbers |
| `categorical` | `text · categorical` | low-cardinality text; detail shows the top values with counts |
| `text` | `text` | free text |

## Findings taxonomy

Every finding kind, its group, and its stable JSON `kind`. Correctness and type-safety
render with `!`; structure with `·`.

**Correctness** (the data is wrong; row-level):

| kind | Fires when |
|---|---|
| `buried_header` | a preamble/title block sits above the real header row |
| `ragged_row` | a row's field count differs from the table width (usually a stray comma in an unquoted cell) |
| `total_row` | a pre-aggregated summary line (blank label column, an aggregated value) — not data |

**Type safety** (a naïve cast will corrupt a value; column-scoped):

| kind | Fires when |
|---|---|
| `leading_zero` | leading-zero text — a numeric cast strips the zeros |
| `long_id` | a 16+-digit numeric ID — exceeds exact number range, keep as text |
| `currency_text` | `$`/comma currency (optionally plus float-precision noise) — de-currency before math |
| `mixed_type` | a numeric-dominant column with stray non-numeric values — `num()` skips them |
| `mixed_bool` | a boolean column mixing spelling families — normalize before logic |

**Structure** (shape smells; column-scoped unless noted):

| kind | Fires when |
|---|---|
| `empty_column` | a named column that is entirely empty |
| `spacer_column` | a blank-header column that is entirely empty |
| `constant_column` | one value repeated across every non-blank row |
| `duplicate_key` | an id-like column that is *near*-unique (≥90 % distinct) but has a few duplicates — a key with stray dups |
| `sparse_column` | fill between 1 % and 40 % — mostly blank |
| `duplicate_header` | a header name repeats an earlier column's |

Notes on the heuristics: `duplicate_key` deliberately fires only on near-unique columns,
so a low-cardinality reference column (a repeating category that happens to end in `id`)
is *not* flagged as a broken key. A candidate key (fully unique) is reported in the
READING as `· unique key`, not as a finding.

## The `--json` schema

Top-level keys: `file`, `film`, `reading`, `findings`, `verdict` (and `referral` only
with `--refer`).

```
film:    { columns, rows, bytes, delimiter, encoding, bom, line_endings,
           header_row, preamble, ragged_rows }
reading: [ { letter, header, type, class, fill_pct, nonblank, total, distinct,
             distinct_capped, candidate_key, flag, min, max, examples, top } ]
findings:[ { group, kind, column, subject, detail } ]
verdict: "2 correctness · 4 type safety · 1 structure"   (or "clean — nothing flagged")
referral:[ { trigger, tool, action } ]                    (only with --refer)
```

A machine reader should branch on the stable `class` (reading) and `kind` (findings)
values, not on the prose `type`/`detail`/`subject` strings. `column` is the column letter
a finding is scoped to, or `null` for a row-level (correctness) finding. `min`/`max` are
`null` for any non-numeric class (including `long_id`, which is numeric-looking but kept
as text). `top` is populated only for the `categorical` class.

## What xray does not do

No writing, no in-place edit, no output file — cleaning is xled's job. No query, join,
aggregate, group, sort, pivot, or row filter — that is SQL/DuckDB (xql). No stdin yet
(the header look-ahead needs a rewindable source). xray reads a file and reports; the
`--refer` block names where to go next.
