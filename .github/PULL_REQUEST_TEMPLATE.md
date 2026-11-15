<!--
The PR title becomes the squash-merge commit and must follow Conventional
Commits, e.g. "fix(daemon): keep focus when a window closes".
Allowed types: feat, fix, docs, refactor, test, ci, build, perf, chore, revert.
-->

## What and why

<!-- What does this change, and why? Link the issue: "Closes #123". -->

## How it was tested

<!-- Never test on your logged-in session; see CONTRIBUTING.md. -->

- [ ] `just lint`
- [ ] `just test`
- [ ] `just e2e` (needed when the extension or the D-Bus protocol changes)
- [ ] Tried by hand in `just nested`

## Checklist

- [ ] The PR title follows Conventional Commits (`!` marks a breaking change)
- [ ] Tests cover the change, or the description says why they cannot
- [ ] Docs updated (`README.md`, `docs/`), and for D-Bus changes `docs/dbus.md`
      and the introspection snapshot
- [ ] New files carry an `SPDX-License-Identifier` header for their directory
