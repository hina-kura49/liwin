// SPDX-License-Identifier: GPL-2.0-or-later
// Observes Meta windows and reports them (see commands.ts for the primitive
// commands the daemon sends). No layout logic lives here.

import Meta from 'gi://Meta';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import {windowFlags} from './floating.js';
import type {WindowState} from './floating.js';
import {ALL_WORKSPACES} from './protocol.js';
import type {WireMonitor, WireWindow} from './protocol.js';

export interface WindowEvents {
    windowOpened(window: WireWindow): void;
    /** Workspace, monitor or floating state of a reported window changed. */
    windowChanged(window: WireWindow): void;
    windowClosed(id: number): void;
    focusChanged(id: number): void;
    monitorsChanged(): void;
}

interface Tracked {
    window: Meta.Window;
    signalIds: number[];
    announced: boolean;
}

/** Windows liwin manages: ordinary application windows only. */
function isManageable(win: Meta.Window): boolean {
    return win.get_window_type() === Meta.WindowType.NORMAL &&
        !win.is_skip_taskbar() &&
        (win.is_on_all_workspaces() || win.get_workspace() !== null);
}

/** The inputs of the floating decision (see floating.ts). */
function windowState(win: Meta.Window): WindowState {
    return {
        transient: win.get_transient_for() !== null,
        maximized: win.is_maximized(),
        fullscreen: win.is_fullscreen(),
        allowsResize: win.allows_resize(),
    };
}

export function toWire(win: Meta.Window): WireWindow {
    const r = win.get_frame_rect();
    const ws = win.is_on_all_workspaces() ? ALL_WORKSPACES : win.get_workspace()?.index() ?? 0;
    return [win.get_id(), ws, win.get_monitor(), r.x, r.y, r.width, r.height, windowFlags(windowState(win))];
}

export class WindowTracker {
    private readonly _events: WindowEvents;
    private readonly _tracked = new Map<number, Tracked>();
    private readonly _displaySignals: number[] = [];
    private readonly _monitorSignals: number[] = [];

    constructor(events: WindowEvents) {
        this._events = events;
        const display = global.display;
        this._displaySignals.push(
            display.connect('window-created', (_d, win) => this._track(win, false)),
            display.connect('notify::focus-window', () => this._events.focusChanged(this.focusedId())),
            display.connect('workareas-changed', () => this._events.monitorsChanged()),
            display.connect('window-entered-monitor', (_d, _monitor, win) => this._changed(win)));
        this._monitorSignals.push(
            Main.layoutManager.connect('monitors-changed', () => this._events.monitorsChanged()));

        for (const actor of global.get_window_actors())
            this._track(actor.get_meta_window()!, true);
    }

    destroy(): void {
        for (const id of this._displaySignals)
            global.display.disconnect(id);
        for (const id of this._monitorSignals)
            Main.layoutManager.disconnect(id);
        this._displaySignals.length = 0;
        this._monitorSignals.length = 0;
        for (const t of this._tracked.values())
            for (const id of t.signalIds)
                t.window.disconnect(id);
        this._tracked.clear();
    }

    /** Snapshot of all managed windows. */
    windows(): WireWindow[] {
        return [...this._tracked.values()]
            .filter(t => t.announced && isManageable(t.window))
            .map(t => toWire(t.window));
    }

    monitors(): WireMonitor[] {
        const ws = global.workspace_manager.get_active_workspace();
        const out: WireMonitor[] = [];
        for (let i = 0; i < global.display.get_n_monitors(); i++) {
            const r = ws.get_work_area_for_monitor(i);
            out.push([i, r.x, r.y, r.width, r.height]);
        }
        return out;
    }

    focusedId(): number {
        const win = global.display.focus_window;
        return win && this._tracked.get(win.get_id())?.announced && isManageable(win) ? win.get_id() : 0;
    }

    find(id: number): Meta.Window | null {
        return this._tracked.get(id)?.window ?? null;
    }

    private _track(win: Meta.Window, existing: boolean): void {
        const id = win.get_id();
        if (this._tracked.has(id))
            return;
        const tracked: Tracked = {window: win, signalIds: [], announced: false};
        this._tracked.set(id, tracked);

        const announce = () => {
            if (tracked.announced || !isManageable(win))
                return;
            tracked.announced = true;
            this._events.windowOpened(toWire(win));
            // Focus usually moves to a new window before it is shown, when it
            // was not reported yet; report it again now that it is.
            if (global.display.focus_window === win)
                this._events.focusChanged(id);
        };

        tracked.signalIds.push(
            // `unmanaged` exists on Meta.Window in mutter 18 (verified against
            // the typelib) but is missing from @girs/meta-18's signal map.
            (win as unknown as {connect(s: string, cb: () => void): number})
                .connect('unmanaged', () => this._untrack(id)));

        tracked.signalIds.push(win.connect('workspace-changed', () => this._changed(win)));

        if (existing) {
            // Already mapped windows are part of the initial snapshot.
            tracked.announced = isManageable(win);
        } else {
            // Wait until the window is shown so its type and geometry are final.
            tracked.signalIds.push(win.connect('shown', announce));
        }
    }

    private _changed(win: Meta.Window): void {
        const tracked = this._tracked.get(win.get_id());
        if (tracked?.announced && isManageable(win))
            this._events.windowChanged(toWire(win));
    }

    private _untrack(id: number): void {
        const tracked = this._tracked.get(id);
        if (!tracked)
            return;
        for (const sid of tracked.signalIds)
            tracked.window.disconnect(sid);
        this._tracked.delete(id);
        if (tracked.announced)
            this._events.windowClosed(id);
    }
}
