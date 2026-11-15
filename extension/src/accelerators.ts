// SPDX-License-Identifier: GPL-2.0-or-later
// Detects accelerators already used by GNOME and by the shortcut-using
// extensions Ubuntu ships.
//
// global.display.grab_accelerator() is supposed to refuse combos that are
// already bound, but in practice (GNOME Shell 50.1, nested/headless) it
// accepted <Super>a while GNOME Shell's toggle-application-view used it. So
// liwin reads the keybinding settings itself before grabbing:
//
// - GNOME's keybinding schemas (every `as` key),
// - gnome-settings-daemon custom shortcuts,
// - Ubuntu Dock / Dash to Dock hotkeys, while that extension is active and
//   its hot-keys setting is on,
// - Tiling Assistant shortcuts, while that extension is active.
//
// The result is computed whenever the daemon sends its keybindings; later
// settings changes are not followed.
//
// Known gap: the overlay key (org.gnome.mutter overlay-key) is not inspected.
//
// GLib aborts the process when a missing key or schema is read, so every
// lookup is checked first.

import Gio from 'gi://Gio';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {ExtensionState} from 'resource:///org/gnome/shell/misc/extensionUtils.js';

import {addBound} from './accelerator.js';
import type {BoundAccelerators} from './accelerator.js';
import {
    CUSTOM_SHORTCUTS, DOCK, GNOME_KEYBINDING_SCHEMAS, TILING_ASSISTANT,
    dockBindingKeys, extensionCountsAsActive, isValidSettingsPath, tilingAssistantBindingKeys,
} from './conflictSources.js';
import type {DockFlags, ExtensionInfo, ExtensionStates} from './conflictSources.js';

// GNOME Shell 50's misc/extensionUtils.js names the states ACTIVE,
// ACTIVATING, ...; @girs/gnome-shell 50.0.4 still has the old names
// (ENABLED, ENABLING), which are undefined at runtime.
const STATES = ExtensionState as unknown as ExtensionStates;

/** Map of normalized accelerator → owner ("schema key") currently bound. */
export function boundAccelerators(): BoundAccelerators {
    const bound: BoundAccelerators = new Map();
    const source = Gio.SettingsSchemaSource.get_default();
    if (!source)
        return bound;
    for (const id of GNOME_KEYBINDING_SCHEMAS) {
        const schema = source.lookup(id, true);
        if (schema)
            addStringLists(bound, id, schema, new Gio.Settings({settings_schema: schema}), stringListKeys(schema));
    }
    addCustomShortcuts(bound, source);
    addDock(bound, source);
    addTilingAssistant(bound, source);
    return bound;
}

function stringListKeys(schema: Gio.SettingsSchema): string[] {
    return schema.list_keys().filter(k => schema.get_key(k).get_value_type().dup_string() === 'as');
}

function addStringLists(bound: BoundAccelerators, owner: string, schema: Gio.SettingsSchema,
    settings: Gio.Settings, keys: readonly string[]): void {
    for (const key of keys) {
        if (schema.has_key(key) && schema.get_key(key).get_value_type().dup_string() === 'as')
            addBound(bound, `${owner} ${key}`, settings.get_strv(key));
    }
}

function addCustomShortcuts(bound: BoundAccelerators, source: Gio.SettingsSchemaSource): void {
    const listSchema = source.lookup(CUSTOM_SHORTCUTS.listSchema, true);
    const schema = source.lookup(CUSTOM_SHORTCUTS.schema, true);
    if (!listSchema?.has_key(CUSTOM_SHORTCUTS.listKey) || !schema?.has_key(CUSTOM_SHORTCUTS.bindingKey))
        return;
    const paths = new Gio.Settings({settings_schema: listSchema}).get_strv(CUSTOM_SHORTCUTS.listKey);
    for (const path of paths) {
        if (!isValidSettingsPath(path)) {
            console.warn(`liwin: ignoring invalid custom shortcut path ${JSON.stringify(path)}`);
            continue;
        }
        const binding = new Gio.Settings({settings_schema: schema, path}).get_string(CUSTOM_SHORTCUTS.bindingKey);
        addBound(bound, `custom shortcut ${path}`, [binding]);
    }
}

function addDock(bound: BoundAccelerators, source: Gio.SettingsSchemaSource): void {
    const found = activeExtensionSchema(DOCK.uuids, DOCK.schema, source);
    if (!found)
        return;
    const [owner, schema] = found;
    const settings = new Gio.Settings({settings_schema: schema});
    const flag = (k: keyof DockFlags) => schema.has_key(k) && settings.get_boolean(k);
    const flags: DockFlags = {
        'hot-keys': flag('hot-keys'),
        'hotkeys-overlay': flag('hotkeys-overlay'),
        'hotkeys-show-dock': flag('hotkeys-show-dock'),
    };
    addStringLists(bound, owner, schema, settings, dockBindingKeys(schema.list_keys(), flags));
}

function addTilingAssistant(bound: BoundAccelerators, source: Gio.SettingsSchemaSource): void {
    const found = activeExtensionSchema(TILING_ASSISTANT.uuids, TILING_ASSISTANT.schema, source);
    if (!found)
        return;
    const [owner, schema] = found;
    addStringLists(bound, owner, schema, new Gio.Settings({settings_schema: schema}),
        tilingAssistantBindingKeys(stringListKeys(schema)));
}

/**
 * The settings schema `id` of the first active extension among `uuids` (see
 * extensionCountsAsActive()), with
 * "uuid schema" as owner label. Looks in the system schemas, then in the
 * extension's own schemas/ directory (user-installed extensions ship theirs).
 */
function activeExtensionSchema(uuids: readonly string[], id: string,
    source: Gio.SettingsSchemaSource): [string, Gio.SettingsSchema] | null {
    for (const uuid of uuids) {
        // lookup() returns undefined for unknown uuids at runtime.
        const ext = Main.extensionManager.lookup(uuid) as ReturnType<typeof Main.extensionManager.lookup> | undefined;
        if (!ext || !extensionCountsAsActive(ext as ExtensionInfo, STATES))
            continue;
        let schema = source.lookup(id, true);
        if (!schema) {
            const dir = ext.dir.get_child('schemas');
            if (dir.query_exists(null)) {
                try {
                    schema = Gio.SettingsSchemaSource.new_from_directory(dir.get_path()!, source, false)
                        .lookup(id, false);
                } catch (e) {
                    console.warn(`liwin: cannot read schemas of ${uuid}: ${e}`);
                }
            }
        }
        if (schema)
            return [`${uuid} ${id}`, schema];
    }
    return null;
}
