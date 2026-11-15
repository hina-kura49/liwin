// SPDX-License-Identifier: GPL-2.0-or-later
// Slides windows to the place the daemon gave them.
//
// move_resize_frame() is called exactly as before; the window's frame is
// what the daemon asked for. When the window actor then moves (immediately,
// or when a Wayland client commits its resized buffer), its translation is
// set so that it stays where it was on screen and eased back to zero.
// Only the position is animated; the size changes at once.
//
// Skipped when GNOME animations are off (St.Settings enable-animations, the
// same check as misc/animationUtils.js adjustAnimationTime()), in the
// overview, for unmapped actors, for windows that were maximized or
// fullscreen (Shell animates that size change itself) and for a window's
// first placement (Shell's map animation is running).

import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import type Meta from 'gi://Meta';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import {PLACE_ANIMATION_MS, PLACE_WATCH_TIMEOUT_MS, compensatingTranslation, shouldAnimate} from './animation.js';
import type {Point} from './animation.js';

interface Tracked {
    actor: Meta.WindowActor;
    window: Meta.Window;
    destroyId: number;
    /** notify::x / notify::y handlers while waiting for the move; empty otherwise. */
    watchIds: number[];
    /** GLib source ending the watch; 0 when not watching. */
    timeoutId: number;
    /** Actor position last seen. */
    last: Point;
    /** Requested frame position. */
    target: Point;
    /** Generation of the running ease; 0 when none runs. */
    easing: number;
}

const TRANSLATION_PROPS = ['translation-x', 'translation-y'];

type EaseParams = Parameters<Meta.WindowActor['ease']>[0];

export class PlacementAnimator {
    private readonly _tracked = new Map<Meta.WindowActor, Tracked>();
    /** Windows reported as opened that were not placed yet. */
    private readonly _fresh = new Set<number>();
    private _generation = 0;

    windowOpened(id: number): void {
        this._fresh.add(id);
    }

    windowClosed(id: number): void {
        this._fresh.delete(id);
    }

    /**
     * Call right before `win.move_resize_frame(false, x, y, ...)`.
     * `wasConstrained`: the window was maximized or fullscreen.
     */
    beforeMove(win: Meta.Window, x: number, y: number, wasConstrained: boolean): void {
        const firstPlacement = this._fresh.delete(win.get_id());
        const actor = win.get_compositor_private<Meta.WindowActor | null>();
        if (!actor)
            return;
        const frame = win.get_frame_rect();
        const animate = shouldAnimate({
            animationsEnabled: St.Settings.get().enable_animations,
            actorMapped: actor.mapped,
            overviewVisible: Main.overview.visible,
            wasConstrained,
            firstPlacement,
            from: {x: frame.x, y: frame.y},
            to: {x, y},
        });
        const existing = this._tracked.get(actor);
        if (!animate) {
            // Do not slide this window for a move that is not ours to animate.
            if (existing)
                this._forget(existing, true);
            return;
        }
        const t = existing ?? this._track(actor, win);
        t.target = {x, y};
        t.last = {x: actor.x, y: actor.y};
        this._watch(t);
    }

    /** Stops every animation and puts all actors back at zero translation. */
    destroy(): void {
        for (const t of [...this._tracked.values()])
            this._forget(t, true);
        this._fresh.clear();
    }

    private _track(actor: Meta.WindowActor, window: Meta.Window): Tracked {
        const t: Tracked = {
            actor, window, destroyId: 0, watchIds: [], timeoutId: 0,
            last: {x: 0, y: 0}, target: {x: 0, y: 0}, easing: 0,
        };
        t.destroyId = actor.connect('destroy', () => this._forget(t, false));
        this._tracked.set(actor, t);
        return t;
    }

    private _watch(t: Tracked): void {
        if (t.watchIds.length === 0) {
            t.watchIds.push(
                t.actor.connect('notify::x', () => this._onActorMoved(t)),
                t.actor.connect('notify::y', () => this._onActorMoved(t)));
        }
        if (t.timeoutId)
            GLib.source_remove(t.timeoutId);
        t.timeoutId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, PLACE_WATCH_TIMEOUT_MS, () => {
            t.timeoutId = 0;
            this._stopWatching(t);
            this._forgetIfIdle(t);
            return GLib.SOURCE_REMOVE;
        });
    }

    private _stopWatching(t: Tracked): void {
        for (const id of t.watchIds)
            t.actor.disconnect(id);
        t.watchIds.length = 0;
        if (t.timeoutId) {
            GLib.source_remove(t.timeoutId);
            t.timeoutId = 0;
        }
    }

    private _onActorMoved(t: Tracked): void {
        const {actor} = t;
        const next = {x: actor.x, y: actor.y};
        if (next.x === t.last.x && next.y === t.last.y)
            return;
        const translation = compensatingTranslation(
            t.last, {x: actor.translation_x, y: actor.translation_y}, next);
        t.last = next;

        const generation = ++this._generation;
        t.easing = generation;
        for (const prop of TRANSLATION_PROPS)
            actor.remove_transition(prop);
        actor.translation_x = translation.x;
        actor.translation_y = translation.y;
        // snake_case on purpose: Shell's ease() derives the transition names
        // to cancel from the keys (translation_x → translation-x).
        actor.ease({
            translation_x: 0,
            translation_y: 0,
            duration: PLACE_ANIMATION_MS,
            mode: Clutter.AnimationMode.EASE_OUT_QUAD,
            onStopped: () => {
                if (t.easing !== generation)
                    return;
                // Also reached when someone else removed the transition.
                t.easing = 0;
                if (this._tracked.get(actor) === t) {
                    actor.translation_x = 0;
                    actor.translation_y = 0;
                }
                this._forgetIfIdle(t);
            },
        } as unknown as EaseParams);

        const frame = t.window.get_frame_rect();
        if (frame.x === t.target.x && frame.y === t.target.y)
            this._stopWatching(t);
    }

    private _forgetIfIdle(t: Tracked): void {
        if (t.easing === 0 && t.watchIds.length === 0 && this._tracked.get(t.actor) === t)
            this._forget(t, false);
    }

    /** Drops all state of `t`; `reset` also stops its animation. */
    private _forget(t: Tracked, reset: boolean): void {
        if (this._tracked.get(t.actor) !== t)
            return;
        this._tracked.delete(t.actor);
        this._stopWatching(t);
        t.actor.disconnect(t.destroyId);
        if (reset && t.easing !== 0) {
            t.easing = 0;
            for (const prop of TRANSLATION_PROPS)
                t.actor.remove_transition(prop);
            t.actor.translation_x = 0;
            t.actor.translation_y = 0;
        }
        t.easing = 0;
    }
}
