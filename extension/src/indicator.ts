// SPDX-License-Identifier: GPL-2.0-or-later
// Top bar indicator showing the active workspace number, placed in the
// right box directly left of the system indicators (quick settings).
// Display only: it has no menu and reports nothing to the daemon.

import Clutter from 'gi://Clutter';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';

import {indicatorPosition, workspaceLabel} from './workspaceLabel.js';

/** statusArea role; unique to this extension. */
const ROLE = 'liwin-workspace';

export class WorkspaceIndicator {
    private readonly _button: PanelMenu.Button;
    private readonly _label: St.Label;
    private readonly _signalIds: number[];

    constructor() {
        // (menuAlignment, nameText, dontCreateMenu): no menu, clicks do nothing.
        this._button = new PanelMenu.Button(0.5, 'liwin workspace', true);
        this._label = new St.Label({y_align: Clutter.ActorAlign.CENTER});
        this._button.add_child(this._label);

        const qs = Main.panel.statusArea.quickSettings?.container ?? null;
        const box = qs?.get_parent() ?? null;
        Main.panel.addToStatusArea(ROLE, this._button, indicatorPosition(box?.get_children() ?? [], qs), 'right');

        const wm = global.workspace_manager;
        // Removing or reordering workspaces can change the active index
        // without a switch.
        this._signalIds = ['active-workspace-changed', 'workspace-removed', 'workspaces-reordered']
            .map(signal => wm.connect(signal, () => this._update()));
        this._update();
    }

    destroy(): void {
        for (const id of this._signalIds)
            global.workspace_manager.disconnect(id);
        this._signalIds.length = 0;
        // Also removes it from Main.panel.statusArea (panel.js connects to
        // the button's destroy signal) and destroys its container.
        this._button.destroy();
    }

    private _update(): void {
        this._label.text = workspaceLabel(global.workspace_manager.get_active_workspace_index());
    }
}
