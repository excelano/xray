# xray — design

**Status:** Active design, opened 2026-07-10. This session is carrying it forward. Command name: `xray` (x-family; you *x-ray* a file before you operate on it). The bare `xray` crate name is a dormant 2018 crate, so the crate publishes as **`xray-profiler`** with `[[bin]] name = "xray"` — the command everyone types is still `xray`. Repo: `excelano/xray`.

**One line:** a read-only profiler for a single delimited file — the "what *is* this?" you run first, before xled cleans it or xql queries it.

## The problem

Every job on an unfamiliar CSV starts the same way: orient before acting. What are the columns? What type is each, really? How many rows, how many blanks, how many distinct values, what does the top of the distribution look like, what's the delimiter, is the file ragged? That first-contact profiling is the single most common thing done to a table, and in the owned stack it has no home — it gets farmed out to qsv `stats`, csvstat, datamash, `head`, four tools that overlap heavily and none of which belong to the family. Once one of them is open, the session tends to stay in the specialist zoo instead of returning to xled and xql.

Built for one user's real need, not a market. The need is orientation: the look you take at a file before you decide what to do to it.

## The user

The primary user is Claude Code, working alongside David in a live data session — the same organizing principle as xled, xql, and xshape. The behavioural target is specific and is the whole point of the tool: **xray is the move I reach for *first* on any file, so owning it means the session starts in the family stack and never has to leave.** The entry point is the lever. If the first move pulls in qsv, the next moves tend to follow it there; if the first move is `xray`, the natural next moves are `xled` (clean what xray flagged) or `xql` (query now that the shape is known). Understand → clean → ask.

Same two inherited requirements as its siblings: an example-dense one-page reference mapping xray's output to the qsv/csvstat idioms an LLM already knows, and output an LLM can consume without ceremony (see the open question on default format).

## The boundary — observe, never act

xray's scope line is the cleanest in the family because it is enforced by construction:

**xray only reads. It never writes, never mutates, never reshapes, never filters or aggregates on demand.** It answers "what is this file," never "give me the rows where…". There is no boundary to police between xray and its siblings, because they *act* (mutate, query) and it only *observes*. The two actors already exist; xray is the missing observer that completes them.

The corollary that keeps it honest: **the day xray grows a `--where` predicate, it has become a worse xql.** Ad-hoc filtering, selecting, and on-demand aggregation are xql's job. xray computes a *fixed* whole-file profile — the same battery of facts every time — not a query the user composes. That fixity is the feature: one command, the whole gestalt, no query to write.

| Tool  | Verb | Writes? |
|-------|------|---------|
| **xray** | observe (fixed profile) | never |
| xled   | edit values | yes (previewed) |
| xql    | query the row set | yes (previewed) |

## What it retires

xray plus the xql the family already owns credibly retires three specialists and part of a fourth:

- **qsv `stats` / `frequency` / `headers`** → whole-file profile, per-column cardinality and top-value frequency, the header list.
- **csvstat** (csvkit) → per-column type inference, null/blank counts, min/max/mean/stddev.
- **datamash** → its *ungrouped* stats. (The *grouped* case — stats per category — is already xql's `GROUP BY`; xray does not chase it.)
- **qsv `sample` / `slice` / `search` / `head`** → the peek-at-rows preview, folded into profiling as a sample of real rows.

## The profile — what every run reports

The fixed battery (to be finalised this session; this is the starting slate):

*File level* — row count, column count, detected delimiter, encoding, line-ending style, presence/absence of a header row, and ragged-row detection (rows whose field count differs from the header).

*Per column* — position (letter + name), inferred type (string / integer / decimal / boolean / date, with the same stringly-typed caution as xled: `02134` is text, not the number 2134), count and rate of blank/null cells, distinct-value count (cardinality), and a small top-frequency table. For numeric columns, min / max / mean / and a spread stat. For string columns, min/max length and a couple of example values.

*A sample* — the first few real data rows, rendered readably, so the profile includes what the data actually looks like and not only its statistics.

## Open — to settle this session

These are the decisions xray is being carried forward to make:

1. **Default output format.** The tension: a human skimming a terminal wants an aligned, sectioned, readable report; an LLM consuming the profile wants something structured and stable. Options: a pretty aligned report by default with `--json` for machine use; JSON-first with a `--pretty` human view; or a single format that serves both. Given the primary user is Claude, lean toward a format that is *both* readable and reliably parseable — but decide deliberately.
2. **Flag surface.** Candidates: `--json`, a column selector to profile a subset, `--sample N` for row-count control, `--top N` for the frequency table depth, `--no-sample`, a full-scan vs. sampled-scan toggle for very large files. Keep it small — the fixed-profile discipline means most "flags" are really xql queries in disguise and should be refused.
3. **Sampling on large files.** Exact stats require a full scan; a huge file may warrant a sampled profile with the sampling stated in the output. Decide the default (full scan until proven too slow) and how honestly to label a sampled result.
4. **Type inference rules.** Reuse xled's cast philosophy exactly — a value is a string until it unambiguously is not, leading zeros and long IDs stay text. Share the inference code with xled if practical.
5. **Bare-command / zero-config behaviour.** `xray file.csv` with no flags gives the full default profile. Confirm that matches the family's "bare command reports state" reflex.

## First move next session

Settle open question 1 (default output format) — it shapes everything downstream, and the primary-user-is-Claude framing points hard at a format that reads cleanly *and* parses reliably. Then lock the fixed profile battery (the "what every run reports" slate) against a real file from `~/xled-corpus`.
