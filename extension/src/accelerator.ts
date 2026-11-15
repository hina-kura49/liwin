// SPDX-License-Identifier: GPL-2.0-or-later
// Accelerator parsing and conflict bookkeeping.
//
// Pure module: imports nothing from gi:// or resource://, so it can be unit
// tested with node. Reading GSettings lives in accelerators.ts.

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

/** Normalized accelerator → human-readable owner ("schema key"). */
export type BoundAccelerators = Map<string, string>;

/**
 * Records `accels` as owned by `owner`. Unparsable accelerators are ignored;
 * the first owner of an accelerator wins.
 */
export function addBound(bound: BoundAccelerators, owner: string, accels: Iterable<string>): void {
    for (const accel of accels) {
        const norm = normalizeAccelerator(accel);
        if (norm && !bound.has(norm))
            bound.set(norm, owner);
    }
}
