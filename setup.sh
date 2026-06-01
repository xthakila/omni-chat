#!/usr/bin/env bash
# OmniChat one-shot setup: system libraries + Rust + CEF runtime + build + install.
# Idempotent — safe to re-run. After the first setup, use ./update.sh to upgrade.
#
#   ./setup.sh                 # full bootstrap
#   JOBS=4 ./setup.sh          # cap build parallelism (low-RAM / thin laptops)
#   CEF_PATH=/opt/cef ./setup.sh   # custom CEF location
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CEF_DIR="${CEF_PATH:-$HOME/.local/share/cef}"

say()  { printf '\033[1;36m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33mwarning:\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

say "OmniChat setup — system libraries, Rust, CEF, build, install"
echo

# --- 1. System build libraries -------------------------------------------------
# Need: C/C++ toolchain + cmake (CEF's build bits), pkg-config, GTK3 dev headers
# (tray + window), and libxdo (window focus). Runtime libs come with the -dev pkgs.
DEPS_APT="build-essential pkg-config cmake libgtk-3-dev libxdo-dev"

# Are all build deps already present? If so, skip the package step entirely — no
# sudo needed (keeps re-runs idempotent and works in passwordless/CI shells).
deps_satisfied() {
    command -v cc        >/dev/null 2>&1 || return 1
    command -v cmake     >/dev/null 2>&1 || return 1
    command -v pkg-config >/dev/null 2>&1 || return 1
    pkg-config --exists gtk+-3.0 2>/dev/null || return 1
    pkg-config --exists libxdo 2>/dev/null \
        || ldconfig -p 2>/dev/null | grep -q 'libxdo\.so' \
        || [ -e /usr/include/xdo.h ] || return 1
    return 0
}

if deps_satisfied; then
    say "Build dependencies already satisfied — skipping system package install"
elif command -v apt-get >/dev/null 2>&1; then
    say "Installing system libraries (apt): $DEPS_APT"
    sudo apt-get update -qq
    sudo apt-get install -y $DEPS_APT
elif command -v dnf >/dev/null 2>&1; then
    say "Installing system libraries (dnf)"
    sudo dnf install -y gcc gcc-c++ make cmake pkgconf-pkg-config gtk3-devel libxdo-devel
elif command -v pacman >/dev/null 2>&1; then
    say "Installing system libraries (pacman)"
    sudo pacman -S --needed --noconfirm base-devel cmake pkgconf gtk3 xdotool
else
    warn "No supported package manager (apt/dnf/pacman) detected."
    warn "Install the equivalents of these and re-run: $DEPS_APT"
fi
echo

# --- 2. Rust toolchain ---------------------------------------------------------
if ! command -v cargo >/dev/null 2>&1; then
    say "Rust not found — installing via rustup (non-interactive)"
    command -v curl >/dev/null 2>&1 || die "curl is required to install Rust; install it and re-run."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi
# Put cargo + cargo-installed bins (export-cef-dir) on PATH for this shell.
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
command -v cargo >/dev/null 2>&1 || die "cargo still not on PATH — open a new shell and re-run."
say "Using $(cargo --version)"
echo

# --- 3. CEF runtime ------------------------------------------------------------
# The required CEF version is pinned by the `cef` crate; export-cef-dir downloads
# the matching build, so we never hardcode a version (a mismatch breaks linking).
if [ -f "$CEF_DIR/libcef.so" ]; then
    say "CEF already present at $CEF_DIR (skipping ~300 MB download)"
else
    say "Installing CEF runtime to $CEF_DIR (~300 MB, once)"
    command -v export-cef-dir >/dev/null 2>&1 || cargo install export-cef-dir
    mkdir -p "$CEF_DIR"
    export-cef-dir --force "$CEF_DIR"
fi
[ -f "$CEF_DIR/libcef.so" ] || die "CEF install failed: $CEF_DIR/libcef.so is missing."
echo

# --- 4. Build ------------------------------------------------------------------
export CEF_PATH="$CEF_DIR"
export LD_LIBRARY_PATH="$CEF_DIR:${LD_LIBRARY_PATH:-}"
JOBS_ARG=""
[ -n "${JOBS:-}" ] && JOBS_ARG="-j${JOBS}"   # e.g. JOBS=4 on low-RAM machines
say "Building release${JOBS_ARG:+ ($JOBS_ARG)} — a few minutes on the first build"
( cd "$SCRIPT_DIR" && cargo build --release $JOBS_ARG )
echo

# --- 5. Install ----------------------------------------------------------------
say "Installing launcher, recipes, and desktop entry"
bash "$SCRIPT_DIR/install.sh"
echo
say "Done. Launch with 'omnichat' or from your application launcher."
say "Update later with:  $SCRIPT_DIR/update.sh"
