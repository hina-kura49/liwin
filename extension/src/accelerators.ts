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

const KEYBINDING_SCHEMAS = [
    'org.gnome.desktop.wm.keybindings',
    'org.gnome.shell.keybindings',
    'org.gnome.mutter.keybindings',
    'org.gnome.mutter.wayland.keybindings',
    'org.gnome.settings-daemon.plugins.media-keys',
];

const MODIFIER_ALIASES: Record<string, string> = {
    control: 'control', ctrl: 'control', primary: 'control',
    alt: 'alt', mod1: 'alt',
    shift: 'shift',
    super: 'super', mod4: 'super',
    meta: 'meta',
    hyper: 'hyper',
};

/**
 * Normalizes a GTK accelerator string (`<Ctrl><Alt>T`) to a canonical form
 * (`alt+control+t`), or returns null if it cannot be parsed.
 */
export function normalizeAccelerator(accel: string): string | null {
    const mods = new Set<string>();
    let rest = accel.trim();
    for (;;) {
        const m = /^<([^>]+)>/.exec(rest);
        if (!m)
            break;
        const mod = MODIFIER_ALIASES[m[1].toLowerCase()];
        if (!mod)
            return null;
        mods.add(mod);
        rest = rest.slice(m[0].length);
    }
    if (rest.length === 0)
        return null;
    return [...[...mods].sort(), rest.toLowerCase()].join('+');
}

/** Map of normalized accelerator → "schema key" currently bound by GNOME. */
export function gnomeBoundAccelerators(): Map<string, string> {
    const bound = new Map<string, string>();
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
            for (const accel of settings.get_strv(key)) {
                const norm = normalizeAccelerator(accel);
                if (norm && !bound.has(norm))
                    bound.set(norm, `${id} ${key}`);
            }
        }
    }
    return bound;
}
