#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# End-to-end "hello world": start a headless nested GNOME Shell with the
# extension and the daemon, open a window, move it with the CLI, and check
# its new geometry as reported back by the extension.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
sb="$root/.sandbox"
liwin="$root/target/debug/liwin"

mkdir -p "$sb/config/liwin"
rm -f "$sb/env"
cat >"$sb/config/liwin/liwin.toml" <<'TOML'
config-version = 2
[mode.main.binding]
alt-h = 'focus left'
alt-l = 'focus right'
# Super+A is GNOME Shell's "show apps": must be rejected, not stolen.
cmd-a = 'focus up'
TOML
"$root/scripts/nested.sh" headless >"$sb/e2e-shell.out" 2>&1 &
nested_pid=$!
cleanup() {
    [[ -n "${app_pid:-}" ]] && kill "$app_pid" 2>/dev/null || true
    kill "$nested_pid" 2>/dev/null || true
    # Stop only the nested Shell (matching on its own command line, never on
    # a pattern that could match this script or other processes).
    local pid
    for pid in $(pgrep -x gnome-shell || true); do
        if tr '\0' ' ' <"/proc/$pid/cmdline" 2>/dev/null | grep -q -- "--wayland-display liwin-nested-0"; then
            kill -TERM "$pid" 2>/dev/null || true
        fi
    done
    wait "$nested_pid" 2>/dev/null || true
}
trap cleanup EXIT

wait_for() { # timeout-seconds description command...
    local t=$1 what=$2; shift 2
    for _ in $(seq $((t * 10))); do "$@" >/dev/null 2>&1 && return 0; sleep 0.1; done
    echo "e2e: timed out waiting for $what" >&2
    echo "--- shell log ---" >&2; tail -n 40 "$sb/shell.log" >&2 || true
    echo "--- daemon log ---" >&2; tail -n 40 "$sb/daemon.log" >&2 || true
    exit 1
}

wait_for 30 "sandbox env" test -f "$sb/env"
# shellcheck disable=SC1091
source "$sb/env"
connected() {
    busctl --user get-property io.github.liwin.Daemon /io/github/liwin/Daemon \
        io.github.liwin.Daemon1 ShellConnected | grep -q true
}
wait_for 30 "extension to connect to the daemon" connected
echo "e2e: extension connected"

GDK_BACKEND=wayland gnome-text-editor --new-window >"$sb/app.log" 2>&1 &
app_pid=$!
count_windows() { [[ $("$liwin" list-windows | grep -c .) -eq $1 ]]; }
wait_for 30 "the first window to be reported" count_windows 1
GDK_BACKEND=wayland gnome-text-editor --new-window >>"$sb/app.log" 2>&1 || true
wait_for 30 "the second window to be reported" count_windows 2

# Geometry as reported by the Shell (after a fresh snapshot), one line per
# window in tree order: "<id> <x> <y> <w> <h> <workspace>".
geometry() {
    "$liwin" debug resync
    sleep 0.3
    "$liwin" list-windows | tr -d '*' | awk '{
        split($4, g, /[x+]/); sub("workspace=", "", $2)
        print $1, g[3], g[4], g[1], g[2], $2 }'
}

# Two windows on workspace 1 share the work area side by side.
side_by_side() {
    geometry | awk 'NR==1{x1=$2;y1=$3;w1=$4} NR==2{x2=$2;y2=$3;w2=$4}
        END{ exit !(w1==w2 && y1==y2 && (x2==x1+w1 || x1==x2+w2)) }'
}
wait_for 10 "two windows to be tiled side by side" side_by_side
geometry
echo "e2e: two windows tiled side by side"

# `move left` swaps them.
focused=$("$liwin" list-windows | awk '$1=="*"{print $2}')
x_before=$(geometry | awk -v id="$focused" '$1==id{print $2}')
"$liwin" move left
moved_left() { [[ $(geometry | awk -v id="$focused" '$1==id{print $2}') -lt $x_before ]]; }
wait_for 10 "move left to swap the windows" moved_left
echo "e2e: move left swapped the windows"

# Sending the focused window to workspace 2 leaves the other one alone, at
# full width.
width_before=$(geometry | awk -v id="$focused" '$1==id{print $4}')
"$liwin" move-node-to-workspace 2
alone() {
    geometry | awk -v id="$focused" -v w="$width_before" '
        $1==id{ws=$6} $1!=id{other=$4}
        END{ exit !(ws==2 && other==2*w) }'
}
wait_for 10 "the remaining window to fill workspace 1" alone
geometry
echo "e2e: move-node-to-workspace 2 re-tiled workspace 1"

# Keybindings were sent to the extension, grabbed, and acknowledged.
applied() { sed 's/\x1b\[[0-9;]*m//g' "$sb/daemon.log" | grep -q 'KeybindingsApplied grabbed=2 rejected=\[2\]'; }
wait_for 5 "keybindings to be applied" applied
echo "e2e: alt-h/alt-l grabbed, cmd-a (GNOME show-apps) rejected"

# disable() must say Goodbye and release everything without errors.
gnome-extensions disable liwin@kura
disconnected() { ! connected; }
wait_for 10 "the daemon to see the extension go away" disconnected
echo "e2e: extension disabled, daemon notified"
if grep -E "JS ERROR|liwin.*(rror|failed)" "$sb/shell.log"; then
    echo "e2e: errors in the shell log" >&2
    exit 1
fi
echo "e2e: OK"
