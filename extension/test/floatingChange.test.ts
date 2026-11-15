// SPDX-License-Identifier: GPL-2.0-or-later
import assert from 'node:assert/strict';
import {describe, it} from 'node:test';

import {isFloating} from '../src/floating.js';
import type {WindowState} from '../src/floating.js';
import {FloatingChangeDetector, isFloatingWhileConstrained} from '../src/floatingChange.js';

const normal: WindowState = {transient: false, maximized: false, fullscreen: false, allowsResize: true};
// What mutter reports for a maximized window: allows_resize() turns false.
const maximized: WindowState = {...normal, maximized: true, allowsResize: false};
const fullscreen: WindowState = {...normal, fullscreen: true, allowsResize: false};

describe('isFloatingWhileConstrained', () => {
    it('extends the current rule', () => {
        for (const s of [normal, {...normal, transient: true}, {...normal, allowsResize: false}])
            assert.equal(isFloatingWhileConstrained(s), isFloating(s));
    });

    it('floats maximized and fullscreen windows', () => {
        assert.equal(isFloatingWhileConstrained(maximized), true);
        assert.equal(isFloatingWhileConstrained(fullscreen), true);
    });
});

describe('FloatingChangeDetector', () => {
    it('reports maximize and restore as flag changes', () => {
        const d = new FloatingChangeDetector();
        d.reported(1, normal);
        assert.equal(d.update(1, maximized), true);
        assert.equal(d.update(1, maximized), null);
        assert.equal(d.update(1, normal), false);
    });

    it('reports fullscreen too', () => {
        const d = new FloatingChangeDetector();
        d.reported(1, normal);
        assert.equal(d.update(1, fullscreen), true);
    });

    it('ignores changes that keep the flag', () => {
        const d = new FloatingChangeDetector();
        d.reported(1, {...normal, transient: true});
        assert.equal(d.update(1, {...maximized, transient: true}), null);
        d.reported(2, maximized);
        assert.equal(d.update(2, fullscreen), null);
    });

    it('ignores windows that were never reported or are forgotten', () => {
        const d = new FloatingChangeDetector();
        assert.equal(d.update(9, maximized), null);
        d.reported(9, normal);
        d.forget(9);
        assert.equal(d.update(9, maximized), null);
    });

    it('detects nothing under the current policy', () => {
        const d = new FloatingChangeDetector(isFloating);
        d.reported(1, normal);
        assert.equal(d.update(1, maximized), null);
        assert.equal(d.update(1, fullscreen), null);
    });
});
