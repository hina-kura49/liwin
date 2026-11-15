// SPDX-License-Identifier: GPL-2.0-or-later
// Client side of io.github.liwin.ShellBridge1 (see docs/dbus.md).
//
// Rules: this module exports nothing on D-Bus and never blocks. Every call is
// asynchronous and cancelled on destroy(). Signals are only accepted from the
// current owner of io.github.liwin.Daemon.

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

import {
    BRIDGE_INTERFACE, BUS_NAME, METHOD_SIGNATURES, OBJECT_PATH, PROTOCOL_VERSION, SIGNAL_SIGNATURES,
} from './protocol.js';
import type {MethodArgs, MethodName, Placement, SignalArgs, SignalName, WireMonitor, WireWindow} from './protocol.js';

const CALL_TIMEOUT_MS = 1000;

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

/** How to dispatch each daemon signal. */
type SignalTable = {
    [S in SignalName]: {
        /** Also handled before Hello succeeded. */
        whileDisconnected?: boolean;
        handle(...args: SignalArgs[S]): void;
    };
};

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

        const table = this._signalTable();
        for (const member of Object.keys(table) as SignalName[])
            this._subscribe(member, table[member]);

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
        this._call('SyncMonitors', monitors);
    }

    syncWindows(focused: number, windows: WireWindow[]): void {
        this._call('SyncWindows', focused, windows);
    }

    windowOpened(window: WireWindow): void {
        this._call('WindowOpened', window);
    }

    windowChanged(window: WireWindow): void {
        this._call('WindowChanged', window);
    }

    windowClosed(id: number): void {
        this._call('WindowClosed', id);
    }

    focusChanged(id: number): void {
        this._call('FocusChanged', id);
    }

    keybindingTriggered(bindingId: number): void {
        this._call('KeybindingTriggered', bindingId);
    }

    keybindingsApplied(rejected: number[]): void {
        this._call('KeybindingsApplied', rejected);
    }

    requestCompleted(requestId: number, ok: boolean, error: string): void {
        this._call('RequestCompleted', requestId, ok, error);
    }

    private _signalTable(): SignalTable {
        const h = this._handlers;
        return {
            ResyncRequested: {
                whileDisconnected: true,
                handle: () => {
                    if (this._connected)
                        h.onResyncRequested();
                    else
                        this._hello();
                },
            },
            MoveResizeWindow: {handle: (req, id, x, y, w, hh) => h.onMoveResizeWindow(req, id, x, y, w, hh)},
            FocusWindow: {handle: (req, id) => h.onFocusWindow(req, id)},
            ApplyLayout: {handle: placements => h.onApplyLayout(placements)},
            ActivateWorkspace: {handle: (req, ws) => h.onActivateWorkspace(req, ws)},
            MoveWindowToWorkspace: {handle: (req, id, ws) => h.onMoveWindowToWorkspace(req, id, ws)},
            KeybindingsChanged: {handle: bindings => h.onKeybindingsChanged(bindings)},
        };
    }

    private _subscribe<S extends SignalName>(member: S, entry: SignalTable[S]): void {
        const signature = SIGNAL_SIGNATURES[member];
        this._subscriptions.push(this._conn.signal_subscribe(
            BUS_NAME, BRIDGE_INTERFACE, member, OBJECT_PATH, null,
            Gio.DBusSignalFlags.NONE,
            (_conn, _sender, _path, _iface, _member, params) => {
                if (!this._connected && !entry.whileDisconnected)
                    return;
                const type = params?.get_type_string() ?? '()';
                if (type !== signature) {
                    console.warn(`liwin: ignoring ${member} with signature ${type}, expected ${signature}`);
                    return;
                }
                try {
                    const args = (params?.deepUnpack() ?? []) as SignalArgs[S];
                    (entry.handle as (...a: SignalArgs[S]) => void)(...args);
                } catch (e) {
                    console.error(`liwin: error handling ${member}:`, e);
                }
            }));
    }

    private _hello(): void {
        this._callRaw('Hello', [PROTOCOL_VERSION], reply => {
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
    private _call<M extends MethodName>(method: M, ...args: MethodArgs[M]): void {
        if (this._connected)
            this._callRaw(method, args, () => {});
    }

    private _callRaw<M extends MethodName>(method: M, args: MethodArgs[M], onReply: (reply: GLib.Variant) => void): void {
        // MethodArgs mirrors METHOD_SIGNATURES; the GVariant typing of
        // @girs cannot follow the generic index, hence the cast.
        const params = new GLib.Variant(METHOD_SIGNATURES[method] as string, args as any);
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
