// SPDX-License-Identifier: GPL-2.0-or-later
// Which windows are reported as floating (never tiled).
//
// Pure module: works on plain booleans read from a Meta.Window by
// windows.ts, so it can be unit tested with node.

import {FLAG_FLOATING} from './protocol.js';

/** The window properties the floating decision depends on. */
export interface WindowState {
    /** get_transient_for() !== null */
    transient: boolean;
    /** is_maximized() */
    maximized: boolean;
    /** is_fullscreen() */
    fullscreen: boolean;
    /** allows_resize() */
    allowsResize: boolean;
}

/**
 * Transient windows (dialogs) and fixed-size windows float.
 *
 * allows_resize() is false while a window is maximized or fullscreen (seen
 * with GNOME Shell 50: a maximized gnome-text-editor reported false), so it
 * only says "fixed size" when the window is in neither state. Tiling
 * unmaximizes windows anyway.
 */
export function isFloating(s: WindowState): boolean {
    if (s.transient)
        return true;
    const constrained = s.maximized || s.fullscreen;
    return !constrained && !s.allowsResize;
}

/** Wire flags of a window in state `s`. */
export function windowFlags(s: WindowState): number {
    return isFloating(s) ? FLAG_FLOATING : 0;
}
