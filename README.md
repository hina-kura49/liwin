# liwin

An AeroSpace-like (i3-style) tiling workflow for GNOME on Wayland, running on
top of the stock GNOME Shell.

> Status: skeleton. Windows can be listed, moved and focused end to end, and
> keybindings are wired through, but there is no automatic tiling yet.

- `crates/liwin-core` — pure window tree + layout logic (no GNOME, no D-Bus)
- `crates/liwin-daemon` — D-Bus service (`io.github.liwin.Daemon`) and config
- `crates/liwin-cli` — the `liwin` command
- `extension/` — thin GNOME Shell extension (`liwin@kura`, TypeScript → GJS)

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and
[docs/dbus.md](docs/dbus.md).

## Requirements

- GNOME Shell 50 on Wayland
- Rust (stable), Node.js + npm, [`just`](https://github.com/casey/just)
- `dbus-daemon` (for the D-Bus integration tests)
- For the visible nested Shell: `mutter-dev-bin` (Ubuntu) / `mutter-devkit`

## Usage

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

## Configuration

`~/.config/liwin/liwin.toml`, using AeroSpace key names:

```toml
config-version = 2

[mode.main.binding]
alt-h = 'focus left'
alt-j = 'focus down'
alt-k = 'focus up'
alt-l = 'focus right'
```

Keys already used by GNOME are skipped (never stolen) and reported in the
daemon log.
