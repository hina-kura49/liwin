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

# Start every run from GNOME defaults: settings written by an earlier run
# (e.g. a rebound shortcut, or disabled-extensions after a test that disabled
# liwin) would otherwise change what this run tests.
rm -rf "$sb/config/dconf"
mkdir -p "$sb/config" "$sb/data/gnome-shell/extensions" "$sb/cache"
rm -rf "$sb/data/gnome-shell/extensions/liwin@kura"
cp -r "$ext_src" "$sb/data/gnome-shell/extensions/"

export XDG_CONFIG_HOME="$sb/config" XDG_DATA_HOME="$sb/data" XDG_CACHE_HOME="$sb/cache"

# GNOME Shell keeps a crash sentinel (gnome-shell-disable-extensions) in the
# runtime dir for its first 60 s; if the *logged-in* Shell crashed while a
# nested one had it there, the user's extensions would be disabled. The
# headless Shell needs nothing from the real runtime dir, so give it its own.
# The devkit window must talk to the real compositor and keeps the real one.
#
# The private dir is a short-named subdirectory of the user's runtime dir, not
# a path inside the checkout: Wayland socket paths must fit in 108 bytes, which
# a deep checkout path exceeds. The sentinel then lands in the subdirectory,
# where the logged-in session never looks.
run_dir=""
if [[ "$mode" == headless ]]; then
    run_dir=$(mktemp -d "${XDG_RUNTIME_DIR:-/tmp}/liwin-nested.XXXXXX")
    chmod 700 "$run_dir"
    export XDG_RUNTIME_DIR="$run_dir"
fi
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
# Helpers of the nested session (gvfsd-fuse, xdg-document-portal) mount FUSE
# filesystems in the private runtime dir; unmount them once they are gone,
# then remove the dir.
unmount_sandbox() {
    [[ -n "$run_dir" ]] || return 0
    local mnt
    for mnt in $(findmnt -rn -o TARGET | grep -F "$run_dir/" || true); do
        fusermount3 -u -z "$mnt" 2>/dev/null || fusermount -u -z "$mnt" 2>/dev/null || true
    done
    # Never delete through a mount: the document portal exposes real files.
    if findmnt -rn -o TARGET | grep -qF "$run_dir/"; then
        echo "nested.sh: $run_dir still has mounts; not removing it" >&2
        return 0
    fi
    rm -rf --one-file-system "$run_dir"
}
trap 'reap_sandbox; sleep 0.5; unmount_sandbox' EXIT
dbus-run-session -- "$root/scripts/nested-inner.sh"
