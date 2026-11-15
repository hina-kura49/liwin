// SPDX-License-Identifier: GPL-2.0-or-later
import assert from 'node:assert/strict';
import {describe, it} from 'node:test';

import {addBound, normalizeAccelerator} from '../src/accelerator.js';
import type {BoundAccelerators} from '../src/accelerator.js';

describe('normalizeAccelerator', () => {
    it('sorts modifiers and lowercases the key', () => {
        assert.equal(normalizeAccelerator('<Super><Alt>H'), 'alt+super+h');
        assert.equal(normalizeAccelerator('<Alt><Super>h'), 'alt+super+h');
    });

    it('maps modifier aliases to one name', () => {
        assert.equal(normalizeAccelerator('<Ctrl>t'), 'control+t');
        assert.equal(normalizeAccelerator('<Control>t'), 'control+t');
        assert.equal(normalizeAccelerator('<Primary>t'), 'control+t');
        assert.equal(normalizeAccelerator('<Mod1>t'), 'alt+t');
        assert.equal(normalizeAccelerator('<Mod4>t'), 'super+t');
        assert.equal(normalizeAccelerator('<Meta>t'), 'meta+t');
        assert.equal(normalizeAccelerator('<Hyper>t'), 'hyper+t');
    });

    it('is case-insensitive for modifiers', () => {
        assert.equal(normalizeAccelerator('<SHIFT><super>Return'), 'shift+super+return');
    });

    it('collapses repeated modifiers', () => {
        assert.equal(normalizeAccelerator('<Ctrl><Control>a'), 'control+a');
    });

    it('accepts keys without modifiers', () => {
        assert.equal(normalizeAccelerator('XF86AudioPlay'), 'xf86audioplay');
        assert.equal(normalizeAccelerator('  F1 '), 'f1');
    });

    it('rejects unknown modifiers and missing keys', () => {
        assert.equal(normalizeAccelerator('<Foo>a'), null);
        assert.equal(normalizeAccelerator('<Super>'), null);
        assert.equal(normalizeAccelerator(''), null);
    });

    it('treats the GTK and the GSettings spelling of a combo as equal', () => {
        assert.equal(normalizeAccelerator('<Shift><Super>1'), normalizeAccelerator('<Super><Shift>1'));
    });
});

describe('addBound', () => {
    it('records normalized accelerators with their owner', () => {
        const bound: BoundAccelerators = new Map();
        addBound(bound, 'schema key', ['<Super>a', '<Ctrl><Alt>T']);
        assert.deepEqual([...bound], [['super+a', 'schema key'], ['alt+control+t', 'schema key']]);
    });

    it('keeps the first owner and skips unparsable entries', () => {
        const bound: BoundAccelerators = new Map();
        addBound(bound, 'first', ['<Super>a']);
        addBound(bound, 'second', ['<Mod4>A', '<Bogus>x', '']);
        assert.deepEqual([...bound], [['super+a', 'first']]);
    });
});
