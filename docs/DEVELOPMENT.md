# Developing liwin

This page is for building liwin from source and hacking on it. To just use
liwin, see the [README](../README.md) or the
[product page](https://hina-kura49.github.io/liwin/). How changes get
reviewed, merged and released is in [CONTRIBUTING.md](../CONTRIBUTING.md).

## Project layout

- `crates/liwin-core` — pure window tree + layout logic (no GNOME, no D-Bus)
- `crates/liwin-daemon` — D-Bus service (`io.github.liwin.Daemon`) and config
- `crates/liwin-cli` — the `liwin` command
- `extension/` — thin GNOME Shell extension (`liwin@kura`, TypeScript → GJS)
- `scripts/` — nested Shell / end-to-end helpers and `install.sh` (the
  release installer)
- `site/` — the product page, published to GitHub Pages by
  `.github/workflows/pages.yml`

See [ARCHITECTURE.md](ARCHITECTURE.md) and [dbus.md](dbus.md).

## Requirements

- GNOME Shell 50 on Wayland
- Rust (stable), Node.js + npm, [`just`](https://github.com/casey/just)
- `dbus-daemon` (for the D-Bus integration tests)
- For the visible nested Shell: `mutter-dev-bin` (Ubuntu) / `mutter-devkit`

## Building and testing

```sh
just build        # Rust workspace + extension
just test         # unit + D-Bus integration tests, extension type-check
just e2e          # headless nested Shell: CLI moves a real window
just nested       # nested Shell in a window (devkit), isolated in .sandbox/
```

Nested and headless sessions use a private session bus and keep dconf and
the extension directory under `.sandbox/`, so the logged-in session is not
touched. While one runs, `source .sandbox/env` to talk to it:

```sh
liwin list-windows
liwin focus left
liwin debug move-window <id> 100 100 800 600   # daemon runs with --enable-debug-commands
```

`just ext-install` copies the extension into the real
`~/.local/share/gnome-shell/extensions`; on Wayland it is picked up after
logging out and in.

## Releases

Releases are cut by release-please (`.github/workflows/release-please.yml`).
`.github/workflows/release.yml` builds the assets and uploads them to the
GitHub release. Besides the versioned tarballs it uploads version-less copies
(`liwin-linux-x86_64.tar.gz`, `liwin-linux-aarch64.tar.gz`), the extension
zip, `install.sh` and `SHA256SUMS`, so
`https://github.com/hina-kura49/liwin/releases/latest/download/<name>` always
points at the newest release.
