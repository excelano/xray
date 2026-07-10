# Releasing xray

The release loop for a new version. Run it from a clean `main` with the working tree committed.

The crate publishes as **`x-ray`** (the bare `xray` name is a dormant 2018 crate); the command everyone installs is **`xray`**. cargo-dist's tarballs and installer are named after the crate — `x-ray-installer.sh`, `x-ray-<target>.tar.xz` — and each contains the `xray` binary. The Homebrew formula and the apt package are pinned to **`xray`** via `[package.metadata.dist] formula = "xray"` and `[package.metadata.deb] name = "xray"` in `Cargo.toml`, so they install as `brew install excelano/tap/xray` and `apt install xray`, matching the rest of the family. (crates.io is the lone hyphenated coordinate.)

1. **Bump the version.** Edit `version` in `Cargo.toml` (e.g. `0.1.0` → `0.1.1`). Update `Cargo.lock` with a build (`cargo build`), run `cargo test`, commit.

2. **Tag and push.** `git tag v0.1.1 && git push origin main --tags`. The `v*` tag triggers cargo-dist (`.github/workflows/release.yml`), which builds the five platform tarballs, the shell/PowerShell installers, the Homebrew formula, and the checksums, then creates the GitHub Release.

3. **Build the .debs.** cargo-dist creates the release with the default `GITHUB_TOKEN`, and GitHub does **not** fire `release: published` for token-created releases (a documented anti-recursion safeguard). So `deb.yml` won't auto-run — dispatch it by hand:
   ```sh
   gh workflow run deb.yml -f tag=v0.1.1
   ```
   It builds amd64 + arm64 packages and uploads them to the release.

4. **crates.io publishes itself.** The `v*` tag also triggers `publish-crate.yml`, which runs `cargo publish` with the org-secret token — so crates.io is live within a minute of the push, no local step. Confirm it succeeded:
   ```sh
   gh run list --workflow=publish-crate.yml --limit 1
   ```
   Do **not** run `cargo publish` by hand — the pipeline beats you to it and you'll just get `already exists`. **Versions are immutable**: you can `cargo yank` a bad release to hide it from new dependency resolution, but never re-publish the same number. A fix is always a fresh version bump, never a re-push.

5. **Add the .debs to the Excelano apt repo.** Download the two `.deb`s from the release, then in `~/excelano-apt/`: `add-deb.sh` each one → `rebuild.sh` (GPG-signs) → `updatesite excelano.com.apt -y`. **Dry-run the rsync first** (`rsync … --delete -n`) and confirm zero deletions before the real push — the apt pool is a superset of live, and a stray `--delete` wipe is the standing hazard.

## Notes

- **crates.io API needs a User-Agent.** Requests without one return empty. To verify a publish from a script: `curl -s -H "User-Agent: …" https://crates.io/api/v1/crates/x-ray`.
- **First-time crates.io setup:** the `CRATES_IO_TOKEN` org secret must be present for `publish-crate.yml` to fire; a verified crates.io email is required before the first publish; the token needs `publish-new` + `publish-update` scopes.
- **First-time Homebrew:** cargo-dist pushes the formula to `excelano/homebrew-tap`; that tap repo must exist and the release job needs the `HOMEBREW_TAP_TOKEN` secret allowed to push to it. **Gotcha (bit v0.1.0):** if that's an *org* secret scoped to selected repos, a brand-new repo isn't on the list, so the `publish-homebrew-formula` job fails at checkout with `Input required and not supplied: token`. Add the new repo to the secret's access list before tagging. If it's already tagged, don't re-run that job on the old tag — push the formula to the tap by hand for that release; the token fix takes effect on the next one.
- **docs.rs** rebuilds automatically on each publish — no action needed.
- The README, the landing page (`excelano.com/xray`), and `SECURITY.md` reference the version implicitly via "latest"; none need a per-release edit.
