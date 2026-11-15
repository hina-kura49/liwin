// SPDX-License-Identifier: GPL-2.0-or-later
import assert from 'node:assert/strict';
import {describe, it} from 'node:test';

import {indicatorPosition, workspaceLabel} from '../src/workspaceLabel.js';

describe('workspaceLabel', () => {
    it('shows the 1-based workspace number', () => {
        assert.equal(workspaceLabel(0), '1');
        assert.equal(workspaceLabel(9), '10');
    });

    it('is empty without an active workspace', () => {
        assert.equal(workspaceLabel(-1), '');
    });
});

describe('indicatorPosition', () => {
    it('inserts directly before quick settings', () => {
        assert.equal(indicatorPosition(['a11y', 'keyboard', 'quickSettings'], 'quickSettings'), 2);
    });

    it('falls back to the start of the box', () => {
        assert.equal(indicatorPosition(['a', 'b'], null), 0);
        assert.equal(indicatorPosition(['a', 'b'], undefined), 0);
        assert.equal(indicatorPosition(['a', 'b'], 'quickSettings'), 0);
    });
});
