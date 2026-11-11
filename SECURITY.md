# Security Policy

## Reporting a vulnerability

Please report suspected vulnerabilities privately through GitHub Security Advisories at https://github.com/excelano/xray/security/advisories/new. If you would rather not use GitHub, email david.anderson@excelano.com instead. I aim to respond within seven days.

Please do not open public issues for security problems.

## Supported versions

The latest release receives security fixes. Older versions are not supported. A fix ships in a new release; update the way you installed.

## What xray can access

xray is a CLI that runs locally on your machine. It reads the CSV or DSV file you point it at (or standard input), profiles it in a single pass, and prints the result. It never writes, edits, or deletes the files it profiles. The one write path is `--install-skill`, which writes xray's Claude Code skill to `~/.claude/skills/xray/`; `--uninstall-skill` removes that directory. It makes no network calls of any kind, has no auth layer, and implements no administrative operations. It can only read and write files your operating-system user already has access to.

## What xray stores

xray stores nothing outside the skill directory above. There is no config directory, no history file, no cache, no telemetry, no analytics, and no remote logging. It reads a file, writes its report to standard output, and exits.

## Verifying releases

Every GitHub release carries the `.deb` packages and a Windows zip, built by `build-release`, the release program in excelano/shipping, from a tagged commit. The Excelano apt repository is signed, so `apt` verifies each package it installs, and WinGet checks the zip against the SHA-256 recorded in its manifest. To check a file downloaded from the release page by hand, compare its `sha256sum` with the digest GitHub lists for that asset.
