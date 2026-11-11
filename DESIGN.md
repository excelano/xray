# xray — design

**Name:** `xray` (x-family; you *x-ray* a file before you operate on it). On crates.io the crate is **`x-ray`**, with `[[bin]] name = "xray"`; the README's Install section says why.

**One line:** a read-only profiler for a single delimited file — the "what *is* this?" you run first, before xled cleans it or xql queries it.

## The problem

Every job on an unfamiliar CSV starts the same way: orient before acting. What are the columns? What type is each, really? How many rows, how many blanks, how many distinct values, what does the top of the distribution look like, what's the delimiter, is the file ragged? That first-contact profiling is the most common thing done to a table, and without xray it gets farmed out to qsv `stats`, csvstat, datamash and `head`, which overlap heavily and none of which belong to the family. Once one of them is open, the session tends to stay in the specialist zoo instead of returning to xled and xql.

## The user

The primary user is Claude Code, working alongside David in a live data session, the same organizing principle as xled, xql and xshape. The behavioral target is specific: **xray is the move to reach for *first* on any file, so the session starts in the family stack and does not have to leave.** If the first move pulls in qsv, the next moves tend to follow it there; if the first move is `xray`, the natural next moves are `xled` (clean what xray flagged) or `xql` (query now that the shape is known). Understand → clean → ask.

Like its siblings, xray carries an example-dense reference mapping its output to the qsv/csvstat idioms an LLM already knows, and output an LLM can consume without ceremony (`--json`).

## The boundary — observe, never act

**xray only reads. It never writes, never mutates, never reshapes, never filters or aggregates on demand.** It answers "what is this file," never "give me the rows where…". There is no boundary to police between xray and its siblings, because they *act* and it only *observes*.

The corollary: **the day xray grows a `--where` predicate, it has become a worse xql.** Ad-hoc filtering, selecting and on-demand aggregation are xql's job. xray computes a *fixed* whole-file profile, the same battery of facts every time, not a query the user composes. That fixity is the feature: one command, the whole file, no query to write.

The family splits by what changes: xray observes; xled fixes cell values; xshape reshapes the grid (pivot, unpivot, split); xql queries the row set. A join needs a second table and belongs to a SQL engine such as DuckDB, and converting between office formats is this2that's.

## What it replaces

xray, with xql beside it, covers what qsv `stats` / `frequency` / `headers` (whole-file profile, per-column cardinality and top values, the header list) and csvstat (per-column type inference, blank counts, min/max) are reached for. The *grouped* case datamash covers, stats per category, is xql's `GROUP BY`; xray does not chase it. Peeking at rows is `head`'s job; xray shows example values per column, not a row sample.

## The mental model — diagnostic imaging

xray images a table and hands back a reading in three registers, always on, plus an opt-in referral. It never operates.

**The film** — what the file objectively *is*. Dimensions (rows × cols), detected delimiter, encoding, line-ending style, BOM, header row, file size. The plain picture before interpretation.

**The reading** — the per-column table. One row per column: letter (`A`/`B`/`AF`) and name, inferred type, fill rate, cardinality, and a detail field: the min…max range for a numeric column, the top values with counts for a categorical one, real examples for text. Inline `!` flags mark cells of concern (leading-zero, currency, mixed-type), and a fully unique column is marked `· unique key`.

**The findings** — the diagnostic problem list, the reason xray exists rather than aliasing csvstat. Not more statistics: the *damage* that will bite a later step. Grouped and ranked most-severe-first (correctness → type safety → structure), under a headline verdict. Severity is a two-level glyph, `!` (will bite you) and `·` (worth knowing), reinforced by color and never carried by color alone. The kinds and the rule for each are in `skills/xray/reference.md`.

**The referral** *(opt-in, `--refer`)* — names the family tool that treats each finding, and the command where the repair is one unambiguous invocation: structure and value repairs go to xled, filtering or aggregating once the columns are clean goes to xql. **Off by default.** The primary user is Claude, who already knows the family, and a default referral would repeat what it knows; a human learning the stack asks for it.

### Where the findings come from

The findings are drawn from a corpus of real client spreadsheets, kept outside the repository, which shows structural damage (buried headers, preamble rows, total rows, spacer columns, ragged rows, duplicate headers), value damage (leading-zero and long IDs, currency text, float-precision noise, mixed-type columns, whitespace padding) and schema smells (candidate keys, constant columns, sparse columns). The synthetic fixture `fixtures/messy/vendor_spend.csv` exercises a cross-section. The corpus is what thresholds such as the buried-header heuristic and the cardinality cap are tuned against.

## Relationship to the family — one detection core

xled and xql each have a `describe` that covers part of what xray does.

- **xled's `describe`** is advisory *structural* region detection (preamble, blank rows, total rows). It shares xray's type philosophy, so it is the natural library consumer of a shared detection crate, and the family then agrees on structure and types by construction.
- **xql's `describe`** answers a *different* question, an SQL-castable column/type schema, under a *different* type philosophy: xql coerces values so it can query them, whereas xray and xled preserve strings (a leading zero is text, not a number). Sharing xray's stringly-typed inference would mean the wrong thing for a query engine, so xql keeps its own coerce-to-query `describe`.

## Architecture — one pass over a buffered input

The reader loads the whole input, then makes a single pass over it. Loading first is what lets the delimiter sniff and the buried-header look-ahead re-read the front, which a pipe cannot rewind; a file and a pipe take the same path. Memory therefore tracks input size, which suits the stated user: live sessions on client exports, which run to hundreds of megabytes. A multi-gigabyte file belongs to DuckDB.

The pass keeps per-column accumulators plus distinct and frequency sets bounded by a **cardinality cap**: distinct values are counted exactly to 10 000, then reported as `10,000+` in the render and `distinct_capped: true` in the JSON. A capped column is never called a key.

## Settled

- **Mental model** — diagnostic imaging; three registers (film / reading / findings + verdict) always on, referral opt-in.
- **Referral** — off by default, `--refer` to show; only ever suggests family tools.
- **Color** — reinforces severity, never carries it alone; a colorblind-safe axis (blue ↔ amber ↔ gray + brightness), `!`/`·` glyphs redundant; auto-off on non-TTY, `NO_COLOR` and `--color=never|always|auto` honored; via `anstyle` + `anstream`. Palette (Critical `#c62828`/`#f98a8a`, Warning `#9a5b06`/`#fbc23c`, Note `#5c6b78`/`#93a4b3`, Accent `#0b6f86`/`#38d6ef`).
- **Type inference** — xled's cast philosophy (string until unambiguously not; leading zeros and long IDs stay text).
- **Flag surface** — `--refer`, `--json`, `--color`, `--header`/`--no-header`, `-d`/`--delim`, `--install-skill`/`--uninstall-skill`, plus `--version`/`--help`. `--where`, `--select` and a row-sample flag are refused: filtering and sampling are a query's or `head`'s job.
- **Bare command** — `xray file.csv` gives the full default profile; no flag is required. A bare `xray` at a terminal with nothing piped in is the one usage error, by the family's stdin rule.

## The contract

The `--json` view is the contract (keys, `class` and `kind` vocabularies, exit codes, only ever added to within a major version), and `skills/xray/reference.md` is its schema.
