#!/bin/sh
# xray — uninstaller
#
# Removes the xray binary installed by install.sh. xray stores nothing else on
# disk (no config, no history), so this is the entire cleanup.
#
#     curl --proto '=https' --tlsv1.2 -LsSf https://raw.githubusercontent.com/excelano/xray/main/uninstall.sh | sh

set -eu

if [ -n "${CARGO_HOME:-}" ]; then
    install_dir="$CARGO_HOME/bin"
else
    install_dir="$HOME/.cargo/bin"
fi

target="$install_dir/xray"

if [ -e "$target" ]; then
    rm -f "$target"
    echo "Removed $target"
elif command -v xray >/dev/null 2>&1; then
    found="$(command -v xray)"
    echo "xray is installed at $found, not the expected location ($target)."
    echo "Remove it manually if you want it gone."
    exit 1
else
    echo "xray is not installed."
fi
