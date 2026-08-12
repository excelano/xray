# xray reference

Complete reference for the `xray` CLI. Load this when `SKILL.md` isn't specific enough —
every flag, every column class, the full findings taxonomy, the detection heuristics, and
the `--json` schema. xray reads a delimited file and reports on it; it never writes.

## Invocation and flags

```
xray [OPTIONS] [FILE]
```

`[FILE]` is optional: omit it, or give `-`, and xray profiles stdin instead, reporting the
source as `(stdin)`. A pipe and a file produce an identical profile — the whole input is
read before parsing either way, since the delimiter sniff and the header look-ahead both
re-read the front. A bare `xray` at a terminal, with nothing piped in, is a usage error
rather than a silent wait. Human output goes to stdout; there is no in-place mode and no
output file, because xray does not mutate.

| Flag | Meaning |
|---|---|
| `--refer` | also print the REFERRAL block: which family tool (xled / xql) treats each finding, named by column, with a runnable command where the repair is unambiguous. Off by default — the profile stands on its own — but an agent should always pass it |
| `--json` | emit the profile as JSON instead of the human render. Always plain (no colour). Stable `class` / `kind` / `column` keys for a machine reader |
| `--header <ROW>` | set the header row explicitly, 1-based. `0` means the file has no header (row 1 is data). Out of range is an error, not a clamp. Omit to auto-detect a buried header |
| `--color <WHEN>` | `auto` (default) colours a terminal and goes plain when piped or read by a program (honours `NO_COLOR`); `always` forces colour; `never` forces plain |
| `-V`, `--version` / `-h`, `--help` | standard |

xray exits `0` whether the file is clean or full of findings — it reports, it does not
judge, so a caller must read the verdict rather than the exit status to learn what the
file is. `1` means bad input: an unreadable file, undecodable data, a `--header` past the
last row. `2` means a bad invocation: an unknown flag, a missing argument, contradictory
options.

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

Hands each hazard to the treating tool, naming the column it is about. Empty when there
is nothing to hand off.

| Trigger | Tool | Action | Command |
|---|---|---|---|
| preamble rows above the header | xled | read with `--no-header`, crop, promote the header row | — |
| pre-aggregated row *n* | xled | crop past the summary line before aggregating | — |
| *n* ragged rows | xled | repair the stray delimiters; later addresses depend on the width | — |
| *col* is currency text | xled | strip the formatting before any math | `xled '[col] s/[$,]//g' file.csv` |
| *cols* stay text | xled | a numeric cast strips the zeros — cast at math time, not in the file | — |
| numbers trapped as text | xql | filter or aggregate once those columns are clean | — |

A command appears only where the repair follows unambiguously from what xray can see. The
blanks are not gaps waiting to be filled: which boolean spelling wins, what a duplicate
header becomes, and whether a sparse column is merged cells or optional data are decisions
the profiler is not entitled to make, and a plausible guess that happens to run is worse
than silence.

Commands read rather than write — no `-i`. xray never changes a byte and does not hand
over something that changes one by proxy; the preview is where the transform gets checked.

Column addressing is the bracketed header name, falling back to the spreadsheet letter
when the header is blank or contains a `]`. The file path is shell-quoted when it needs
it, and omitted entirely for piped stdin, where there is no file to name.

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
findings:[ { group, severity, kind, column, subject, detail } ]
verdict: { clean, total, worst, counts: { correctness, type_safety, structure },
           summary }
referral:[ { trigger, tool, action, command } ]           (only with --refer;
                                                           command is null unless the
                                                           repair is unambiguous)
```

A machine reader should branch on the stable `class` (reading) and `kind` (findings)
values, not on the prose `type`/`detail`/`subject` strings. The same rule governs the
verdict: read `clean`, `worst`, and `counts`, and leave `summary` (the sentence the
human render prints) to a person. `group` is the snake-case key `correctness`,
`type_safety`, or `structure`; `severity` collapses those three to the two levels the
render draws as `!` and `·` — `warn` for correctness and type safety, `note` for
structure. `worst` is the most severe group that actually fired, or `null` on a clean
file. `column` is the column letter
a finding is scoped to, or `null` for a row-level (correctness) finding. `min`/`max` are
`null` for any non-numeric class (including `long_id`, which is numeric-looking but kept
as text). `top` is populated only for the `categorical` class.

## What xray does not do

No writing, no in-place edit, no output file — cleaning is xled's job. No query, join,
aggregate, group, sort, pivot, or row filter — that is SQL/DuckDB (xql). xray reads and
reports; the `--refer` block names where to go next.
