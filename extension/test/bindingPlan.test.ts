// SPDX-License-Identifier: GPL-2.0-or-later
import assert from 'node:assert/strict';
import {describe, it} from 'node:test';

import {addBound} from '../src/accelerator.js';
import type {BoundAccelerators} from '../src/accelerator.js';
import {planBindings} from '../src/bindingPlan.js';

function bound(entries: [string, string[]][]): BoundAccelerators {
    const map: BoundAccelerators = new Map();
    for (const [owner, accels] of entries)
        addBound(map, owner, accels);
    return map;
}

describe('planBindings', () => {
    it('grabs everything when nothing conflicts', () => {
        const plan = planBindings([[1, '<Alt>h'], [2, '<Alt>l']], new Map());
        assert.deepEqual(plan, {grab: [[1, '<Alt>h'], [2, '<Alt>l']], conflicts: []});
    });

    it('never steals an accelerator GNOME already uses', () => {
        const gnome = bound([['org.gnome.shell.keybindings toggle-application-view', ['<Super>a']]]);
        const plan = planBindings([[1, '<Super>a'], [2, '<Alt>h']], gnome);
        assert.deepEqual(plan.grab, [[2, '<Alt>h']]);
        assert.deepEqual(plan.conflicts, [{
            bindingId: 1,
            accelerator: '<Super>a',
            owner: 'org.gnome.shell.keybindings toggle-application-view',
        }]);
    });

    it('compares accelerators after normalization', () => {
        const gnome = bound([['wm switch', ['<Shift><Super>1']]]);
        const plan = planBindings([[7, '<Super><Shift>1']], gnome);
        assert.deepEqual(plan.grab, []);
        assert.equal(plan.conflicts[0]?.bindingId, 7);
    });

    it('leaves unparsable accelerators to grab_accelerator()', () => {
        const plan = planBindings([[3, '<Bogus>x']], bound([['x', ['<Super>x']]]));
        assert.deepEqual(plan.grab, [[3, '<Bogus>x']]);
    });

    it('keeps the daemon order', () => {
        const plan = planBindings([[3, '<Alt>3'], [1, '<Alt>1'], [2, '<Alt>2']], new Map());
        assert.deepEqual(plan.grab.map(([id]) => id), [3, 1, 2]);
    });
});
