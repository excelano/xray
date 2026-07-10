# xray — backlog

Open work after the v0.1 core (scan → film/reading/findings, `--refer`, colour,
`--json`, buried-header, corpus-hardened). Grouped by kind; **P1** = do before
calling v0.1 shippable, **P2** = wanted soon, **P3** = someday / decide-if-ever.

## Robustness & correctness

- **P1 — Big all-digit IDs lose precision.** An 18–19-digit all-digit field
  (Snowflake/BIGINT-style) classifies as `Int` and is parsed through `f64` for
  min/max, silently rounding and saturating at `i64::MAX` in the render. Fix:
  treat an all-digit run longer than ~15 digits as a text ID (same "keep as
  text" reflex as leading zeros), so it neither corrupts stats nor invites a
  cast. Not seen in the corpus, but common in DB exports.
- **P2 — Bool / numeric columns swallow contamination.** A numeric-dominant
  column already flags stray text as `mixed_type`, but a *boolean*-dominant
  column with stray `NA`/`Unknown` sentinels is silently labelled `bool`, and a
  numeric column with a few stray `$`-values ignores the currency cells. Fold
  both minorities into the mixed-type signal.
- **P3 — CRLF detection only scans the first 4 KB.** A file that is LF early and
  CRLF later is mis-reported. Cheap to widen or sample.
- **P3 — Render pads by char count, not display width.** CJK (2 cols) / combining
  marks (0 cols) misalign the reading table. Needs a width-aware pad helper.

## Capability

- **P2 — `--top N` / `--sample N` depth knobs.** The two remaining flags from the
  specced surface: frequency-table depth and rows-shown. Small.
- **P2 — Streaming vs `read_to_end`.** DESIGN and the module doc promise bounded
  memory for files "too big for xled", but the whole file is read into RAM. Fine
  to ~0.5 GB (261 MB corpus file profiles in 7 s); only multi-GB files would OOM.
  Decide: implement true streaming from the file handle (sniff from a small
  pre-read, then stream), or soften the claim. See "evaluate" note below.
- **P3 — stdin input.** `xray` reads a file only; DESIGN noted stdin "not yet
  wired". Useful for pipelines (`… | xray`), but the profiler wants a rewindable
  source for the header look-ahead, so it would buffer stdin first.
- **P3 — Column-subset selector.** The one borderline flag (`--cols A,C,E`).
  Risks becoming an xql-query-in-disguise; decide if it earns its place.
- **P3 — Deferred value-damage checks.** From the corpus taxonomy, not yet
  implemented: whitespace-pad, smart-punct / HTML-entities / escaping,
  multi-value newline cells, stacked / side-by-side tables. Each is a finding
  kind; add as real files demand them.

## Tuning (corpus-driven)

- **P2 — Cardinality cap value.** Currently exact to 10 000 distinct, then `N+`.
  Confirm the number and the `+` labelling against a wide/deep file.
- **P3 — Sparse-column threshold.** Fires < 40 % fill; 268 hits across the
  corpus. Legitimate on wide gov exports, but confirm it isn't drowning the
  findings on very wide files (maybe cap how many sparse notes show).
- **P3 — Buried-header monitoring.** The quote-aware sniff removed the false
  positives; keep an eye on the modal-width heuristic as new file shapes appear.

## Ship v0.1 (distribution & docs)

- **P1 — README (real).** Currently a stub. Prose walkthrough, one hero example,
  the family framing (xray/xled/xql). Prose, not screenshots.
- **P1 — One-page example-dense reference.** A DESIGN requirement: map xray's
  output to the qsv/csvstat idioms an LLM already knows, so first-contact
  fluency. This is the adoption lever, same as xled's.
- **P1 — Tests.** The repo has none. Unit tests for `classify`/`resolve` and the
  heuristics, plus a synthetic-fixture regression suite (the corpus stays out of
  the repo; distil synthetic cases in, xled-style).
- **P2 — install.sh / uninstall.sh + cargo-dist + RELEASING.md.** Per the CLI
  install conventions (Rust = thin cargo-dist shim). Then the apt / homebrew /
  crates.io channels like xled (crate name is `x-ray`, command `xray`).

## Resolved this session (for reference)

Quote-aware delimiter sniff; UTF-8 survival; five code-review correctness bugs
(buried-header FP, total-row FP, nondeterministic tie-break, `id_like`
over-match, `--header` range); `mixed_bool` family logic; `duplicate_key`
near-unique gate. All verified against the 110-file corpus.
