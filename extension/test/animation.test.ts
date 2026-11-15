// SPDX-License-Identifier: GPL-2.0-or-later
import assert from 'node:assert/strict';
import {describe, it} from 'node:test';

import {PLACE_ANIMATION_MS, compensatingTranslation, shouldAnimate} from '../src/animation.js';
import type {AnimationConditions} from '../src/animation.js';

const base: AnimationConditions = {
    animationsEnabled: true,
    actorMapped: true,
    overviewVisible: false,
    wasConstrained: false,
    firstPlacement: false,
    from: {x: 0, y: 0},
    to: {x: 100, y: 50},
};

describe('shouldAnimate', () => {
    it('animates a visible move', () => {
        assert.equal(shouldAnimate(base), true);
    });

    it('respects the GNOME animations setting', () => {
        assert.equal(shouldAnimate({...base, animationsEnabled: false}), false);
    });

    it('skips invisible windows and the overview', () => {
        assert.equal(shouldAnimate({...base, actorMapped: false}), false);
        assert.equal(shouldAnimate({...base, overviewVisible: true}), false);
    });

    it('leaves windows to Shell while it animates them itself', () => {
        assert.equal(shouldAnimate({...base, wasConstrained: true}), false);
        assert.equal(shouldAnimate({...base, firstPlacement: true}), false);
    });

    it('does not animate a resize in place', () => {
        assert.equal(shouldAnimate({...base, to: {...base.from}}), false);
        assert.equal(shouldAnimate({...base, to: {x: 0, y: 1}}), true);
    });
});

describe('compensatingTranslation', () => {
    it('keeps a resting actor where it was', () => {
        assert.deepEqual(compensatingTranslation({x: 10, y: 20}, {x: 0, y: 0}, {x: 110, y: 70}), {x: -100, y: -50});
    });

    it('continues from the current point of a running animation', () => {
        // Seen at 110 - 40 = 70; it now moves to 300.
        assert.deepEqual(compensatingTranslation({x: 110, y: 0}, {x: -40, y: 0}, {x: 300, y: 0}), {x: -230, y: 0});
    });

    it('is zero when nothing moved', () => {
        assert.deepEqual(compensatingTranslation({x: 5, y: 5}, {x: 0, y: 0}, {x: 5, y: 5}), {x: 0, y: 0});
    });
});

describe('PLACE_ANIMATION_MS', () => {
    it('is short', () => {
        assert.ok(PLACE_ANIMATION_MS > 0 && PLACE_ANIMATION_MS <= 250);
    });
});
