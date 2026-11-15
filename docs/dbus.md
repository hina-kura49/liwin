# liwin D-Bus protocol

Bus: the user's **session bus**.

Only the daemon owns a bus name and exports objects. The GNOME Shell
extension exports nothing.

| Item        | Value                         |
|-------------|-------------------------------|
| Bus name    | `io.github.liwin.Daemon`      |
| Object path | `/io/github/liwin/Daemon`     |
| Interfaces  | `io.github.liwin.Daemon1` (CLI / scripting), `io.github.liwin.ShellBridge1` (extension) |

Protocol version: **1** (exchanged in `ShellBridge1.Hello`).

## Common types

| Name        | Signature | Meaning |
|-------------|-----------|---------|
| `WindowId`  | `t`       | `Meta.Window.get_id()` (stable for the window's lifetime, unique per Shell instance). `0` means "no window". |
| `Window`    | `(tuiiiii)` | `(id, workspace_index, monitor_index, x, y, width, height)`; geometry is the frame rect in logical pixels. |
| `Monitor`   | `(iiiii)` | `(monitor_index, work_area_x, work_area_y, work_area_width, work_area_height)` |
| `RequestId` | `u`       | Daemon-chosen id correlating a command signal with `RequestCompleted`. |
| `BindingId` | `u`       | Opaque daemon-chosen id for a keybinding. |

Window titles, application ids and WM classes are **never** sent.

## `io.github.liwin.Daemon1` — public, for the CLI

Callable by any session-bus client of the same user.

### Methods

- `RunCommand(in as args, out s output)`
  Executes one command in AeroSpace-like syntax, e.g. `["focus", "left"]`,
  `["list-windows"]`. Returns human-readable output (may be empty).
  Errors: `io.github.liwin.Error.InvalidCommand`,
  `io.github.liwin.Error.ShellNotConnected`, `io.github.liwin.Error.Timeout`,
  `io.github.liwin.Error.Failed`.

  Debug commands (`debug move-window <id> <x> <y> <w> <h>`) are only accepted
  when the daemon was started with `--enable-debug-commands`; otherwise they
  fail with `InvalidCommand`.

### Properties

- `Version` (`s`, read-only): daemon version.
- `ShellConnected` (`b`, read-only): whether an extension has said `Hello`.

## `io.github.liwin.ShellBridge1` — for the extension only

**Methods** are accepted only from the unique bus name that currently owns
`org.gnome.Shell.Introspect` (i.e. the gnome-shell process, whose extensions
share its `Gio.DBus.session` connection). Other callers receive
`io.github.liwin.Error.AccessDenied`.

**Signals** are emitted by the daemon and, except for `ResyncRequested`,
are **unicast**: their destination is the authenticated Shell's unique name,
so other peers cannot receive them through match rules. The extension
subscribes with `sender = "io.github.liwin.Daemon"`, so the bus only delivers
signals that really come from the current owner of that name.
`ResyncRequested` is broadcast once at daemon startup, when no Shell peer is
known yet; it carries no data.

The extension must call every method asynchronously.

### Methods (extension → daemon)

- `Hello(in u protocol_version, out u daemon_protocol_version)`
  First call after the daemon's name appears. On a version mismatch the
  extension logs an error and stays idle.
- `Goodbye()`
  Sent from `disable()`. GNOME Shell's bus connection outlives the
  extension, so the daemon cannot detect this on its own. The daemon forgets
  the peer (and its model) until the next `Hello`.
- `SyncMonitors(in a(iiiii) monitors)`
  Full replacement of the monitor/work-area list.
- `SyncWindows(in t focused, in a(tuiiiii) windows)`
  Full replacement of the managed window set (normal, non-skip-taskbar
  windows only). `focused` may be `0`.
- `WindowOpened(in (tuiiiii) window)`
- `WindowClosed(in t window_id)`
- `FocusChanged(in t window_id)` — `0` when no managed window is focused.
- `KeybindingTriggered(in u binding_id)`
- `KeybindingsApplied(in au rejected)`
  Sent after processing `KeybindingsChanged`. `rejected` lists binding ids
  whose accelerator could not be grabbed (usually because GNOME already uses
  it). Rejected bindings are not stolen from their current owner.
- `RequestCompleted(in u request_id, in b ok, in s error)`
  `error` is a short machine-oriented reason (e.g. `"no-such-window"`), empty
  when `ok` is true.

### Signals (daemon → extension)

- `MoveResizeWindow(u request_id, t window_id, i x, i y, i width, i height)`
  Unmaximizes the window if needed and calls
  `Meta.Window.move_resize_frame(false, x, y, width, height)`.
- `FocusWindow(u request_id, t window_id)`
  Calls `Meta.Window.activate(global.get_current_time())`.
- `KeybindingsChanged(a(us) bindings)`
  Full replacement of `(binding_id, accelerator)` pairs. Accelerators use GTK
  syntax (`<Alt>h`). The extension ungrabs all previous accelerators, then
  grabs the new ones, then replies with `KeybindingsApplied`.
- `ResyncRequested()`
  The extension replies with `SyncMonitors` + `SyncWindows` (and the daemon
  re-sends `KeybindingsChanged` after the next `Hello`/sync).

## Sequences

Extension enabled while the daemon is running:

```
ext: bus_watch_name(io.github.liwin.Daemon) → appeared
ext → Hello(1)                → 1
ext → SyncMonitors([...])
ext → SyncWindows(focused, [...])
daemon ⇒ KeybindingsChanged([...])
ext → KeybindingsApplied([rejected ids])
```

Daemon started after the extension:

```
daemon: own io.github.liwin.Daemon
ext: name appeared → Hello → SyncMonitors → SyncWindows   (same as above)
```

CLI move (the end-to-end "hello world"):

```
cli → Daemon1.RunCommand(["debug","move-window","42","100","100","800","600"])
daemon ⇒ MoveResizeWindow(7, 42, 100, 100, 800, 600)
ext → RequestCompleted(7, true, "")
daemon → cli: "ok"
```

## Threat model notes

- The session bus is shared by every process of the user; it is not a strong
  security boundary. The goals here are narrower: no arbitrary client can call
  *primitive* window operations inside the compositor, no window titles leak
  onto the bus, and the Shell never blocks on D-Bus.
- A same-user process could still drive liwin through the public
  `Daemon1.RunCommand` interface, exactly like the CLI. That is by design
  (scriptability). Debug primitives are off unless explicitly enabled.
- A process that grabs `io.github.liwin.Daemon` before the real daemon could
  impersonate it. Its reach is limited to the primitives above (move/resize,
  focus, grab accelerators).
