# Contributing to Term

Thank you for considering contributing to Term! We welcome bug reports, feature
requests, and pull requests from the community.

## Getting Started

1. Fork the repository on GitHub.
2. Clone your fork: `git clone https://github.com/<your-username>/term.git`
3. Create a feature branch: `git checkout -b feature/your-feature-name`
4. Install Rust: https://rustup.rs/
5. Build: `cargo build`
6. Run tests: `cargo test`
7. Run clippy: `cargo clippy -- -D warnings`
8. Run fmt: `cargo fmt --check`

## Making Changes

- Keep changes focused — one feature or bug fix per pull request.
- Update `CHANGELOG.md` with your changes under an "Unreleased" section.
- Add or update tests for any new functionality.
- Run `cargo test`, `cargo clippy -- -D warnings`, and `cargo fmt` before
  submitting.
- Write clear, conventional commit messages (e.g. `feat: ...`, `fix: ...`,
  `docs: ...`, `chore: ...`).

## Pull Request Process

1. Push your branch to your fork.
2. Open a pull request against `main` on `GrayCodeAI/term`.
3. A maintainer will review your changes and may request changes.
4. Once approved, a maintainer will merge your pull request.

## Code Style

- Rust edition 2024.
- Follow default `rustfmt` settings (see `.rustfmt.toml`).
- Address all clippy warnings.
- Keep public APIs documented with doc comments.

## Reporting Bugs

Open an issue at https://github.com/GrayCodeAI/term/issues with:

- A clear title and description
- Steps to reproduce
- Expected vs. actual behavior
- Your Rust version (`rustc --version`) and OS
- Any relevant logs or error output

## License

By contributing, you agree that your contributions will be licensed under the
MIT License (see `LICENSE`).
