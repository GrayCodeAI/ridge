# Security Policy

## Reporting a vulnerability

Please report security issues privately via
[GitHub Security Advisories](https://github.com/GrayCodeAI/term/security/advisories/new)
rather than opening a public issue.

## Hardening in this codebase

- **No shell injection**: `git`/`gh` are invoked via `std::process::Command` with
  argument arrays — no intermediate shell.
- **Worktree path validation**: worktree creation rejects `..` traversal and
  empty branch segments.
- **Read-only diff review**: the diff tab never mutates the repository.
- **Modules**: run as local executables you installed yourself from
  `~/.config/term/modules/` — like any script in your PATH, only add ones you trust.

## Audits

Weekly `cargo audit` runs in CI (Security Audit workflow, manual dispatch available).
