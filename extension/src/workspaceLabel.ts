// SPDX-License-Identifier: GPL-2.0-or-later
// Text of the top bar workspace indicator.
//
// Pure module: imports nothing from gi:// or resource://, so it can be unit
// tested with node.

/** The 1-based workspace number liwin's commands use (`workspace <n>`). */
export function workspaceLabel(activeIndex: number): string {
    return activeIndex >= 0 ? String(activeIndex + 1) : '';
}

/**
 * Where to insert the indicator in the panel's right box so that it sits
 * directly left of the system indicators: the index of the quick settings
 * button there, or 0 (leftmost) when it is not in that box.
 */
export function indicatorPosition<T>(boxChildren: readonly T[], quickSettings: T | null | undefined): number {
    if (quickSettings === null || quickSettings === undefined)
        return 0;
    const i = boxChildren.indexOf(quickSettings);
    return i === -1 ? 0 : i;
}
