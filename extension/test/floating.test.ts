// SPDX-License-Identifier: GPL-2.0-or-later
import assert from 'node:assert/strict';
import {describe, it} from 'node:test';

import {isFloating, windowFlags} from '../src/floating.js';
import type {WindowState} from '../src/floating.js';
import {FLAG_FLOATING} from '../src/protocol.js';

const normal: WindowState = {transient: false, maximized: false, fullscreen: false, allowsResize: true};

describe('isFloating', () => {
    it('tiles ordinary resizable windows', () => {
        assert.equal(isFloating(normal), false);
    });

    it('floats transient windows whatever their state', () => {
        assert.equal(isFloating({...normal, transient: true}), true);
        assert.equal(isFloating({...normal, transient: true, maximized: true}), true);
    });

    it('floats fixed-size windows', () => {
        assert.equal(isFloating({...normal, allowsResize: false}), true);
    });

    it('does not trust allows_resize() while maximized or fullscreen', () => {
        assert.equal(isFloating({...normal, allowsResize: false, maximized: true}), false);
        assert.equal(isFloating({...normal, allowsResize: false, fullscreen: true}), false);
    });
});

describe('windowFlags', () => {
    it('sets FLAG_FLOATING only for floating windows', () => {
        assert.equal(windowFlags(normal), 0);
        assert.equal(windowFlags({...normal, transient: true}), FLAG_FLOATING);
    });
});
