# liwin

An AeroSpace-like (i3-style) tiling workflow for GNOME on Wayland, running on
top of the stock GNOME Shell.

<p align="center">
  <img src="docs/images/demo.gif" width="960" alt="Illustration: new windows on a GNOME desktop are tiled automatically next to each other, then focused, moved and resized from the keyboard">
</p>
<p align="center"><sub>Illustration of the workflow. The key names are examples; bindings are set in <code>liwin.toml</code>.</sub></p>

## Features

- **Automatic tiling.** Windows are tiled per GNOME workspace and monitor as
  they open and close — no manual arranging.
- **Keyboard-driven.** Focus, move, layout and workspace commands, bound to
  keys in an AeroSpace-style config.
- **Plays nice with GNOME.** Runs on top of the stock GNOME Shell. Keys
  already used by GNOME are skipped, never stolen.
- **Scriptable.** Everything goes through a D-Bus service, so the `liwin`
  command (and your own scripts) can drive it.

## Status

> Status: early. Windows are tiled automatically per GNOME workspace and
> monitor; focus/move/layout/workspace commands and keybindings work. Test it
> in the nested Shell (`just nested`) before using it on your real desktop.

## Project layout

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

## License

liwin is made of parts under different licenses:

| Part | License | License files |
|------|---------|---------------|
| GNOME Shell extension (`extension/`) | `GPL-2.0-or-later` | [LICENSE-GPL-2.0](LICENSE-GPL-2.0) |
| Rust crates (`crates/`: liwin-core, liwin-daemon, liwin-cli) | `MIT OR Apache-2.0`, at your option | [LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE) |
| Build and test tooling (`justfile`, `scripts/`) | `MIT OR Apache-2.0`, at your option | [LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE) |

Each source file carries an `SPDX-License-Identifier` header stating its
license.

The extension runs inside GNOME Shell, which is GPL-2.0-or-later. The daemon
and CLI are separate programs that talk to it only over D-Bus.

Unless you explicitly state otherwise, any contribution you intentionally
submit for inclusion in the Rust crates, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
