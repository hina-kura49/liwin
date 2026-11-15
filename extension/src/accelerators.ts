// SPDX-License-Identifier: GPL-2.0-or-later
// Detects accelerators already used by GNOME's own keybindings.
//
// global.display.grab_accelerator() is supposed to refuse combos that are
// already bound, but in practice (GNOME Shell 50.1, nested/headless) it
// accepted <Super>a while GNOME Shell's toggle-application-view used it. So
// liwin checks the keybinding GSettings schemas itself before grabbing.
//
// Known gaps: custom shortcuts of gnome-settings-daemon (relocatable schema)
// and the overlay key are not inspected.

import Gio from 'gi://Gio';

import {addBound} from './accelerator.js';
import type {BoundAccelerators} from './accelerator.js';

const KEYBINDING_SCHEMAS = [
    'org.gnome.desktop.wm.keybindings',
    'org.gnome.shell.keybindings',
    'org.gnome.mutter.keybindings',
    'org.gnome.mutter.wayland.keybindings',
    'org.gnome.settings-daemon.plugins.media-keys',
];

/** Map of normalized accelerator → "schema key" currently bound by GNOME. */
export function gnomeBoundAccelerators(): BoundAccelerators {
    const bound: BoundAccelerators = new Map();
    const source = Gio.SettingsSchemaSource.get_default();
    if (!source)
        return bound;
    for (const id of KEYBINDING_SCHEMAS) {
        const schema = source.lookup(id, true);
        if (!schema)
            continue;
        const settings = new Gio.Settings({settings_schema: schema});
        for (const key of schema.list_keys()) {
            if (schema.get_key(key).get_value_type().dup_string() !== 'as')
                continue;
            addBound(bound, `${id} ${key}`, settings.get_strv(key));
        }
    }
    return bound;
}
