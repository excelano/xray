# Security Policy

## Reporting a vulnerability

Please report suspected vulnerabilities privately through GitHub Security Advisories at https://github.com/excelano/xray/security/advisories/new. If you would rather not use GitHub, email david.anderson@excelano.com instead. I aim to respond within seven days.

Please do not open public issues for security problems.

## Supported versions

The latest 0.x release receives security fixes. Older versions are not supported.

## What xray can access

xray is a CLI that runs locally on your machine. It reads the CSV or DSV file you point it at, profiles it in a single pass, and prints the result. It is read-only by design: it never writes, edits, or deletes any file, makes no network calls of any kind, has no auth layer, and implements no administrative operations. It can only read files your operating-system user already has access to.

## What xray stores

xray stores nothing. There is no config directory, no history file, no cache, no telemetry, no analytics, and no remote logging. It reads a file, writes its report to standard output, and exits.

## Verifying releases

Every GitHub release includes a `.sha256` file next to each archive listing its SHA-256 hash. Verify any download before running it:

    sha256sum x-ray-x86_64-unknown-linux-gnu.tar.xz
    # compare against the value in x-ray-x86_64-unknown-linux-gnu.tar.xz.sha256

Release artifacts are built by GitHub Actions from a tagged commit using the cargo-dist configuration in this repo (`dist-workspace.toml` and the generated `.github/workflows/release.yml`). The workflow and build configuration are public and auditable.
