# xray — design

**Status:** Active design, opened 2026-07-10. This session is carrying it forward. Command name: `xray` (x-family; you *x-ray* a file before you operate on it). The bare `xray` crate name is a dormant 2018 crate, so the crate publishes as **`x-ray`** — the real word, free because crates.io counts `xray` and `x-ray` as distinct names — with `[[bin]] name = "xray"`, so the command everyone types is still `xray`. Repo: `excelano/xray`.

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

## The mental model — diagnostic imaging

xray images a table and hands back a reading in three registers, always on, plus an opt-in referral. It never operates. The metaphor is the name's: you *x-ray* the file, you get a diagnostic readout.

**The film** — what the file objectively *is*. Dimensions (rows × cols), detected delimiter, quote char, encoding, line-ending style, BOM, header row present/absent, file size. The plain picture before interpretation.

**The reading** — the per-column table, the substance. One row per column: letter (`A`/`B`/`AF`) + name, inferred type, fill (non-blank count / rate), cardinality, and type-appropriate stats — numeric gets min/max/mean/spread, text gets min/max length and real examples. Inline `!` flags mark cells of concern (leading-zero, currency, mixed-type).

**The findings** — the diagnostic problem list, the reason xray exists rather than aliasing csvstat. Not more statistics — the *damage* that will bite a later step: ragged rows, mixed-type columns, leading-zero/ID columns, high-blank columns, total/subtotal rows, spacer columns, duplicate/blank headers, buried headers, candidate keys. Grouped and ranked most-severe-first (correctness → type safety → structure), under a headline verdict (*"7 findings: …"*). Severity is a two-level glyph: `!` (will bite you) and `·` (worth knowing), reinforced by colour (see the colour design), never carried by colour alone.

**The referral** *(opt-in, `--refer`)* — names the family tool that treats each finding: leading zeros / ragged rows → xled; filter or aggregate → xql; pivot → xshape. **Off by default.** The primary user is Claude, who already knows the family; a default referral would be preaching (lux principle: never wire self-evident guidance). It earns its keep for a human learning the stack, so it waits to be asked.

### The findings catalogue

Grounded row-for-row in `~/xled-corpus/CORPUS-FINDINGS.md` (143 real client spreadsheets), in three registers:

- **Structural** — buried header (via the modal-width-jump heuristic), preamble rows, leading/trailing blank rows, spacer column, trailing empty cols, total/subtotal rows, stacked tables, side-by-side tables, ragged rows, blank/duplicate headers.
- **Value** — leading-zero/long-ID (→ keep text), currency formatting, float-precision noise (`449.29999999999995`), mixed-type column, whitespace pad, smart-punct / HTML-entity / escaping, multi-value newline cell.
- **Schema** — candidate key (100% unique), constant column, high-blank/sparse, sentinel values (`TBN`, `n/a`, `ignore`), categorical → top-N, aggregate-as-source.

The synthetic torture fixture `fixtures/messy/vendor_spend.csv` exercises a cross-section; the real corpus is for *tuning* thresholds later (the modal-width heuristic, "how mixed is mixed", cardinality cap), same as xled's validation run.

## Relationship to the family — one detection core

**xled and xql both already ship a `describe`.** They pre-date xray and each gropes toward a slice of what xray now owns:

- **xled's `describe`** is advisory *structural* region detection (preamble, blank rows, total rows) — and its corpus notes flag that it *misses the buried-header case*. That is xray's structural-findings layer in embryo; it only lived in xled because xray didn't exist. **Resolution (Fork A):** build the structural + stringly-typed detection properly in xray, and treat xled's `describe` as a future **library consumer** of a shared detection crate — the family agrees on structure and types by construction, one implementation.
- **xql's `describe`** answers a *different* question — an SQL-castable column/type schema — under a *different* type philosophy: xql wants to coerce values so it can query them, whereas xray and xled deliberately preserve strings (a leading zero is text, not a number). That philosophical split is exactly why xql stays on the far side of the library boundary: sharing xray's stringly-typed inference would mean the wrong thing for a query engine. So the shared core is **structural detection + preserve-the-string inference** (xled + xray); xql keeps its own coerce-to-query describe. Lower crossover, and principled, not incidental.

## Architecture — stream, don't buffer

**Resolution (Fork B):** xray is read-only and single-pass, so it *streams* — bounded memory regardless of file size, unlike xled (whole file in RAM, ~8.7× file size, ~1 GB on the 93 MB corpus exports). This is a capability win: xray profiles the big files xled chokes on, which fits "the first move on *any* file." The one cost is that exact distinct-counts need a **cardinality cap** — exact up to a bound (K distinct), then report `K+` (or an approximate count), with the cap stated in the output. Streaming with a cardinality cap is the design; the cap value is a tuning knob for the corpus phase.

> **As-built (v0.1.0):** the single-pass scan and cardinality cap shipped, but the reader currently loads the whole file into memory before the pass rather than streaming from the file handle — so the "bounded memory regardless of file size" property above is *not yet realized* (fine to ~0.5 GB; a 261 MB corpus file profiles in ~7 s). Closing this — true streaming from a small pre-read, or softening the claim — is tracked as a P2 in `BACKLOG.md`.

## Settled

- **Mental model** — diagnostic imaging; three registers (film / reading / findings + verdict) always on, referral opt-in.
- **Referral** — off by default, `--refer` to show; only ever suggests family tools.
- **Colour** — reinforces severity, never carries it alone; colourblind-safe axis (blue ↔ amber ↔ gray + brightness), `!`/`·` glyphs redundant; auto-off on non-TTY, `NO_COLOR` and `--color=never|always|auto` honoured; via `anstyle` + `anstream`. Palette approved 2026-07-10 (Critical `#c62828`/`#f98a8a`, Warning `#9a5b06`/`#fbc23c`, Note `#5c6b78`/`#93a4b3`, Accent `#0b6f86`/`#38d6ef`).
- **Type inference** — xled's cast philosophy exactly (string until unambiguously not; leading zeros / long IDs stay text), shared via the detection core above.
- **Architecture** — streaming single-pass with a cardinality cap.
- **xled `describe`** — future consumer of the shared detection crate; **xql `describe`** stays separate (coerce-to-query type philosophy).

## Open — still to settle

1. ~~**`--json` shape.**~~ **Done.** Same three registers as the human render, as one model / two renderings; findings carry a stable machine `kind` and column letter; referral gated by `--refer`; always plain.
2. **Flag surface (final).** Shipped: `--refer`, `--json`, `--color`, plus `--version`/`--help`. Not yet built: `--top N` (frequency depth) and `--sample N` (rows shown) — the depth knobs. Resist anything that's an xql query in disguise (`--where`, `--select`). A subset column selector is the one borderline case — decide.
3. **Cardinality cap value.** The K where exact distinct-counts become `K+`. A corpus-tuning knob; pick a default (candidate: 10k) and how to label a capped count.
4. ~~**Buried-header heuristic.**~~ **Built.** Modal-width-jump detection over a bounded look-ahead buffer; reports the header row + preamble in the film and a `buried_header` finding; `--header <N>` (0 = none) overrides. False-positive threshold still wants real-corpus tuning (a row-1 header with trailing-blank cells can mis-detect).
5. **Bare-command / zero-config.** `xray file.csv` gives the full default profile — matches the family's "bare command reports state" reflex. Confirm no required flags.

## First move next session

The design is settled enough to build. Next is implementation of the streaming scan + the human render (film / reading / findings) against `fixtures/messy/vendor_spend.csv`, then the `--json` schema (open 1). Corpus-tuning (open 3, 4) comes after the render is real and there's something to tune.
