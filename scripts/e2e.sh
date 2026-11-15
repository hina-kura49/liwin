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
    pkill -TERM -f "gnome-shell .*--wayland-display liwin-nested-0" 2>/dev/null || true
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
window_listed() { [[ -n "$("$liwin" list-windows)" ]]; }
wait_for 30 "a window to be reported" window_listed
"$liwin" list-windows
id=$("$liwin" list-windows | head -n1 | awk '{print ($1 == "*") ? $2 : $1}')

echo "e2e: moving window $id to 100,120 640x480"
"$liwin" debug move-window "$id" 100 120 640 480

moved() {
    "$liwin" debug resync
    sleep 0.2
    "$liwin" list-windows | grep -E "^.\s$id\s" | grep -q "640x480+100+120"
}
wait_for 10 "the window geometry to update" moved
"$liwin" list-windows

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
