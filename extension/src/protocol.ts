// SPDX-License-Identifier: GPL-2.0-or-later
// Constants and wire types of io.github.liwin.ShellBridge1 (docs/dbus.md).
//
// Pure module: imports nothing from gi:// or resource://, so it can be unit
// tested with node.

export const BUS_NAME = 'io.github.liwin.Daemon';
export const OBJECT_PATH = '/io/github/liwin/Daemon';
export const BRIDGE_INTERFACE = 'io.github.liwin.ShellBridge1';
export const PROTOCOL_VERSION = 2;

/** Workspace index meaning "on all workspaces". */
export const ALL_WORKSPACES = 0xffffffff;
/** Window flag: do not tile (not resizable, transient, ...). */
export const FLAG_FLOATING = 1;

/** `(id, workspace, monitor, x, y, width, height, flags)` */
export type WireWindow = [number, number, number, number, number, number, number, number];
/** `(id, x, y, width, height)` */
export type Placement = [number, number, number, number, number];
/** `(index, x, y, width, height)` of the monitor's work area */
export type WireMonitor = [number, number, number, number, number];

/** GVariant type strings of the methods the extension calls (in-args only). */
export const METHOD_SIGNATURES = {
    Hello: '(u)',
    Goodbye: '()',
    SyncMonitors: '(a(iiiii))',
    SyncWindows: '(ta(tuiiiiiu))',
    WindowOpened: '((tuiiiiiu))',
    WindowChanged: '((tuiiiiiu))',
    WindowClosed: '(t)',
    FocusChanged: '(t)',
    KeybindingTriggered: '(u)',
    KeybindingsApplied: '(au)',
    RequestCompleted: '(ubs)',
} as const;

/** GVariant type strings of the signals the daemon emits. */
export const SIGNAL_SIGNATURES = {
    ApplyLayout: '(a(tiiii))',
    ActivateWorkspace: '(uu)',
    MoveWindowToWorkspace: '(utu)',
    MoveResizeWindow: '(utiiii)',
    FocusWindow: '(ut)',
    KeybindingsChanged: '(a(us))',
    ResyncRequested: '()',
} as const;

export type MethodName = keyof typeof METHOD_SIGNATURES;
export type SignalName = keyof typeof SIGNAL_SIGNATURES;
