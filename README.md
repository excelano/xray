# xray — profile for tabular data

> **Status: active design, early scaffold.** See [DESIGN.md](DESIGN.md).

xray is the read-only profiler in a three-tool family for messy tabular data. It is the first move on an unfamiliar file: one command, the whole picture — columns, inferred types, blank rates, cardinality, top values, delimiter and encoding, ragged rows, and a sample of what the data actually looks like. You *x-ray* a file before you decide what to do to it.

- **xray** observes — a fixed whole-file profile, and never writes.
- **[xled](https://github.com/excelano/xled)** edits cell *values* in place.
- **[xql](https://github.com/excelano/xql)** queries the row *set* — filter, aggregate, group.

Two act on the data; xray only looks. That read-only scope is enforced by construction — the day xray grows a `--where` predicate it has become a worse xql. It computes the same battery of facts every time, not a query you compose.

Together they retire the profiling zoo: `xray` covers qsv `stats`/`frequency`/`headers`, csvstat, and datamash's ungrouped stats, so the session can stay in one family.
