// liwin GNOME Shell extension entry point.
//
// A thin layer: it reports windows to liwin-daemon, executes the daemon's
// primitive commands and forwards keybinding presses. All logic lives in the
// daemon. See docs/ARCHITECTURE.md.

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

import {DaemonClient} from './daemon.js';
import {WindowTracker} from './windows.js';

class Liwin {
    private readonly _daemon: DaemonClient;
    private readonly _windows: WindowTracker;

    constructor() {
        this._windows = new WindowTracker({
            windowOpened: w => this._daemon.windowOpened(w),
            windowClosed: id => this._daemon.windowClosed(id),
            focusChanged: id => this._daemon.focusChanged(id),
            monitorsChanged: () => this._daemon.syncMonitors(this._windows.monitors()),
        });
        this._daemon = new DaemonClient({
            onConnected: () => this._sendSnapshot(),
            onResyncRequested: () => this._sendSnapshot(),
            onDisconnected: () => {},
            onMoveResizeWindow: (req, id, x, y, w, h) => {
                const err = this._windows.moveResize(id, x, y, w, h);
                this._daemon.requestCompleted(req, err === null, err ?? '');
            },
            onFocusWindow: (req, id) => {
                const err = this._windows.focus(id);
                this._daemon.requestCompleted(req, err === null, err ?? '');
            },
            // Keybindings are not implemented yet: report all as rejected.
            onKeybindingsChanged: bindings => {
                this._daemon.keybindingsApplied(bindings.map(([id]) => id));
            },
        });
    }

    destroy(): void {
        this._daemon.destroy();
        this._windows.destroy();
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
