# xray — a read-only profiler for tabular data

xray is the first move on an unfamiliar CSV or DSV. One pass, one command, and it tells you what the file *is* before you touch it: its shape, what each column really holds, and — the part that earns it a place in your toolkit — what is going to bite you when you clean or query it. You *x-ray* the file, you read the film, and you know your next move.

**Project page:** [https://excelano.com/xray/](https://excelano.com/xray/)

```text
$ xray contracts.csv

FILM
  20 columns × 4,812 rows       header: row 1       6.4 MB
  delimiter comma   encoding utf-8   line endings CRLF

READING
  col  header            type              fill  distinct  detail
  A    contract_id       text · leading-0  100%      4812  0007 … 9944   ! keep as text
  E    fy25_spend        text · currency   100%      4120  $0 … $1,204,880.00   ! not numeric
  H    renewals          int · MIXED        96%        14  0 … 37   ! 3 non-numeric values
  ...

FINDINGS  (5)   1 correctness · 3 type safety · 1 structure
  correctness
   1  ! total row 4813 — pre-aggregated "$18,442,905.00"; a summary line, not data
  type safety
   2  ! contract_id (A) is leading-zero text — 0007, 0044; a numeric cast strips the zeros
   3  ! fy25_spend (E) is currency text, not a number — $ and thousands commas; de-currency before math
   ...
```

## Why

Every job on a messy CSV starts the same way: orient before acting. What are the columns, what type is each *really*, how many rows, how many blanks, what's the delimiter, is a header even on the first row? That first-contact profiling has been scattered across half a dozen general tools — `qsv`, `csvstat`, `datamash`, `head` — none of which quite answers the question you actually have, which is not "what's the mean of column 4" but *"what's going to break when I touch this?"*

xray answers that. Its **findings** register is a ranked problem list, not more statistics: ragged rows, total rows masquerading as data, leading-zero IDs a cast would mangle, currency trapped as text, mixed-type columns, headers buried under title rows. Everything it flags is damage that would corrupt a later step, and every finding names the tool that fixes it.

It is stringly-typed on purpose. `02134` is text, not the number 2134; an 18-digit ID is text, not a rounded float — because silently coercing those is exactly the surprise xray exists to catch. And it only ever *observes*: it never edits a value, drops a row, or filters a result. The day it grows a `--where` it has become a worse query tool.

## The family

xray is the read-only member of a three-tool family for messy tabular data:

- **xray** observes — a fixed whole-file profile, and never writes.
- **[xled](https://github.com/excelano/xled)** edits cell *values* in place (sed and awk for tables).
- **[xql](https://github.com/excelano/xql)** queries the row *set* — filter, aggregate, group.

Two of them act on the data; xray only looks. Its findings hand you off to the other two: leading zeros and total rows go to xled for a crop, currency-cleaned columns go to xql for the query.

## Install

### Debian and Ubuntu

Add the [Excelano apt repository](https://excelano.com/apt/) once:

```sh
curl -fsSL https://excelano.com/apt/setup.sh | sudo sh
```

Then install it, so `apt upgrade` keeps it current:

```sh
sudo apt install x-ray
```

Both amd64 and arm64 packages ship with every release. The command is `xray`.

### Homebrew

```sh
brew install excelano/tap/x-ray
```

### crates.io

```sh
cargo install x-ray
```

The crate is `x-ray`; the installed command is `xray`.

### Curl (any Linux or macOS)

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://raw.githubusercontent.com/excelano/xray/main/install.sh | sh
```

To remove it: swap `install.sh` for `uninstall.sh` in that line.

## Usage

```sh
xray data.csv                 # the full profile: film, reading, findings
xray --refer data.csv         # also print which family tool treats each finding
xray --json data.csv          # the same profile as structured JSON
xray --header 6 data.csv      # force the header to row 6 (0 = no header)
xray --color never data.csv   # plain output (also automatic when piped)
```

xray auto-detects a buried header, sniffs the delimiter (quote-aware), and colours the output for a terminal while emitting plain text to a pipe. Everything it needs, it reads in one streaming pass.
