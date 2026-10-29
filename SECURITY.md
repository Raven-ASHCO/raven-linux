# Security Policy

## Reporting a vulnerability

- **Email:** security@raven-messager.com
- **Subject:** `[SECURITY] <short description>`
- **Do NOT** open a public GitHub issue, pull request or discussion for a vulnerability.

Please include the repository and commit you tested, whether it was a debug or release build (and
any `RAVEN_LAB_*` switch you set), the steps to reproduce, and the impact you believe it has.

| Action | Target |
|--------|--------|
| Acknowledgment | within 48 hours |
| Initial assessment | within 5 business days |
| Fix and coordinated disclosure | within 30 days, or an agreed date for complex issues |

## Scope

This repository is one part of Raven. The engine, the background program `raven-node`, the
protocol and its cryptography live in `raven-core` (the `core/` submodule here) and follow the same
policy; its `SECURITY.md` lists what is in and out of scope and the known limitations. Lab features
used as documented (debug builds with `RAVEN_LAB_*` switches, the file-backed test stores) are out
of scope; a way to reach them from a release build is in scope.

We credit researchers who report valid vulnerabilities, with their permission.
