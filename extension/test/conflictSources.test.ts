// SPDX-License-Identifier: GPL-2.0-or-later
import assert from 'node:assert/strict';
import {describe, it} from 'node:test';

import {
    dockBindingKeys, extensionCountsAsActive, isValidSettingsPath, tilingAssistantBindingKeys,
} from '../src/conflictSources.js';
import type {DockFlags} from '../src/conflictSources.js';

// Values of GNOME Shell 50's misc/extensionUtils.js ExtensionState.
const STATES = {ACTIVE: 1, INACTIVE: 2, ERROR: 3, OUT_OF_DATE: 4, INITIALIZED: 6, DEACTIVATING: 7, ACTIVATING: 8};

describe('extensionCountsAsActive', () => {
    it('is false for unknown extensions', () => {
        assert.equal(extensionCountsAsActive(undefined, STATES), false);
    });

    it('is true while active or being enabled', () => {
        assert.equal(extensionCountsAsActive({state: STATES.ACTIVE, enabled: true}, STATES), true);
        assert.equal(extensionCountsAsActive({state: STATES.ACTIVATING, enabled: true}, STATES), true);
    });

    it('is true for enabled extensions that are not loaded yet', () => {
        assert.equal(extensionCountsAsActive({enabled: true}, STATES), true);
        assert.equal(extensionCountsAsActive({state: STATES.INITIALIZED, enabled: true}, STATES), true);
    });

    it('is false for disabled or broken extensions', () => {
        assert.equal(extensionCountsAsActive({state: STATES.INACTIVE, enabled: false}, STATES), false);
        assert.equal(extensionCountsAsActive({state: STATES.DEACTIVATING, enabled: false}, STATES), false);
        assert.equal(extensionCountsAsActive({state: STATES.ERROR, enabled: true}, STATES), false);
        assert.equal(extensionCountsAsActive({state: STATES.OUT_OF_DATE, enabled: true}, STATES), false);
    });
});

describe('isValidSettingsPath', () => {
    it('accepts dconf directory paths', () => {
        assert.equal(isValidSettingsPath('/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/custom0/'), true);
    });

    it('rejects paths GLib would abort on', () => {
        assert.equal(isValidSettingsPath(''), false);
        assert.equal(isValidSettingsPath('relative/'), false);
        assert.equal(isValidSettingsPath('/no/trailing/slash'), false);
        assert.equal(isValidSettingsPath('/double//slash/'), false);
    });
});

describe('dockBindingKeys', () => {
    const keys = [
        'app-hotkey-1', 'app-hotkey-10', 'app-shift-hotkey-1', 'app-ctrl-hotkey-9',
        'shortcut', 'shortcut-text', 'shortcut-timeout', 'hot-keys', 'hotkeys-overlay', 'dock-position',
    ];
    const off: DockFlags = {'hot-keys': false, 'hotkeys-overlay': false, 'hotkeys-show-dock': false};

    it('is empty while hot-keys is off', () => {
        assert.deepEqual(dockBindingKeys(keys, {...off, 'hotkeys-overlay': true, 'hotkeys-show-dock': true}), []);
    });

    it('lists the app hotkeys while hot-keys is on', () => {
        assert.deepEqual(dockBindingKeys(keys, {...off, 'hot-keys': true}),
            ['app-hotkey-1', 'app-hotkey-10', 'app-shift-hotkey-1', 'app-ctrl-hotkey-9']);
    });

    it('adds shortcut when the overlay or show-dock option is on', () => {
        for (const extra of ['hotkeys-overlay', 'hotkeys-show-dock'] as const) {
            const listed = dockBindingKeys(keys, {...off, 'hot-keys': true, [extra]: true});
            assert.ok(listed.includes('shortcut'), extra);
            assert.ok(!listed.includes('shortcut-text'), extra);
        }
    });
});

describe('tilingAssistantBindingKeys', () => {
    it('keeps every shortcut key and drops favorite-layouts', () => {
        assert.deepEqual(
            tilingAssistantBindingKeys(['tile-left-half', 'activate-layout0', 'favorite-layouts', 'search-popup-layout']),
            ['tile-left-half', 'activate-layout0', 'search-popup-layout']);
    });
});
