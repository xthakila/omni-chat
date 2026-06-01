#!/usr/bin/env bash
# OmniChat update: pull latest, rebuild, reinstall.
# Assumes Rust + CEF are already installed — run ./setup.sh for first-time setup.
#
#   ./update.sh                # pull + rebuild + reinstall
#   JOBS=4 ./update.sh         # cap build parallelism (low-RAM / thin laptops)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CEF_DIR="${CEF_PATH:-$HOME/.local/share/cef}"

say()  { printf '\033[1;36m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33mwarning:\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

cd "$SCRIPT_DIR"

# --- Prerequisites present? ----------------------------------------------------
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
command -v cargo >/dev/null 2>&1 || die "cargo not found — run ./setup.sh first."
[ -f "$CEF_DIR/libcef.so" ] || die "CEF not found at $CEF_DIR — run ./setup.sh first."

# --- Pull latest ---------------------------------------------------------------
if [ -d .git ]; then
    if [ -n "$(git status --porcelain)" ]; then
        warn "Working tree has local changes — skipping 'git pull' to avoid conflicts."
        warn "Commit or stash them to pull, or just rebuild the current tree as-is."
    else
        say "Pulling latest (fast-forward only)"
        git pull --ff-only || die "git pull failed (diverged history) — resolve manually."
    fi
else
    warn "Not a git checkout — rebuilding the current tree without pulling."
fi

# --- Rebuild -------------------------------------------------------------------
export CEF_PATH="$CEF_DIR"
export LD_LIBRARY_PATH="$CEF_DIR:${LD_LIBRARY_PATH:-}"
JOBS_ARG=""
[ -n "${JOBS:-}" ] && JOBS_ARG="-j${JOBS}"
say "Building release${JOBS_ARG:+ ($JOBS_ARG)}"
cargo build --release $JOBS_ARG

# --- Reinstall -----------------------------------------------------------------
say "Reinstalling"
bash "$SCRIPT_DIR/install.sh"

# --- Restart hint --------------------------------------------------------------
if pgrep -x omnichat >/dev/null 2>&1; then
    warn "OmniChat is running the OLD binary — restart to pick up the update:"
    warn "    pkill -x omnichat && omnichat &"
fi
say "Update complete."
