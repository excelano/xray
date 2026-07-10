# xray — backlog

> **Status (2026-07-10):** v0.1.0 **SHIPPED** to all channels — crates.io (crate
> `x-ray`, command `xray`), Homebrew (`brew install excelano/tap/xray`), apt
> (`sudo apt install xray`, amd64 + arm64), GitHub release, and a Claude Code
> skill under `skills/xray/`. Docs audited and the landing page (`excelano.com/xray/`)
> is live and correct. Everything below is post-v0.1.0 work.

**When you return, start here** (my recommended order):

1. **Bool / numeric contamination** (correctness gap — below). A boolean column
   with a few stray `NA`/`Unknown` cells is silently labelled `bool`; fold the
   minority into the mixed-type signal, same as numeric columns already do. Small,
   high-value, matches the "never hide damage" ethos.
2. **Streaming vs `read_to_end`** (design decision — below). DESIGN now carries an
   as-built note admitting v0.1 buffers the whole file; resolve it — implement true
   streaming, or soften the claim in DESIGN + README. Pick a lane.
3. **`--top N` / `--sample N`** (finishes the spec'd flag surface — below).

Priority legend, re-based post-release: **P1** = next work session, **P2** =
wanted soon, **P3** = someday / decide-if-ever.

## Robustness & correctness

- **P1 — Bool / numeric columns swallow contamination.** A numeric-dominant column
  already flags stray text as `mixed_type`, but a *boolean*-dominant column with
  stray `NA`/`Unknown` sentinels is silently labelled `bool`, and a numeric column
  with a few stray `$`-values ignores the currency cells. Fold both minorities into
  the mixed-type signal. (`src/resolve.rs` bool/int branches, `src/findings.rs`
  `mixed_type`.)
- **P3 — CRLF detection only scans the first 4 KB.** A file that is LF early and
  CRLF later is mis-reported. Cheap to widen or sample.
- **P3 — Render pads by char count, not display width.** CJK (2 cols) / combining
  marks (0 cols) misalign the reading table. Needs a width-aware pad helper.

## Capability

- **P1 — Streaming vs `read_to_end`.** DESIGN's "Architecture — stream, don't
  buffer" promises bounded memory regardless of file size, but the reader loads the
  whole file into RAM before the single pass (see the as-built note in DESIGN.md).
  Fine to ~0.5 GB (a 261 MB corpus file profiles in ~7 s); only multi-GB files
  would OOM. **Decide:** implement true streaming from the file handle (sniff from a
  small pre-read, then stream — same rewindable-source problem as stdin below), or
  soften the DESIGN + README claim. The README already says "single pass," not
  "streaming," so softening is nearly done; the DESIGN decision is the open part.
- **P2 — `--top N` / `--sample N` depth knobs.** The two remaining flags from the
  spec'd surface: frequency-table depth (top values per categorical) and rows-shown
  (a real-row sample). Small. Resist anything that drifts toward an xql query.
- **P3 — stdin input.** `xray` reads a file only; DESIGN noted stdin "not yet
  wired". Useful for pipelines (`… | xray`), but the profiler wants a rewindable
  source for the header look-ahead, so it would buffer stdin first (couples to the
  streaming decision above).
- **P3 — Column-subset selector.** The one borderline flag (`--cols A,C,E`). Risks
  becoming an xql-query-in-disguise; decide if it earns its place.
- **P3 — Deferred value-damage checks.** From the corpus taxonomy, not yet
  implemented: whitespace-pad, smart-punct / HTML-entities / escaping, multi-value
  newline cells, stacked / side-by-side tables. Each is a finding kind; add as real
  files demand them.

## Tuning (corpus-driven)

- **P2 — Cardinality cap value.** Currently exact to 10 000 distinct, then `N+`.
  Confirm the number and the `+` labelling against a wide/deep file.
- **P3 — Sparse-column threshold.** Fires < 40 % fill; 268 hits across the corpus.
  Legitimate on wide gov exports, but confirm it isn't drowning the findings on very
  wide files (maybe cap how many sparse notes show).
- **P3 — Buried-header monitoring.** The quote-aware sniff removed the false
  positives; keep an eye on the modal-width heuristic as new file shapes appear.

## Docs

- **P2 — qsv / csvstat Rosetta.** The DESIGN adoption lever: a short crosswalk
  mapping xray's output to the qsv `stats` / csvstat idioms an LLM already knows
  ("what you'd call `stats --everything`, xray calls the reading; `frequency` is the
  `--top`"). The skill teaches xray on its own terms; this teaches it in the reader's
  existing vocabulary. Add to `skills/xray/reference.md` or the README.

## Resolved (v0.1.0, for reference)

Core: streaming-*single-pass* scan → film / reading / findings, `--refer`, `--json`,
colour (colourblind-safe), buried-header detection, quote-aware delimiter sniff,
UTF-8 survival (lossy decode). Corpus-hardened against 110 real CSVs.

Correctness fixes: five code-review bugs (buried-header FP, total-row FP,
nondeterministic tie-break, `id_like` over-match, `--header` range); `mixed_bool`
family logic; `duplicate_key` near-unique gate; **big all-digit IDs kept as text**
(`long_id` class — 16+ digits stay text, null min/max, no `f64` corruption).

Ship: real prose README, 8 end-to-end tests + synthetic fixtures, install/uninstall
shims, cargo-dist + RELEASING.md, apt + Homebrew + crates.io channels, Claude skill
(`skills/xray/`), SECURITY.md. Naming pinned so every coordinate but the crate is
`xray` (`formula` + deb `name` overrides). Docs audited; landing page live.
