#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Runs a nested GNOME Shell with the liwin extension and daemon, isolated
# from the logged-in session:
#   - a private session bus (dbus-run-session),
#   - XDG_CONFIG_HOME / XDG_DATA_HOME / XDG_CACHE_HOME under .sandbox/, so
#     dconf (enabled-extensions, ...) and the extension directory are not the
#     real ones.
#
# usage: scripts/nested.sh devkit|headless
#   devkit   – visible window via mutter-devkit (GNOME 49+, `mutter-dev-bin`)
#   headless – no window, a virtual monitor (for automated tests)
#
# While it runs, `source .sandbox/env` in another terminal to talk to the
# nested session bus (e.g. `liwin list-windows`).
set -euo pipefail

mode=${1:-devkit}
root=$(cd "$(dirname "$0")/.." && pwd)
sb="$root/.sandbox"
ext_src="$root/extension/build/liwin@kura"
daemon="$root/target/debug/liwin-daemon"

[[ -d "$ext_src" ]] || { echo "extension not built: run 'just ext-build'" >&2; exit 1; }
[[ -x "$daemon" ]] || { echo "daemon not built: run 'just build'" >&2; exit 1; }
case "$mode" in devkit|headless) ;; *) echo "unknown mode '$mode'" >&2; exit 2 ;; esac

mkdir -p "$sb/config" "$sb/data/gnome-shell/extensions" "$sb/cache"
rm -rf "$sb/data/gnome-shell/extensions/liwin@kura"
cp -r "$ext_src" "$sb/data/gnome-shell/extensions/"

export XDG_CONFIG_HOME="$sb/config" XDG_DATA_HOME="$sb/data" XDG_CACHE_HOME="$sb/cache"
export LIWIN_SANDBOX="$sb" LIWIN_MODE="$mode" LIWIN_DAEMON="$daemon"
# D-Bus-activated helpers (e.g. gnome-shell-calendar-server) can outlive the
# private bus. Stop exactly the processes carrying our sandbox environment.
reap_sandbox() {
    local pid
    for pid in $(pgrep -u "$(id -u)"); do
        [[ $pid == "$$" ]] && continue
        if tr '\0' '\n' <"/proc/$pid/environ" 2>/dev/null | grep -qxF "XDG_CONFIG_HOME=$sb/config"; then
            kill "$pid" 2>/dev/null || true
        fi
    done
}
trap reap_sandbox EXIT
dbus-run-session -- "$root/scripts/nested-inner.sh"
