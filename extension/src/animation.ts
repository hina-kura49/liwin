// SPDX-License-Identifier: GPL-2.0-or-later
// Decisions and arithmetic of the placement animation (see placementAnimator.ts).
//
// Pure module: imports nothing from gi:// or resource://, so it can be unit
// tested with node.
//
// The animation is visual only: the window gets exactly the frame the daemon
// asked for at once; only the actor's translation is eased from "where it
// was on screen" back to zero.

/** Duration of the move animation, before St's slow-down factor. */
export const PLACE_ANIMATION_MS = 150;

/**
 * How long to wait for the actor to reach the requested position after
 * move_resize_frame(). Wayland clients move when they commit the resized
 * buffer, which may take a while; later moves are not animated.
 */
export const PLACE_WATCH_TIMEOUT_MS = 1000;

export interface AnimationConditions {
    /** St.Settings.get().enable_animations */
    animationsEnabled: boolean;
    /** The window actor is mapped (on the active workspace, not minimized). */
    actorMapped: boolean;
    /** The overview is shown; window actors are not visible as such. */
    overviewVisible: boolean;
    /**
     * The window was maximized or fullscreen and is being restored: GNOME
     * Shell runs its own size-change animation on the actor then.
     */
    wasConstrained: boolean;
    /**
     * First placement after the window opened: GNOME Shell's map animation
     * is running and resets the actor's translation when it ends.
     */
    firstPlacement: boolean;
    /** Current frame position. */
    from: {x: number; y: number};
    /** Requested frame position. */
    to: {x: number; y: number};
}

/** Whether to animate one placement. */
export function shouldAnimate(c: AnimationConditions): boolean {
    if (!c.animationsEnabled || !c.actorMapped || c.overviewVisible)
        return false;
    if (c.wasConstrained || c.firstPlacement)
        return false;
    return c.from.x !== c.to.x || c.from.y !== c.to.y;
}

export interface Point {
    x: number;
    y: number;
}

/**
 * The translation that keeps an actor visually in place after its position
 * changed from `prev` to `next`, given its current translation (which may be
 * mid-animation). Easing that translation to zero then slides the actor from
 * where it was seen to its new position.
 */
export function compensatingTranslation(prev: Point, translation: Point, next: Point): Point {
    return {
        x: prev.x + translation.x - next.x,
        y: prev.y + translation.y - next.y,
    };
}
