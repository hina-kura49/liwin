#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Installs liwin for the current user from the GitHub release:
#   - liwin and liwin-daemon      -> ~/.local/bin (or $LIWIN_BIN_DIR)
#   - the GNOME Shell extension   -> ~/.local/share/gnome-shell/extensions
#   - a systemd user unit that starts liwin-daemon with the graphical session
#
#   curl -fsSL https://github.com/hina-kura49/liwin/releases/latest/download/install.sh | bash
#
# Environment:
#   LIWIN_VERSION=1.0.0   install that release instead of the latest one
#   LIWIN_BIN_DIR=DIR     where to put the binaries (default ~/.local/bin)
#   LIWIN_NO_SERVICE=1    do not install/enable the systemd user unit
#
# `install.sh --uninstall` removes everything this script installed (your
# ~/.config/liwin/liwin.toml is kept).
set -euo pipefail

REPO="hina-kura49/liwin"
UUID="liwin@kura"
BIN_DIR="${LIWIN_BIN_DIR:-$HOME/.local/bin}"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
CONFIG_HOME="${XDG_CONFIG_HOME:-$HOME/.config}"
EXT_DIR="$DATA_HOME/gnome-shell/extensions/$UUID"
UNIT_DIR="$CONFIG_HOME/systemd/user"
UNIT="$UNIT_DIR/liwin.service"

say() { printf '\033[1;38;5;202m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33mwarning:\033[0m %s\n' "$*" >&2; }
die() {
    printf '\033[1;31merror:\033[0m %s\n' "$*" >&2
    exit 1
}
have() { command -v "$1" >/dev/null 2>&1; }

uninstall() {
    if have systemctl && [ -f "$UNIT" ]; then
        systemctl --user disable --now liwin.service 2>/dev/null || true
    fi
    rm -f "$UNIT" "$BIN_DIR/liwin" "$BIN_DIR/liwin-daemon"
    have systemctl && systemctl --user daemon-reload 2>/dev/null || true
    if have gnome-extensions; then
        gnome-extensions disable "$UUID" 2>/dev/null || true
    fi
    rm -rf "$EXT_DIR"
    say "liwin removed. Log out and back in to unload the extension."
}

if [ "${1:-}" = "--uninstall" ]; then
    uninstall
    exit 0
elif [ $# -gt 0 ]; then
    die "unknown argument: $1 (only --uninstall is accepted)"
fi

[ "$(uname -s)" = Linux ] || die "liwin runs on Linux (GNOME on Wayland) only"
case "$(uname -m)" in
    x86_64 | amd64) arch=x86_64 ;;
    aarch64 | arm64) arch=aarch64 ;;
    *) die "unsupported CPU architecture: $(uname -m) (x86_64 and aarch64 are available)" ;;
esac
for tool in curl tar sha256sum; do
    have "$tool" || die "'$tool' is required"
done

if [ -n "${LIWIN_VERSION:-}" ]; then
    base="https://github.com/$REPO/releases/download/v${LIWIN_VERSION#v}"
else
    base="https://github.com/$REPO/releases/latest/download"
fi
tarball="liwin-linux-$arch.tar.gz"
zip="$UUID.shell-extension.zip"

if have gnome-shell; then
    shell_version="$(gnome-shell --version 2>/dev/null | awk '{print $3}')"
    case "$shell_version" in
        50 | 50.*) ;;
        *) warn "GNOME Shell ${shell_version:-?} found; liwin supports GNOME Shell 50" ;;
    esac
else
    warn "gnome-shell not found; liwin needs GNOME Shell 50 on Wayland"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

say "Downloading $tarball and $zip from $base"
for file in "$tarball" "$zip" SHA256SUMS; do
    curl -fsSL --retry 3 -o "$tmp/$file" "$base/$file" || die "download failed: $base/$file"
done
(cd "$tmp" && grep -E " (\*)?($tarball|$zip)\$" SHA256SUMS | sha256sum -c --quiet -) ||
    die "checksum mismatch, refusing to install"

say "Installing liwin and liwin-daemon to $BIN_DIR"
mkdir -p "$tmp/x" "$BIN_DIR"
tar -xzf "$tmp/$tarball" -C "$tmp/x"
for bin in liwin liwin-daemon; do
    src="$(find "$tmp/x" -type f -name "$bin" | head -n 1)"
    [ -n "$src" ] || die "$bin not found in $tarball"
    install -m 755 "$src" "$BIN_DIR/$bin"
done
"$BIN_DIR/liwin" --version

say "Installing the GNOME Shell extension ($UUID)"
if have gnome-extensions; then
    gnome-extensions install --force "$tmp/$zip"
elif have unzip; then
    rm -rf "$EXT_DIR"
    mkdir -p "$EXT_DIR"
    unzip -q "$tmp/$zip" -d "$EXT_DIR"
else
    die "need gnome-extensions or unzip to install the extension"
fi

# Pre-enable the extension: on Wayland GNOME Shell only sees a newly installed
# extension after logging out and in, so `gnome-extensions enable` cannot be
# used yet. Adding it to enabled-extensions makes it start with the next login.
if have gsettings; then
    enabled="$(gsettings get org.gnome.shell enabled-extensions 2>/dev/null || true)"
    case "$enabled" in
        *"'$UUID'"*) ;;
        "" ) warn "could not read org.gnome.shell enabled-extensions" ;;
        "@as []" | "[]") gsettings set org.gnome.shell enabled-extensions "['$UUID']" ;;
        *) gsettings set org.gnome.shell enabled-extensions "${enabled%]}, '$UUID']" ;;
    esac
fi

if [ -z "${LIWIN_NO_SERVICE:-}" ] && have systemctl; then
    say "Installing the systemd user unit $UNIT"
    mkdir -p "$UNIT_DIR"
    cat >"$UNIT" <<EOF
[Unit]
Description=liwin tiling daemon
Documentation=https://github.com/$REPO
PartOf=graphical-session.target
After=graphical-session.target

[Service]
ExecStart=$BIN_DIR/liwin-daemon
Restart=on-failure

[Install]
WantedBy=graphical-session.target
EOF
    systemctl --user daemon-reload || true
    if systemctl --user enable liwin.service; then
        systemctl --user restart liwin.service 2>/dev/null ||
            warn "could not start liwin.service now; it starts with your next login"
    fi
else
    warn "no systemd user unit installed: start '$BIN_DIR/liwin-daemon' with your session yourself"
fi

case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) warn "$BIN_DIR is not on your PATH; add it to use the 'liwin' command" ;;
esac

cat <<EOF

liwin is installed. Next steps:
  1. Log out and log back in (GNOME on Wayland loads new extensions at login).
  2. If the extension is not active, run: gnome-extensions enable $UUID
  3. Open a few windows. Alt+H/J/K/L moves focus, Alt+Shift+H/J/K/L moves windows.
  Optional: customise keys in ~/.config/liwin/liwin.toml

Uninstall: curl -fsSL $base/install.sh | bash -s -- --uninstall
EOF
