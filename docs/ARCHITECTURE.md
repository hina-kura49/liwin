# liwin architecture

liwin brings an AeroSpace-like (i3-style) tiling workflow to GNOME on Wayland,
running *on top of* the stock GNOME Shell instead of replacing it.

Target: GNOME Shell 50, Wayland only (X11 is out of scope).

## Why a Shell extension at all

On Wayland, regular clients cannot enumerate, move, resize or focus other
clients' windows, and cannot grab global shortcuts. Only the compositor
(GNOME Shell / mutter) can. liwin therefore puts a **thin** GNOME Shell
extension inside the compositor and keeps all logic in a Rust daemon.

```
 ┌────────────┐  RunCommand (method)   ┌──────────────────────────┐
 │ liwin-cli  │ ─────────────────────▶ │ liwin-daemon (Rust)      │
 └────────────┘                        │  owns io.github.liwin.   │
                                       │       Daemon             │
 ┌───────────────────────────────┐     │  ┌────────────────────┐  │
 │ GNOME Shell (gnome-shell)     │     │  │ liwin-core         │  │
 │  ┌─────────────────────────┐  │ ──▶ │  │ tree + layout      │  │
 │  │ liwin@kura extension    │  │ methods (events, async)     │  │
 │  │ (TypeScript → GJS ESM)  │  │ ◀── │  └────────────────────┘  │
 │  └─────────────────────────┘  │ signals (commands)          │
 └───────────────────────────────┘     └──────────────────────────┘
                     session bus (D-Bus)
```

## Components

### 1. GNOME Shell extension (`extension/`, UUID `liwin@kura`)

Responsibilities — and nothing more:

- Observe windows: creation, destruction (`unmanaged`), focus changes, and the
  current set of windows and monitor work areas. Report them to the daemon.
- Execute primitive commands the daemon sends: move/resize a window, focus a
  window.
- Register global keybindings (only the compositor can on Wayland) and report
  which binding fired. The extension never sees the command string bound to a
  key — only an opaque numeric binding id.

It does **no layout computation** and holds no tree.

Rules for the extension:

- **It exports nothing on D-Bus and owns no bus name.** Window operations can
  therefore not be invoked by arbitrary session-bus clients.
- **Every D-Bus call it makes is asynchronous** (`Gio.DBusConnection.call` with
  a callback, never `call_sync` or a synchronous proxy), so a slow or hung
  daemon can never freeze the compositor.
- It only accepts signals whose sender is the owner of
  `io.github.liwin.Daemon` (match rule on the well-known sender name). The
  daemon sends command signals unicast to the Shell's unique name, so other
  session-bus clients do not receive them.
- It sends the **minimum** information: window ids, workspace/monitor indices
  and geometry. No titles, no app ids, no WM classes (signals and method
  payloads may be observable by other session-bus clients, e.g. via
  monitoring).
- On `disable()` it must undo everything: ungrab every accelerator
  (`global.display.ungrab_accelerator`), disconnect every signal handler,
  unsubscribe D-Bus signals, unwatch the bus name, and cancel in-flight calls.

#### Keybindings

`Main.wm.addKeybinding()` requires a GSettings schema key per binding, which
cannot express user-defined bindings from a TOML file. liwin instead uses the
same mechanism GNOME Shell's own `GrabAccelerator` D-Bus API uses
(`js/ui/shellDBus.js`):

1. Conflict check against GNOME's own keybinding settings
   (`org.gnome.desktop.wm.keybindings`, `org.gnome.shell.keybindings`,
   `org.gnome.mutter.keybindings`, `org.gnome.mutter.wayland.keybindings`,
   `org.gnome.settings-daemon.plugins.media-keys`), comparing normalized
   accelerators. A taken accelerator is **not stolen**: the binding is
   skipped, logged, and reported back to the daemon as rejected (see
   `KeybindingsApplied` in `dbus.md`); the daemon logs a warning naming the
   key. This check is required: on GNOME Shell 50.1, `grab_accelerator()`
   accepts accelerators that Shell keybindings already use (observed with
   `<Super>a` and with `toggle-application-view` rebound to `<Alt>y`), even
   though mutter's source suggests it should refuse — and when the key is
   pressed, the external grab wins (verified in the devkit window: Alt+Y ran
   the liwin command and the app grid did not open). Without this check
   liwin would silently take GNOME's shortcuts. Custom shortcuts of
   gnome-settings-daemon are not inspected yet.
2. `action = global.display.grab_accelerator(accel, Meta.KeyBindingFlags.NONE)`;
   `Meta.KeyBindingAction.NONE` (e.g. another external grabber has it) is
   treated as a rejection too.
3. Otherwise `Main.wm.allowKeybinding(Meta.external_binding_name_for_action(action), Shell.ActionMode.NORMAL)`
   so it fires in normal mode only (not in the overview, lock screen, or
   modal dialogs).
4. `global.display` `accelerator-activated` → look up our action → call
   `KeybindingTriggered(binding_id)` on the daemon.

When bindings change, and on `disable()`, every grabbed action is released
with `global.display.ungrab_accelerator(action)` and removed from the
allowed-keybinding map.

### 2. Daemon (`crates/liwin-daemon`, Rust, zbus)

- Owns the well-known name `io.github.liwin.Daemon` on the session bus.
- Holds the authoritative model (`liwin-core`): one tree per (GNOME
  workspace, monitor) of horizontal/vertical split containers with windows
  as leaves. Floating windows (transient or fixed-size) are tracked but not
  tiled. liwin uses GNOME's own workspaces rather than emulating them.
- Translates high-level commands (`focus left`, keybinding presses) into
  primitive operations, emitted as signals for the extension.
- Loads the TOML configuration and sends the keybinding table
  (`binding_id → accelerator`) to the extension.
- Authenticates the extension: methods of the `ShellBridge1` interface are
  only accepted from the unique bus name that currently owns
  `org.gnome.Shell.Introspect` — a name GNOME Shell itself owns on the same
  connection (`Gio.DBus.session`) that extensions use. Any other caller gets
  `io.github.liwin.Error.AccessDenied`.
- Tiles automatically: whenever a window opens, closes, changes workspace or
  monitor, or a layout command runs, the affected trees are laid out into the
  monitor's work area (minus outer gaps, with inner gaps) and sent to the
  extension as one `ApplyLayout` batch.

### 3. CLI (`crates/liwin-cli`)

A thin client: it forwards its argv to `Daemon1.RunCommand` and prints the
reply, e.g. `liwin focus left`, `liwin list-windows`. All parsing and
validation happens in the daemon so the CLI and keybindings share one command
language (as in AeroSpace).

### 4. Core library (`crates/liwin-core`)

Pure Rust, no D-Bus, no GNOME. Contains:

- `Tree`: an arena of nodes. A node is either a `Container { orientation,
  children }` or a `Window { id }`. The root of a workspace is always a
  container.
- Operations: add window (next to the focused one), remove window (with
  normalization: empty containers are removed, single-child non-root
  containers are flattened), split the focused window into a new
  horizontal/vertical container, and directional focus (left/right/up/down).
- Directional focus follows AeroSpace semantics: walk up from the focused
  window to the nearest ancestor whose orientation matches the direction and
  that has a sibling in that direction, then descend into that sibling.
- `layout(rect)`: computes each window's rectangle (equal split for now).

Everything is unit-tested.

## Configuration

`~/.config/liwin/liwin.toml` (respecting `$XDG_CONFIG_HOME`). Key names follow
AeroSpace's `~/.aerospace.toml` so a subset of an AeroSpace config can be read
later:

```toml
config-version = 2
default-root-container-layout = 'tiles'
default-root-container-orientation = 'auto'

[gaps]
inner.horizontal = 0
inner.vertical = 0
outer.left = 0
outer.bottom = 0
outer.top = 0
outer.right = 0

[mode.main.binding]
alt-h = 'focus left'
alt-j = 'focus down'
alt-k = 'focus up'
alt-l = 'focus right'
```

Unknown keys are reported as warnings, not errors, so an AeroSpace file with
macOS-only options still loads. AeroSpace key notation (`alt-shift-h`) is
converted to GTK accelerator syntax (`<Alt><Shift>h`) in the daemon.

## Startup order and resilience

Neither side requires the other to be running first:

- The extension watches `io.github.liwin.Daemon` (`watch_name`,
  asynchronous). When the name appears it calls `Hello`, then sends a full
  snapshot (`SyncMonitors`, `SyncWindows`). The daemon answers the first
  snapshot after each `Hello` with `KeybindingsChanged`. When the daemon
  vanishes, the extension drops the keybinding grabs it made on its behalf.
- On `disable()` the extension calls `Goodbye` (GNOME Shell's bus connection
  outlives the extension, so the daemon could not notice otherwise). If
  gnome-shell itself exits, the daemon notices via `NameOwnerChanged`.
- The daemon emits `ResyncRequested` at startup; an already-running extension
  responds with a full snapshot.
- Primitive commands carry a `request_id`; the extension reports completion
  with `RequestCompleted`. The daemon never blocks waiting for the extension
  beyond a short timeout.

## Development and testing

- `cargo test` — core unit tests and daemon tests against a private
  `dbus-daemon`.
- Nested GNOME Shell (GNOME 49+; needs `mutter-dev-bin` on Ubuntu):
  `dbus-run-session gnome-shell --devkit --wayland`.
- Headless E2E: `dbus-run-session gnome-shell --headless --virtual-monitor WxH`.
- Both nested modes run with `XDG_CONFIG_HOME`/`XDG_DATA_HOME` pointing into
  `.sandbox/`, so enabling the extension in the nested Shell does not touch the
  logged-in session's dconf (`enabled-extensions`) or extension directory.
- The nested Shell runs without an input method: `ibus-daemon` is shadowed by
  a no-op in `.sandbox/bin` and input sources are set to `xkb us`. A nested
  IBus shares the user's IME backend and broke conversion in practice.
- The headless Shell also gets its own `XDG_RUNTIME_DIR` (`.sandbox/run`):
  GNOME Shell keeps a crash sentinel (`gnome-shell-disable-extensions`) there
  for its first 60 s, and the logged-in session's systemd unit would disable
  the user's extensions if *that* Shell crashed meanwhile. The devkit window
  needs the real runtime dir to reach the real compositor, so devkit runs
  still share it.
- Super-based shortcuts cannot be tested in the devkit window: the logged-in
  GNOME Shell handles them before the nested Shell sees them.

See `docs/dbus.md` for the wire protocol.
