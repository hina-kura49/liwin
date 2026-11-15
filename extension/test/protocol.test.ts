// SPDX-License-Identifier: GPL-2.0-or-later
// The signatures the extension uses must match docs/dbus.md, the protocol
// both sides are written against.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {describe, it} from 'node:test';

import {METHOD_SIGNATURES, SIGNAL_SIGNATURES} from '../src/protocol.js';

// dist-test/test/protocol.test.js → ../../../docs/dbus.md
const DOC = readFileSync(new URL('../../../docs/dbus.md', import.meta.url), 'utf8');

/** The markdown between `### heading` and the next heading of level <= 3. */
function section(heading: string): string {
    const start = DOC.indexOf(`### ${heading}`);
    assert.notEqual(start, -1, `section "${heading}" not found`);
    const rest = DOC.slice(start + 4 + heading.length);
    const end = rest.search(/^#{2,3} /m);
    return end === -1 ? rest : rest.slice(0, end);
}

/** Splits "a(us) x, u y" at top-level commas. */
function splitArgs(args: string): string[] {
    const out: string[] = [];
    let depth = 0;
    let cur = '';
    for (const c of args) {
        if (c === '(' || c === '{')
            depth++;
        else if (c === ')' || c === '}')
            depth--;
        if (c === ',' && depth === 0) {
            out.push(cur);
            cur = '';
        } else {
            cur += c;
        }
    }
    if (cur.trim())
        out.push(cur);
    return out.map(s => s.trim());
}

/**
 * Parses list items like "- `Name(in u a, out u b)`" into name → GVariant
 * tuple type of the arguments the extension sends (methods: `in` args;
 * signals: all args).
 */
function documented(heading: string): Map<string, string> {
    const out = new Map<string, string>();
    for (const m of section(heading).matchAll(/^- `(\w+)\(([^`]*)\)`/gm)) {
        const types = splitArgs(m[2])
            .map(arg => arg.split(/\s+/))
            .filter(words => words[0] !== 'out')
            .map(words => (words[0] === 'in' ? words[1] : words[0]));
        out.set(m[1], `(${types.join('')})`);
    }
    return out;
}

describe('protocol signatures', () => {
    it('match the documented ShellBridge1 methods', () => {
        assert.deepEqual(Object.fromEntries(documented('Methods (extension → daemon)')), METHOD_SIGNATURES);
    });

    it('match the documented ShellBridge1 signals', () => {
        assert.deepEqual(Object.fromEntries(documented('Signals (daemon → extension)')), SIGNAL_SIGNATURES);
    });
});
