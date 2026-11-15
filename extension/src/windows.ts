// Observes Meta windows and executes primitive window commands.
// No layout logic lives here.

import Meta from 'gi://Meta';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import type {WireMonitor, WireWindow} from './daemon.js';

export interface WindowEvents {
    windowOpened(window: WireWindow): void;
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
        win.get_workspace() !== null;
}

export function toWire(win: Meta.Window): WireWindow {
    const r = win.get_frame_rect();
    const ws = win.get_workspace()?.index() ?? 0;
    return [win.get_id(), ws, win.get_monitor(), r.x, r.y, r.width, r.height];
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
            display.connect('workareas-changed', () => this._events.monitorsChanged()));
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

    /** Returns an error reason, or null on success. */
    moveResize(id: number, x: number, y: number, w: number, h: number): string | null {
        const win = this.find(id);
        if (!win)
            return 'no-such-window';
        if (win.is_fullscreen())
            win.unmake_fullscreen();
        if (win.is_maximized())
            win.unmaximize();
        win.move_resize_frame(false, x, y, w, h);
        return null;
    }

    focus(id: number): string | null {
        const win = this.find(id);
        if (!win)
            return 'no-such-window';
        win.activate(global.get_current_time());
        return null;
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
        };

        tracked.signalIds.push(
            // `unmanaged` exists on Meta.Window in mutter 18 (verified against
            // the typelib) but is missing from @girs/meta-18's signal map.
            (win as unknown as {connect(s: string, cb: () => void): number})
                .connect('unmanaged', () => this._untrack(id)));

        if (existing) {
            // Already mapped windows are part of the initial snapshot.
            tracked.announced = isManageable(win);
        } else {
            // Wait until the window is shown so its type and geometry are final.
            tracked.signalIds.push(win.connect('shown', announce));
        }
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
