---
name: xray
description: >-
  Profile a CSV/DSV read-only with the `xray` CLI before you edit or query it — the
  diagnostic step in the tabular family. Use this when a task means understanding an
  unfamiliar delimited file first: what it *is* (shape, delimiter, encoding, a buried
  header under a title block), what each column *holds* (type, fill, cardinality —
  stringly-typed, so leading zeros and 16+-digit IDs are reported as text, not numbers),
  and what will *bite a later step* (ragged rows, pre-aggregated total rows, currency
  trapped as text, columns that mix types or boolean spellings, duplicate keys). Reach
  for it instead of eyeballing `head`/`cat` or a throwaway pandas `df.info()`, because it
  parses CSV correctly (quotes, embedded commas and newlines) and ranks the hazards. It
  is **read-only** — it never changes a byte. Do NOT use it to fix values (that's xled)
  or to query, join, aggregate, group, or filter rows (that's SQL/DuckDB, xql); xray
  only *observes* and hands off.
---

# xray — a read-only profiler for tabular data

`xray` reads a CSV or DSV in one pass and tells you three things: **what the file is**,
**what each column holds**, and **what will break the next step**. It is the *look
before you touch* tool — the diagnostic that runs before you clean with [xled](https://github.com/excelano/xled)
or query with [xql](https://github.com/excelano/xql). Think of the mental model as
diagnostic imaging: a **film** of the whole file, a **reading** of each column, and a
ranked list of **findings**. It never writes.

The authoritative sources for xray's behavior are the binary itself (`xray --help`) and
the [README](https://github.com/excelano/xray/blob/main/README.md); if anything here
conflicts with them, they win. If a flag or section described here is missing from
`xray --help`, the installed copy predates it: upgrade with `sudo apt install
--only-upgrade xray` (Debian/Ubuntu), `brew upgrade xray` (macOS), or by re-running the
install one-liner from the README.

## The family, and the one rule that places xray

Three tools, three verbs over the same delimited file:

- **xray observes** — profiles the file, reports hazards, changes nothing.
- **xled edits** — rewrites *cell values* in place (strip currency, restore leading
  zeros, compute a column, crop junk, promote a buried header).
- **xql queries** — set-level questions and reshaping (join, group, aggregate, sort,
  pivot, filter rows in or out) via SQL/DuckDB.

xray is where you **start** on a file you don't yet trust. It produces no output file and
takes no destructive action; its whole job is to tell you which of the other two tools to
reach for, and why. When you already know the file, skip it.

## Running it

**Always pass `--refer`.** It is off by default because a person reading their own file
already knows the family and does not need to be told where xled is. You are not that
reader: the referral block is how you learn which sibling treats each hazard, and for the
cases with one unambiguous repair it hands you the command already written against this
file's name and headers. Running without it throws that away and leaves you guessing at
addressing you could have been given.

```sh
xray --refer file.csv         # the default invocation for an agent
xray file.csv                 # profile only: film, reading, findings
… | xray --refer              # piped data (`xray -` spells stdin explicitly)
xray --json --refer file.csv  # the same profile as structured JSON (for a program)
xray --header 3 file.csv      # force row 3 as the header (0 = no header)
xray --no-header file.csv     # the first row is data, not a header
xray -d '|' file.csv          # force the delimiter when the sniff misreads it
xray --color never file.csv   # plain output (auto-off when piped anyway)
```

xray reads a **file argument or stdin** — omit the file, or give `-`, to profile piped
data (reported as `(stdin)`). Human output goes to stdout; it is safe to redirect or
pipe. There is **no in-place flag and no write path** — by design, there is nothing for
xray to write.

Flags: `--refer` (add the opt-in referral block, off by default), `--json` (machine
output), `--header <ROW>` (1-based; `0` = treat row 1 as data; omit to auto-detect a
buried header), `--no-header` (the family spelling for `--header 0`; giving both is an
error), `-d`/`--delim <CHAR>` (override the sniffed delimiter; `\t` spells tab),
`--color auto|always|never` (auto colours a terminal, goes plain when piped, honours
`NO_COLOR`).

## The three registers (what the output means)

**FILM** — the whole-file shot: column count × row count, which row is the header (and
how much preamble sits above it), byte size, delimiter, encoding, BOM, line endings.

**READING** — one row per column: its letter, header, resolved **type**, **fill %**,
**distinct** count, and a **detail** cell with example values and any flag. The type is
*stringly-typed* (see below), so an ID column reads as `text · leading-0` with a `! keep
as text` flag, not as a number. A column whose values are all distinct and cover every
row is tagged `· unique key` — useful context, not a problem.

**FINDINGS** — the ranked problem list, most severe first, in three groups:
`correctness` (the data is wrong: ragged rows, pre-aggregated total rows, a buried
header) → `type safety` (a value will be corrupted by a naïve cast: leading-zero text,
16+-digit IDs, currency text, mixed types, mixed boolean spellings) → `structure` (shape
smells: empty/spacer columns, constant columns, duplicate keys, sparse columns, duplicate
headers). Correctness and type-safety items are marked `!`; structure notes are marked
`·`. A clean file prints `FINDINGS  (0)   clean — nothing flagged`.

**REFERRAL** (only with `--refer`) — hands each hazard to the sibling that treats it,
naming the column it is about: preamble and pre-aggregated and ragged rows → xled;
currency text → xled; leading-zero and long-ID columns → xled, where the right move is to
leave them alone; numbers trapped as text → xql once those columns are clean.

Where the repair is a single unambiguous invocation, the referral prints **the command**
on the line beneath it, already addressed to this file's name and headers:

```
FY25 Spend ($) (E) is currency text → xled   strip the formatting before any math
    xled '[FY25 Spend ($)] s/[$,]//g' vendor_spend.csv
```

Run it as given. The bracket addressing is what survives a header carrying spaces, parens
or a `$`, which is where hand-written addressing usually goes wrong.

Most referrals carry no command, and that is deliberate rather than unfinished. xray emits
one only where the fix follows from what it can see. Which boolean spelling should win,
what a duplicate header ought to become, whether a sparse column is merged cells or
genuinely optional data — those are yours to decide, and a profiler guessing at them would
produce a command that runs and is wrong. Read the prose and write the command yourself.

The commands **read rather than write**: there is no `-i`, so you see the transform before
it touches anything. Add `-i` once the preview is right.

## The type model (the part that matters)

xray classifies **stringly** — a value stays text until it is unambiguously not — because
that is exactly the discipline xled and xql rely on. Two consequences to internalize:

- **Leading zeros stay text.** `0012`, a zip like `02139`, an account number — reported
  as `text · leading-0` with `keep as text`, never as an integer. A numeric cast would
  strip the zeros.
- **Long all-digit IDs stay text.** A run of 16+ digits (Snowflake/BIGINT-style) is
  `text · long-id`: it exceeds exact numeric range, so xray refuses to treat it as a
  number and reports its min/max as null. Keep it as text.

Currency (`$1,200.00`) is reported as `text · currency`, not a number — the `$` and
thousands commas make it a string until xled strips them. When xray flags float-precision
noise (`449.29999999999995`), that is an already-damaged value, not xray's rounding.

## Worked reading

```sh
# profile an unfamiliar export first — what is it, and what will bite?
xray --refer vendor_spend.csv
#   → FILM: 8 cols × 10 rows, header row 1
#   → READING flags column B leading-0, E currency+float-noise, F mixed, G mixed-bool
#   → FINDINGS: ragged row 11, total row 10, then the type-safety items
#   → REFERRAL: the sibling for each hazard, and for the currency column the
#     command to run — xled '[FY25 Spend ($)] s/[$,]//g' vendor_spend.csv
# now you know the order: repair the rows, strip the currency, then query with xql.

# a title block above the real header? force it, or let auto-detect try
xray --header 4 quarterly_report.csv

# feed the profile to a script (stable class/kind/column keys; no colour)
xray --json vendor_spend.csv | jq '.findings[] | select(.group=="correctness")'

# confirm a file is clean before trusting it
xray employees.csv        # → FINDINGS (0)  clean — nothing flagged
```

## When to stop and switch

xray never fixes and never queries — it points. Once you know what's wrong:

- **Fixing values** — strip the currency, restore the zeros, crop the total row, promote
  the buried header, compute a column → **xled** (`skills/xled`).
- **Querying or reshaping** — join, group, aggregate, sort, pivot, filter rows in or out
  → **SQL/DuckDB** (**xql**, `skills/xql`).

If you catch yourself wanting xray to *change* the file or *answer a question about the
rows*, that's the signal to hand off. xray's job ends at the diagnosis.

See `reference.md` in this directory for the full flag list, every column class, the
complete findings taxonomy with its groups and glyphs, the detection heuristics, and the
`--json` schema.
