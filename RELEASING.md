# Releasing xray

The release loop lives in `~/notes/releasing.md` — the ordered steps, the apt
step, crates.io, the winget submission, the spent-tag rule, and the standing
facts about tokens and secrets. Failure recipes are in
`~/notes/build_release_gotchas.md`. This file carries what is true of xray and
not of its siblings.

| | |
|---|---|
| Loop | cargo-dist |
| Version lives in | `version` in `Cargo.toml` |
| `apt-ship` argument | `xray` |
| crate | `x-ray` |
| winget package | `Excelano.xray` |
| Windows asset | `x-ray-x86_64-pc-windows-msvc.zip` |

**The crate publishes as `x-ray`; everything else is `xray`.** The bare `xray`
name is a dormant 2018 crate, so crates.io is the lone hyphenated coordinate.
cargo-dist names its tarballs and installer after the crate —
`x-ray-installer.sh`, `x-ray-<target>.tar.xz` — and each contains the `xray`
binary. The Homebrew formula and the apt package are pinned back to `xray` via
`[package.metadata.dist] formula = "xray"` and `[package.metadata.deb] name =
"xray"` in `Cargo.toml`, so they install as `brew install excelano/tap/xray` and
`apt install xray`, matching the rest of the family.

That mismatch is the thing to get right in the winget step: the URL carries the
`x-ray-` asset prefix while the package identifier is `Excelano.xray`. Verifying
a publish hits it too — the crates.io API path is `/crates/x-ray`.

**The release builds** the five platform tarballs, the shell and PowerShell
installers, the Homebrew formula, and the checksums, then creates the GitHub
Release. The `.deb` packages come from the separately dispatched `deb.yml`.

**xray trips `Validation-Executable-Error` by design.** Bare invocation with no
input takes the no-input guard, prints `xray: no input — give a file, or pipe
data in`, and returns 2 rather than block on a stdin nobody is writing to.
Recipe in the gotchas file; do not change the guard to appease the sweep.
