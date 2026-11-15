#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Runs inside the private session bus created by scripts/nested.sh.
set -euo pipefail
sb="$LIWIN_SANDBOX"
display=liwin-nested-0

# Safety net: never touch the real user's dconf database.
[[ "$XDG_CONFIG_HOME" == "$sb/config" ]] || { echo "refusing: XDG_CONFIG_HOME is not sandboxed" >&2; exit 1; }

gsettings set org.gnome.shell enabled-extensions "['liwin@kura']"
# A previous run may have disabled it (disabled-extensions wins over enabled).
gsettings set org.gnome.shell disabled-extensions "[]"
gsettings set org.gnome.shell disable-user-extensions false
gsettings set org.gnome.shell welcome-dialog-last-shown-version '999'

cat > "$sb/env" <<ENV
export DBUS_SESSION_BUS_ADDRESS='$DBUS_SESSION_BUS_ADDRESS'
export WAYLAND_DISPLAY='$display'
export XDG_CONFIG_HOME='$XDG_CONFIG_HOME' XDG_DATA_HOME='$XDG_DATA_HOME' XDG_CACHE_HOME='$XDG_CACHE_HOME'
ENV

# The daemon reads .sandbox/config/liwin/liwin.toml via XDG_CONFIG_HOME.
"$LIWIN_DAEMON" --enable-debug-commands >"$sb/daemon.log" 2>&1 &
daemon_pid=$!
trap 'kill $daemon_pid 2>/dev/null || true; rm -f "$sb/env"' EXIT

case "$LIWIN_MODE" in
    devkit)   args=(--devkit --wayland) ;;
    headless) args=(--headless --virtual-monitor 1280x800) ;;
esac
gnome-shell "${args[@]}" --no-x11 --wayland-display "$display" 2>&1 | tee "$sb/shell.log"
