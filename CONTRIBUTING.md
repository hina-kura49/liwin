# Contributing to liwin

Thanks for your interest in liwin! Bug reports, ideas and pull requests are
welcome. This document explains how to set up a development environment, how
to test safely, and how changes get merged and released.

Please follow the [Code of Conduct](CODE_OF_CONDUCT.md). Security issues go
through private reporting, see [SECURITY.md](SECURITY.md).

## Development setup

liwin targets **GNOME Shell 50 on Wayland**. You need:

- Rust stable via [rustup](https://rustup.rs/) (with `rustfmt` and `clippy`:
  `rustup component add rustfmt clippy`)
- Node.js (22 is what CI uses) and npm, for the TypeScript extension
- [`just`](https://github.com/casey/just) (`cargo install just` or your
  distro's package)
- `dbus-daemon` (package `dbus`), used by the D-Bus integration tests
- GNOME Shell 50 itself, for `just e2e` / `just nested`
- For the visible nested Shell: `mutter-dev-bin` (Ubuntu) / `mutter-devkit`
  (other distros), which provides the devkit window

Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for how the daemon, the
CLI and the extension fit together and [docs/dbus.md](docs/dbus.md) for the
D-Bus protocol between them.

## Building and testing

```sh
just build    # Rust workspace + extension (TypeScript -> extension/build/liwin@kura)
just test     # Rust unit + D-Bus integration tests, extension type-check
just lint     # cargo fmt --check, clippy -D warnings, tsc --noEmit
just e2e      # headless nested Shell: extension + daemon + CLI move a real window
just nested   # nested Shell in a window (devkit) to try things by hand
```

`just test` and `just lint` are what CI runs; please run them before opening
a pull request.

### Never test on your logged-in session

The extension runs *inside* GNOME Shell. A bug in it (or a stuck grab, a
leaked keybinding, a crash) affects the compositor that draws your desktop,
and on Wayland a crashed Shell takes the whole session with it. GNOME Shell
also remembers crashes: if it crashes while an extension is loaded it can
disable *all* user extensions. Do not develop by installing the extension into
your real session and reloading it.

`just e2e` and `just nested` instead start a separate, nested GNOME Shell
(`scripts/nested.sh`):

- it runs on a private session bus (`dbus-run-session`), so the nested
  daemon, extension and CLI never talk to your real session's bus;
- `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and `XDG_CACHE_HOME` point into
  `.sandbox/`, so dconf (`enabled-extensions`, keybindings, input sources)
  and the extension directory are sandboxed copies, and the inner script
  refuses to run if they are not;
- the headless variant also gets its own `XDG_RUNTIME_DIR`, so GNOME Shell's
  crash sentinel cannot disable the extensions of your real Shell;
- the nested Shell has no input method, so it cannot disturb your IME;
- on exit, only processes carrying the sandbox environment are stopped.

While a nested Shell runs, `source .sandbox/env` in another terminal to talk
to it, e.g. `liwin list-windows`. The daemon there runs with
`--enable-debug-commands`, so `liwin debug ...` works.

Only `just ext-install` touches your real `~/.local/share/gnome-shell/extensions`.
Use it to try a finished change on your desktop, not as a development loop.

### D-Bus introspection snapshot

The D-Bus protocol is frozen by a snapshot test
(`crates/liwin-daemon/tests/introspection.rs` against
`crates/liwin-daemon/tests/introspection.xml`). If you change the protocol on
purpose, update [docs/dbus.md](docs/dbus.md) and refresh the snapshot:

```sh
LIWIN_BLESS=1 cargo test -p liwin-daemon --test introspection
```

and commit the updated XML together with the change. Protocol changes need
both sides (daemon and extension) to be updated in the same pull request.

## Commits and pull requests

liwin uses [Conventional Commits](https://www.conventionalcommits.org/) and
**squash merges**: every pull request becomes one commit on `main`, and its
message is the **pull request title**. A check enforces the format of the
title:

```
<type>(<optional scope>): <description>
```

Allowed types: `feat`, `fix`, `docs`, `refactor`, `test`, `ci`, `build`,
`perf`, `chore`, `revert`. Scopes are optional, e.g. `daemon`, `cli`, `core`,
`extension`. Mark breaking changes with `!` (`feat(cli)!: ...`) and explain
them in the description.

The type decides the next version and the CHANGELOG entry (see
[Releases](#releases)). While liwin is at 0.x, a breaking change (`!`) or a
`feat` bumps the minor version (0.1.0 -> 0.2.0), and `fix`, `perf`, `revert`,
`docs` and `refactor` bump the patch version and are listed in the CHANGELOG.
`test`, `ci`, `build` and `chore` are not listed and do not cause a release
on their own.

The commits inside your branch do not need to follow any format, but keeping
them small and focused helps review.

### Pull request flow

1. For anything bigger than a small fix, open an issue first to discuss the
   approach.
2. Fork the repository and create a branch from `main`.
3. Make your change with tests. `liwin-core` logic should get unit or
   property tests; daemon behavior tests live in `crates/liwin-daemon/tests/`.
4. Run `just lint` and `just test`, and `just e2e` if you touched the
   extension or the daemon/extension protocol.
5. Open a pull request with a Conventional Commits title and fill in the
   template. Draft pull requests are welcome for early feedback.
6. The maintainer ([@hina-kura49](https://github.com/hina-kura49)) reviews
   every pull request and squash-merges it once CI is green.

## Releases

Releases are automated with
[release-please](https://github.com/googleapis/release-please). It keeps a
"chore: release X.Y.Z" pull request open on `main` that bumps the single
project version (Cargo workspace, `Cargo.lock`, `extension/package.json`,
`extension/metadata.json`) and updates [CHANGELOG.md](CHANGELOG.md).
Merging it tags `vX.Y.Z`, creates the GitHub release, and a workflow attaches
the Linux binaries (x86_64, aarch64), the extension zip, `SHA256SUMS` and
build provenance attestations. Contributors do not need to bump versions.

## Licensing of contributions

liwin is made of parts under different licenses (see the
[README](README.md#license)). By contributing you agree that your
contribution is licensed under the license of the part you change:

| Directory | License |
|-----------|---------|
| `extension/` | `GPL-2.0-or-later` |
| `crates/` | `MIT OR Apache-2.0` |
| Everything else (`justfile`, `scripts/`, `.github/`, ...) | `MIT OR Apache-2.0` |

Unless you explicitly state otherwise, any contribution you intentionally
submit for inclusion in the `MIT OR Apache-2.0` parts, as defined in the
Apache-2.0 license, shall be dual licensed as above, without any additional
terms or conditions.

New source files start with an `SPDX-License-Identifier` header matching
their directory.
