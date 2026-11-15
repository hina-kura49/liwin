// SPDX-License-Identifier: GPL-2.0-or-later
// liwin GNOME Shell extension entry point.
//
// A thin layer: it reports windows to liwin-daemon, executes the daemon's
// primitive commands and forwards keybinding presses. All logic lives in the
// daemon. See docs/ARCHITECTURE.md.

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

import {WindowCommands} from './commands.js';
import {DaemonClient} from './daemon.js';
import {WorkspaceIndicator} from './indicator.js';
import {KeybindingManager} from './keybindings.js';
import {WindowTracker} from './windows.js';

class Liwin {
    private readonly _daemon: DaemonClient;
    private readonly _windows: WindowTracker;
    private readonly _commands: WindowCommands;
    private readonly _keys: KeybindingManager;
    private readonly _indicator: WorkspaceIndicator;

    constructor() {
        this._indicator = new WorkspaceIndicator();
        this._keys = new KeybindingManager(id => this._daemon.keybindingTriggered(id));
        this._windows = new WindowTracker({
            windowOpened: w => this._daemon.windowOpened(w),
            windowChanged: w => this._daemon.windowChanged(w),
            windowClosed: id => this._daemon.windowClosed(id),
            focusChanged: id => this._daemon.focusChanged(id),
            monitorsChanged: () => this._daemon.syncMonitors(this._windows.monitors()),
        });
        this._commands = new WindowCommands(id => this._windows.find(id));
        this._daemon = new DaemonClient({
            onConnected: () => this._sendSnapshot(),
            onResyncRequested: () => this._sendSnapshot(),
            // Do not keep keys grabbed on behalf of a daemon that is gone.
            onDisconnected: () => this._keys.clear(),
            onMoveResizeWindow: (req, id, x, y, w, h) =>
                this._complete(req, this._commands.moveResize(id, x, y, w, h)),
            onApplyLayout: placements => this._commands.applyLayout(placements),
            onActivateWorkspace: (req, ws) =>
                this._complete(req, this._commands.activateWorkspace(ws)),
            onMoveWindowToWorkspace: (req, id, ws) =>
                this._complete(req, this._commands.moveToWorkspace(id, ws)),
            onFocusWindow: (req, id) =>
                this._complete(req, this._commands.focus(id)),
            onKeybindingsChanged: bindings => {
                const rejected = this._keys.set(bindings);
                console.log(`liwin: ${bindings.length - rejected.length}/${bindings.length} keybindings grabbed`);
                this._daemon.keybindingsApplied(rejected);
            },
        });
    }

    destroy(): void {
        this._daemon.destroy();
        this._keys.destroy();
        this._windows.destroy();
        this._indicator.destroy();
    }

    /** Reports the outcome of a primitive command (`error` null on success). */
    private _complete(requestId: number, error: string | null): void {
        this._daemon.requestCompleted(requestId, error === null, error ?? '');
    }

    private _sendSnapshot(): void {
        this._daemon.syncMonitors(this._windows.monitors());
        this._daemon.syncWindows(this._windows.focusedId(), this._windows.windows());
    }
}

export default class LiwinExtension extends Extension {
    private _liwin: Liwin | null = null;

    enable(): void {
        this._liwin = new Liwin();
    }

    disable(): void {
        this._liwin?.destroy();
        this._liwin = null;
    }
}
