// SPDX-License-Identifier: GPL-2.0-or-later
// Minimal declarations for the parts of Node's standard library the unit
// tests use, so the tests need no @types/node dependency.

declare module 'node:test' {
    type Fn = () => void | Promise<void>;
    export function test(name: string, fn: Fn): Promise<void>;
    export function describe(name: string, fn: () => void): Promise<void>;
    export function it(name: string, fn: Fn): Promise<void>;
}

declare module 'node:assert/strict' {
    interface Assert {
        (value: unknown, message?: string): asserts value;
        ok(value: unknown, message?: string): asserts value;
        equal<T>(actual: unknown, expected: T, message?: string): asserts actual is T;
        notEqual(actual: unknown, expected: unknown, message?: string): void;
        deepEqual<T>(actual: unknown, expected: T, message?: string): asserts actual is T;
        throws(fn: () => unknown, message?: string): void;
    }
    const assert: Assert;
    export default assert;
}

declare module 'node:fs' {
    export function readFileSync(path: URL, encoding: 'utf8'): string;
}

declare class URL {
    constructor(url: string, base?: string);
}

interface ImportMeta {
    readonly url: string;
}
