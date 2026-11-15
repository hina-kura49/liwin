// SPDX-License-Identifier: GPL-2.0-or-later
// Client side of io.github.liwin.ShellBridge1 (see docs/dbus.md).
//
// Rules: this module exports nothing on D-Bus and never blocks. Every call is
// asynchronous and cancelled on destroy(). Signals are only accepted from the
// current owner of io.github.liwin.Daemon.

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

export const BUS_NAME = 'io.github.liwin.Daemon';
export const OBJECT_PATH = '/io/github/liwin/Daemon';
export const BRIDGE_INTERFACE = 'io.github.liwin.ShellBridge1';
export const PROTOCOL_VERSION = 2;
const CALL_TIMEOUT_MS = 1000;

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

export interface DaemonHandlers {
    /** The daemon appeared and accepted Hello: send a full snapshot. */
    onConnected(): void;
    /** The daemon vanished. */
    onDisconnected(): void;
    onResyncRequested(): void;
    onMoveResizeWindow(requestId: number, windowId: number, x: number, y: number, w: number, h: number): void;
    onFocusWindow(requestId: number, windowId: number): void;
    onKeybindingsChanged(bindings: [number, string][]): void;
    onApplyLayout(placements: Placement[]): void;
    onActivateWorkspace(requestId: number, workspace: number): void;
    onMoveWindowToWorkspace(requestId: number, windowId: number, workspace: number): void;
}

export class DaemonClient {
    private readonly _conn: Gio.DBusConnection;
    private readonly _handlers: DaemonHandlers;
    private readonly _cancellable = new Gio.Cancellable();
    private readonly _watchId: number;
    private readonly _subscriptions: number[] = [];
    private _connected = false;

    constructor(handlers: DaemonHandlers) {
        this._handlers = handlers;
        // The connection GNOME Shell itself uses. The daemon authenticates us
        // by comparing our unique name with the owner of
        // org.gnome.Shell.Introspect, which Shell owns on this connection.
        this._conn = Gio.DBus.session;

        const subscribe = (member: string, cb: (params: GLib.Variant) => void) => {
            this._subscriptions.push(this._conn.signal_subscribe(
                BUS_NAME, BRIDGE_INTERFACE, member, OBJECT_PATH, null,
                Gio.DBusSignalFlags.NONE,
                (_conn, _sender, _path, _iface, _member, params) => {
                    try {
                        cb(params);
                    } catch (e) {
                        console.error(`liwin: error handling ${member}:`, e);
                    }
                }));
        };
        subscribe('ResyncRequested', () => {
            if (this._connected)
                this._handlers.onResyncRequested();
            else
                this._hello();
        });
        subscribe('MoveResizeWindow', params => {
            if (!this._connected)
                return;
            const [req, id, x, y, w, h] = params.deepUnpack() as [number, number, number, number, number, number];
            this._handlers.onMoveResizeWindow(req, id, x, y, w, h);
        });
        subscribe('FocusWindow', params => {
            if (!this._connected)
                return;
            const [req, id] = params.deepUnpack() as [number, number];
            this._handlers.onFocusWindow(req, id);
        });
        subscribe('ApplyLayout', params => {
            if (!this._connected)
                return;
            const [placements] = params.deepUnpack() as [Placement[]];
            this._handlers.onApplyLayout(placements);
        });
        subscribe('ActivateWorkspace', params => {
            if (!this._connected)
                return;
            const [req, ws] = params.deepUnpack() as [number, number];
            this._handlers.onActivateWorkspace(req, ws);
        });
        subscribe('MoveWindowToWorkspace', params => {
            if (!this._connected)
                return;
            const [req, id, ws] = params.deepUnpack() as [number, number, number];
            this._handlers.onMoveWindowToWorkspace(req, id, ws);
        });
        subscribe('KeybindingsChanged', params => {
            if (!this._connected)
                return;
            const [bindings] = params.deepUnpack() as [[number, string][]];
            this._handlers.onKeybindingsChanged(bindings);
        });

        // GJS override: DBusConnection.watch_name(name, flags, appeared, vanished).
        this._watchId = this._conn.watch_name(BUS_NAME, Gio.BusNameWatcherFlags.NONE,
            () => this._hello(),
            () => this._setDisconnected());
    }

    get connected(): boolean {
        return this._connected;
    }

    destroy(): void {
        // Tell the daemon we are going away (the Shell's bus connection
        // outlives the extension). Fire-and-forget, not cancellable.
        if (this._connected) {
            this._conn.call(BUS_NAME, OBJECT_PATH, BRIDGE_INTERFACE, 'Goodbye', null, null,
                Gio.DBusCallFlags.NO_AUTO_START, CALL_TIMEOUT_MS, null, null);
        }
        this._cancellable.cancel();
        this._conn.unwatch_name(this._watchId);
        for (const id of this._subscriptions)
            this._conn.signal_unsubscribe(id);
        this._subscriptions.length = 0;
        this._connected = false;
    }

    syncMonitors(monitors: WireMonitor[]): void {
        this._call('SyncMonitors', new GLib.Variant('(a(iiiii))', [monitors]));
    }

    syncWindows(focused: number, windows: WireWindow[]): void {
        this._call('SyncWindows', new GLib.Variant('(ta(tuiiiiiu))', [focused, windows]));
    }

    windowOpened(window: WireWindow): void {
        this._call('WindowOpened', new GLib.Variant('((tuiiiiiu))', [window]));
    }

    windowChanged(window: WireWindow): void {
        this._call('WindowChanged', new GLib.Variant('((tuiiiiiu))', [window]));
    }

    windowClosed(id: number): void {
        this._call('WindowClosed', new GLib.Variant('(t)', [id]));
    }

    focusChanged(id: number): void {
        this._call('FocusChanged', new GLib.Variant('(t)', [id]));
    }

    keybindingTriggered(bindingId: number): void {
        this._call('KeybindingTriggered', new GLib.Variant('(u)', [bindingId]));
    }

    keybindingsApplied(rejected: number[]): void {
        this._call('KeybindingsApplied', new GLib.Variant('(au)', [rejected]));
    }

    requestCompleted(requestId: number, ok: boolean, error: string): void {
        this._call('RequestCompleted', new GLib.Variant('(ubs)', [requestId, ok, error]));
    }

    private _hello(): void {
        this._callRaw('Hello', new GLib.Variant('(u)', [PROTOCOL_VERSION]), reply => {
            const [version] = reply.deepUnpack() as [number];
            if (version !== PROTOCOL_VERSION) {
                console.error(`liwin: daemon speaks protocol ${version}, extension speaks ${PROTOCOL_VERSION}; staying idle`);
                return;
            }
            this._connected = true;
            console.log('liwin: connected to daemon');
            this._handlers.onConnected();
        });
    }

    private _setDisconnected(): void {
        if (!this._connected)
            return;
        this._connected = false;
        console.log('liwin: daemon vanished');
        this._handlers.onDisconnected();
    }

    /** Fire-and-forget call; only sent while connected. */
    private _call(method: string, params: GLib.Variant): void {
        if (this._connected)
            this._callRaw(method, params, () => {});
    }

    private _callRaw(method: string, params: GLib.Variant, onReply: (reply: GLib.Variant) => void): void {
        this._conn.call(BUS_NAME, OBJECT_PATH, BRIDGE_INTERFACE, method, params, null,
            Gio.DBusCallFlags.NO_AUTO_START, CALL_TIMEOUT_MS, this._cancellable,
            (conn, res) => {
                let reply: GLib.Variant;
                try {
                    reply = conn!.call_finish(res);
                } catch (e) {
                    if (e instanceof GLib.Error && e.matches(Gio.IOErrorEnum, Gio.IOErrorEnum.CANCELLED))
                        return;
                    console.warn(`liwin: ${method} failed: ${e}`);
                    return;
                }
                try {
                    onReply(reply);
                } catch (e) {
                    console.error(`liwin: error handling ${method} reply:`, e);
                }
            });
    }
}
