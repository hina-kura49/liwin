// Global keybindings via global.display.grab_accelerator(), the same
// mechanism GNOME Shell's own GrabAccelerator D-Bus API uses
// (js/ui/shellDBus.js). Unlike Main.wm.addKeybinding() it needs no GSettings
// schema, so bindings can come from the user's TOML file.
//
// Conflicts: we never steal a key. A binding is skipped and reported as
// rejected when (a) GNOME's keybinding settings already use the accelerator
// (see accelerators.ts — grab_accelerator() alone was observed not to catch
// this), or (b) grab_accelerator() returns Meta.KeyBindingAction.NONE.

import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import {gnomeBoundAccelerators, normalizeAccelerator} from './accelerators.js';

export class KeybindingManager {
    /** grab action id → daemon binding id */
    private readonly _actions = new Map<number, number>();
    private readonly _onTriggered: (bindingId: number) => void;
    private readonly _signalId: number;

    constructor(onTriggered: (bindingId: number) => void) {
        this._onTriggered = onTriggered;
        this._signalId = global.display.connect('accelerator-activated', (_d, action) => {
            const bindingId = this._actions.get(action);
            if (bindingId !== undefined)
                this._onTriggered(bindingId);
        });
    }

    /** Replaces all bindings. Returns the binding ids that could not be grabbed. */
    set(bindings: [number, string][]): number[] {
        this.clear();
        const rejected: number[] = [];
        const gnomeBound = gnomeBoundAccelerators();
        for (const [bindingId, accelerator] of bindings) {
            const norm = normalizeAccelerator(accelerator);
            const owner = norm ? gnomeBound.get(norm) : undefined;
            if (owner) {
                console.warn(`liwin: ${accelerator} is used by GNOME (${owner}), skipping`);
                rejected.push(bindingId);
                continue;
            }
            const action = global.display.grab_accelerator(accelerator, Meta.KeyBindingFlags.NONE);
            if (action === Meta.KeyBindingAction.NONE) {
                console.warn(`liwin: cannot grab ${accelerator} (already in use?), skipping`);
                rejected.push(bindingId);
                continue;
            }
            Main.wm.allowKeybinding(Meta.external_binding_name_for_action(action), Shell.ActionMode.NORMAL);
            this._actions.set(action, bindingId);
        }
        return rejected;
    }

    /** Ungrabs every accelerator this manager grabbed. */
    clear(): void {
        for (const action of this._actions.keys()) {
            Main.wm.allowKeybinding(Meta.external_binding_name_for_action(action), Shell.ActionMode.NONE);
            if (!global.display.ungrab_accelerator(action))
                console.warn(`liwin: failed to ungrab accelerator action ${action}`);
        }
        this._actions.clear();
    }

    destroy(): void {
        this.clear();
        global.display.disconnect(this._signalId);
    }
}
