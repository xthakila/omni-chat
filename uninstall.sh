#!/bin/bash
# OmniChat uninstaller. Removes the binaries, launcher, desktop entry, icon, and
# any legacy alias .desktop files. Keeps your data (services, settings, sessions)
# unless you pass --purge.
#
# Usage:
#   ./uninstall.sh [PREFIX] [--purge]
# PREFIX defaults to $HOME/.local (must match what install.sh used).
set -e

PREFIX="$HOME/.local"
PURGE=0
for arg in "$@"; do
    case "$arg" in
        --purge) PURGE=1 ;;
        *) PREFIX="$arg" ;;
    esac
done

INSTALL_DIR="$PREFIX/lib/omnichat"
BIN_DIR="$PREFIX/bin"
APP_DIR="$PREFIX/share/applications"
ICON_DIR="$PREFIX/share/icons/hicolor/256x256/apps"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/omnichat"

echo "Uninstalling OmniChat (prefix: $PREFIX)"

rm -rf "$INSTALL_DIR"
rm -f "$BIN_DIR/omnichat"
rm -f "$APP_DIR/omnichat.desktop"
rm -f "$ICON_DIR/omnichat.png"

# Legacy alias .desktop files created by older installers (the "5 installs").
for legacy in chromium cef OmniChat Omnichat; do
    rm -f "$APP_DIR/$legacy.desktop"
done

update-desktop-database "$APP_DIR" 2>/dev/null || true
gtk-update-icon-cache "$PREFIX/share/icons/hicolor" 2>/dev/null || true

echo "Removed binaries, launcher, desktop entry, icon, and any legacy aliases."

if [ "$PURGE" = "1" ]; then
    rm -rf "$DATA_DIR"
    echo "Purged data directory: $DATA_DIR"
else
    echo "Your data (services, settings, sessions) is kept at:"
    echo "  $DATA_DIR"
    echo "Remove it too with:  $0 --purge"
fi
