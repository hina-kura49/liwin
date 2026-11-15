# Security policy

## Supported versions

liwin is pre-1.0. Only the latest `0.x` release receives security fixes;
fixes are not backported to older releases.

| Version | Supported |
|---------|-----------|
| latest 0.x release | yes |
| older releases | no |

## Reporting a vulnerability

Please **do not** open a public issue, pull request or discussion for a
security problem.

Report it privately through GitHub:
[**Report a vulnerability**](https://github.com/hina-kura49/liwin/security/advisories/new)
(repository "Security" tab -> "Advisories" -> "Report a vulnerability").
Only the maintainer, [@hina-kura49](https://github.com/hina-kura49), can see
the report.

Please include:

- the liwin version (`liwin --version`, `liwin-daemon --version`) or commit,
- GNOME Shell version and distribution,
- what an attacker can do and under which conditions (e.g. another local
  user, a process in the same session, a crafted config file),
- steps or a proof of concept to reproduce it.

liwin is maintained in spare time; the maintainer aims to acknowledge reports
within a week. Once the issue is confirmed,
a fix is prepared in a private advisory fork and released as a new 0.x
version, and the advisory is published with credit to you unless you prefer
otherwise.

## Scope

liwin consists of a GNOME Shell extension running inside the compositor and a
daemon and CLI talking to it over the **session** D-Bus. Things of particular
interest:

- ways for another user, or for a process outside the session, to drive the
  daemon or the extension,
- ways for a session process to make the extension do more than the
  documented commands (see [docs/dbus.md](docs/dbus.md)),
- crashes of GNOME Shell triggered through liwin.

Note that any process in your session can already talk to your session bus;
that liwin's D-Bus API is reachable from the same session is by design.
