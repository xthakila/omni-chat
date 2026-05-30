#!/usr/bin/env bash
# rss-sampler.sh — sum RSS + PSS across the whole OmniChat (CEF) process tree.
#
# Read-only: only reads /proc. CEF spawns a browser process plus one renderer
# per service (and GPU/utility helpers), so a single `ps` line is misleading —
# this sums the whole `omnichat`/`omnichat_helper` tree. PSS (proportional set
# size) is the honest number because CEF shares large read-only mappings across
# renderers; RSS double-counts that shared memory.
#
# Usage: scripts/rss-sampler.sh [label]
set -euo pipefail

label="${1:-omnichat}"

pids="$(pgrep -x 'omnichat|omnichat_helper' || pgrep -f 'omnichat(_helper)?' || true)"
if [ -z "${pids}" ]; then
    echo "rss-sampler: no omnichat processes running" >&2
    exit 1
fi

total_rss=0
total_pss=0
nproc=0
for pid in ${pids}; do
    roll="/proc/${pid}/smaps_rollup"
    [ -r "${roll}" ] || continue
    rss="$(awk '/^Rss:/ {s+=$2} END {print s+0}' "${roll}")"
    pss="$(awk '/^Pss:/ {s+=$2} END {print s+0}' "${roll}")"
    total_rss=$((total_rss + rss))
    total_pss=$((total_pss + pss))
    nproc=$((nproc + 1))
done

printf '%-16s procs=%-3d  RSS=%5d MiB  PSS=%5d MiB\n' \
    "${label}" "${nproc}" "$((total_rss / 1024))" "$((total_pss / 1024))"
