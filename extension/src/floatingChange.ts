// SPDX-License-Identifier: GPL-2.0-or-later
// PREPARED, NOT WIRED: detecting maximize / fullscreen changes that should
// change a window's floating flag.
//
// Today a maximized or fullscreen window is tiled like any other (and the
// next ApplyLayout unmaximizes it). The intended behavior is that a window
// floats while the user keeps it maximized or fullscreen, and is tiled again
// when it is restored. Reporting that needs the extension to send
// WindowChanged for a flag change, which docs/dbus.md does not cover
// (WindowChanged means "moved to another workspace or monitor"), so nothing
// here is connected to D-Bus yet.
//
// To wire it, windows.ts would connect, per tracked window,
// `notify::maximized-horizontally`, `notify::maximized-vertically` and
// `notify::fullscreen` (properties of Meta.Window in mutter 18), call
// FloatingChangeDetector.update() with windowState(win) and send
// WindowChanged with the new flags when it returns non-null.
//
// Pure module: imports nothing from gi:// or resource://, so it can be unit
// tested with node.

import {isFloating} from './floating.js';
import type {WindowState} from './floating.js';

/** Proposed policy: today's rule, plus "maximized or fullscreen floats". */
export function isFloatingWhileConstrained(s: WindowState): boolean {
    return isFloating(s) || s.maximized || s.fullscreen;
}

/** Remembers the floating flag last reported per window and spots changes. */
export class FloatingChangeDetector {
    private readonly _policy: (s: WindowState) => boolean;
    private readonly _reported = new Map<number, boolean>();

    constructor(policy: (s: WindowState) => boolean = isFloatingWhileConstrained) {
        this._policy = policy;
    }

    /** Records the state a window was reported with (WindowOpened, SyncWindows). */
    reported(id: number, state: WindowState): void {
        this._reported.set(id, this._policy(state));
    }

    /**
     * The new floating flag if it differs from the last reported one (and
     * records it as reported), or null if nothing changed or the window was
     * never reported.
     */
    update(id: number, state: WindowState): boolean | null {
        const before = this._reported.get(id);
        if (before === undefined)
            return null;
        const now = this._policy(state);
        if (now === before)
            return null;
        this._reported.set(id, now);
        return now;
    }

    forget(id: number): void {
        this._reported.delete(id);
    }
}
