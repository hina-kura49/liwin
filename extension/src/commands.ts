// SPDX-License-Identifier: GPL-2.0-or-later
// Executes the primitive window commands the daemon sends. Each returns an
// error reason for RequestCompleted, or null on success. No layout logic
// lives here.

import type Meta from 'gi://Meta';

import type {PlacementAnimator} from './placementAnimator.js';
import type {Placement} from './protocol.js';

export class WindowCommands {
    private readonly _find: (id: number) => Meta.Window | null;
    private readonly _animator: PlacementAnimator | null;

    /**
     * `find` resolves a window id among the windows reported to the daemon;
     * `animator`, if given, animates moves (visual only).
     */
    constructor(find: (id: number) => Meta.Window | null, animator: PlacementAnimator | null = null) {
        this._find = find;
        this._animator = animator;
    }

    /** Returns an error reason, or null on success. */
    moveResize(id: number, x: number, y: number, w: number, h: number): string | null {
        const win = this._find(id);
        if (!win)
            return 'no-such-window';
        const wasConstrained = win.is_fullscreen() || win.is_maximized();
        if (win.is_fullscreen())
            win.unmake_fullscreen();
        if (win.is_maximized())
            win.unmaximize();
        this._animator?.beforeMove(win, x, y, wasConstrained);
        win.move_resize_frame(false, x, y, w, h);
        return null;
    }

    applyLayout(placements: Placement[]): void {
        for (const [id, x, y, w, h] of placements) {
            const err = this.moveResize(id, x, y, w, h);
            if (err)
                console.warn(`liwin: cannot place window ${id}: ${err}`);
        }
    }

    activateWorkspace(index: number): string | null {
        const ws = global.workspace_manager.get_workspace_by_index(index);
        if (!ws)
            return 'no-such-workspace';
        ws.activate(global.get_current_time());
        return null;
    }

    moveToWorkspace(id: number, index: number): string | null {
        const win = this._find(id);
        if (!win)
            return 'no-such-window';
        if (index >= global.workspace_manager.get_n_workspaces())
            return 'no-such-workspace';
        win.change_workspace_by_index(index, false);
        return null;
    }

    focus(id: number): string | null {
        const win = this._find(id);
        if (!win)
            return 'no-such-window';
        win.activate(global.get_current_time());
        return null;
    }
}
