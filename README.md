# liwin

An AeroSpace-like (i3-style) tiling workflow for GNOME on Wayland, running on
top of the stock GNOME Shell.

> Status: early. Windows are tiled automatically per GNOME workspace and
> monitor; focus/move/layout/workspace commands and keybindings work. Test it
> in the nested Shell (`just nested`) before using it on your real desktop.

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
