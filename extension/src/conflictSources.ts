// SPDX-License-Identifier: GPL-2.0-or-later
// Where other parts of the desktop keep their keyboard shortcuts, and which
// of those settings are live shortcuts.
//
// Pure module: imports nothing from gi:// or resource://, so it can be unit
// tested with node. accelerators.ts reads the settings.

/** GNOME's own keybinding schemas; every `as` key is a shortcut. */
export const GNOME_KEYBINDING_SCHEMAS = [
    'org.gnome.desktop.wm.keybindings',
    'org.gnome.shell.keybindings',
    'org.gnome.mutter.keybindings',
    'org.gnome.mutter.wayland.keybindings',
    'org.gnome.settings-daemon.plugins.media-keys',
] as const;

/** gnome-settings-daemon custom shortcuts (Settings → Keyboard → Custom Shortcuts). */
export const CUSTOM_SHORTCUTS = {
    /** `as` key in `listSchema` listing the relocatable settings paths. */
    listSchema: 'org.gnome.settings-daemon.plugins.media-keys',
    listKey: 'custom-keybindings',
    /** Relocatable schema of one custom shortcut. */
    schema: 'org.gnome.settings-daemon.plugins.media-keys.custom-keybinding',
    /** `s` key holding the accelerator. */
    bindingKey: 'binding',
} as const;

/** Ubuntu Dock / Dash to Dock (same schema, docking.js KeyboardShortcuts). */
export const DOCK = {
    uuids: ['ubuntu-dock@ubuntu.com', 'dash-to-dock@micxgx.gmail.com'],
    schema: 'org.gnome.shell.extensions.dash-to-dock',
} as const;

/** Tiling Assistant (Ubuntu's edge-tiling extension). */
export const TILING_ASSISTANT = {
    uuids: ['tiling-assistant@ubuntu.com'],
    schema: 'org.gnome.shell.extensions.tiling-assistant',
} as const;

/** The ExtensionState values (misc/extensionUtils.js) the decision needs. */
export interface ExtensionStates {
    ACTIVE: number;
    ACTIVATING: number;
    ERROR: number;
    OUT_OF_DATE: number;
}

/** The fields of an ExtensionObject (ui/extensionSystem.js) the decision needs. */
export interface ExtensionInfo {
    state?: number;
    /** Listed in org.gnome.shell enabled-extensions. */
    enabled?: boolean;
}

/**
 * Whether an extension's shortcuts must be treated as bound: it is ACTIVE or
 * being enabled, or it is enabled and has not been loaded yet. GNOME Shell
 * loads and enables extensions one after the other (asynchronously), so at
 * liwin's first KeybindingsChanged a later extension may not be ACTIVE
 * yet; never stealing its keys wins over grabbing them.
 */
export function extensionCountsAsActive(ext: ExtensionInfo | undefined, states: ExtensionStates): boolean {
    if (!ext)
        return false;
    if (ext.state === states.ACTIVE || ext.state === states.ACTIVATING)
        return true;
    if (ext.state === states.ERROR || ext.state === states.OUT_OF_DATE)
        return false;
    return ext.enabled === true;
}

/**
 * Whether `path` may be passed to `new Gio.Settings({path})`. GLib aborts
 * the process on an invalid path, which would take GNOME Shell down, so
 * paths read from dconf are checked first.
 */
export function isValidSettingsPath(path: string): boolean {
    return path.startsWith('/') && path.endsWith('/') && !path.includes('//');
}

/** The Dash to Dock settings that decide which of its shortcuts are bound. */
export interface DockFlags {
    'hot-keys': boolean;
    'hotkeys-overlay': boolean;
    'hotkeys-show-dock': boolean;
}

const DOCK_APP_HOTKEY = /^app-(shift-|ctrl-)?hotkey-\d+$/;

/**
 * Keys of the dock schema that are bound right now, given all its keys.
 * Mirrors docking.js: the app hotkeys are bound while `hot-keys` is set;
 * `shortcut` additionally needs `hotkeys-overlay` or `hotkeys-show-dock`.
 */
export function dockBindingKeys(keys: readonly string[], flags: DockFlags): string[] {
    if (!flags['hot-keys'])
        return [];
    const extra = flags['hotkeys-overlay'] || flags['hotkeys-show-dock'];
    return keys.filter(k => DOCK_APP_HOTKEY.test(k) || (extra && k === 'shortcut'));
}

/**
 * Keys of the Tiling Assistant schema that are shortcuts, given its `as`
 * keys: all of them (Shortcuts.getAllKeys(), activate-layoutN,
 * search-popup-layout) except `favorite-layouts`, which holds layout ids.
 */
export function tilingAssistantBindingKeys(stringListKeys: readonly string[]): string[] {
    return stringListKeys.filter(k => k !== 'favorite-layouts');
}
