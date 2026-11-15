// SPDX-License-Identifier: GPL-2.0-or-later
// Decides which daemon keybindings may be grabbed.
//
// Pure module: imports nothing from gi:// or resource://, so it can be unit
// tested with node. keybindings.ts performs the actual grabs.

import {normalizeAccelerator} from './accelerator.js';
import type {BoundAccelerators} from './accelerator.js';

export interface BindingConflict {
    bindingId: number;
    accelerator: string;
    /** Who already uses the accelerator, e.g. "org.gnome.shell.keybindings toggle-overview". */
    owner: string;
}

export interface BindingPlan {
    /** `(binding_id, accelerator)` pairs to pass to grab_accelerator(), in order. */
    grab: [number, string][];
    /** Bindings skipped because someone else already uses the accelerator. */
    conflicts: BindingConflict[];
}

/** Splits `bindings` into those to grab and those already bound elsewhere. */
export function planBindings(bindings: [number, string][], bound: BoundAccelerators): BindingPlan {
    const plan: BindingPlan = {grab: [], conflicts: []};
    for (const [bindingId, accelerator] of bindings) {
        const norm = normalizeAccelerator(accelerator);
        const owner = norm ? bound.get(norm) : undefined;
        if (owner)
            plan.conflicts.push({bindingId, accelerator, owner});
        else
            plan.grab.push([bindingId, accelerator]);
    }
    return plan;
}
