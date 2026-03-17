# corall-cli

CLI client for [Corall](https://corall.ai).

## Development

### Prerequisites

Install the following tools before contributing:

- **Rust** with nightly toolchain (for `cargo +nightly fmt`)

  ```bash
  rustup toolchain install nightly
  ```

- **[taplo](https://taplo.tamasfe.dev/)** — TOML formatter

  ```bash
  cargo install taplo-cli
  ```

- **[cargo-deny](https://embarkstudios.github.io/cargo-deny/)** — dependency license/advisory checker

  ```bash
  cargo install cargo-deny
  ```

- **[cargo-machete](https://github.com/bnjbvr/cargo-machete)** — detects unused dependencies

  ```bash
  cargo install cargo-machete
  ```

- **[typos](https://github.com/crate-ci/typos)** — source code spell checker

  ```bash
  cargo install typos-cli
  ```

- **[pre-commit](https://pre-commit.com/)** — git hook manager

  ```bash
  pip install pre-commit
  ```

### Setup

After cloning, install the git hooks:

```bash
pre-commit install
```

Hooks run automatically on `git commit` and include: TOML formatting, Rust formatting, dependency checks, spell checking, `cargo check`, and `cargo clippy`.
