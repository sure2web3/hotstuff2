# Contributing to hotstuff2

Thank you for your interest in contributing to the hotstuff2 project! We welcome contributions of all kinds, including bug reports, feature requests, documentation improvements, and code contributions.

## How to Contribute

### 1. Fork the Repository

Click the "Fork" button at the top right of the [GitHub repository](https://github.com/sure2web3/hotstuff2) page to create your own copy.

### 2. Clone Your Fork

```sh
git clone https://github.com/your-username/hotstuff2.git
cd hotstuff2
```

### 3. Create a Branch

Create a new branch for your feature or bugfix:

```sh
git checkout -b my-feature
```

### 4. Make Your Changes

- Follow Rust best practices and keep code modular.
- Write clear commit messages.
- Add or update tests as appropriate.
- Run `cargo fmt` to format your code.
- Run `cargo test` to ensure all tests pass.

### 5. Push and Create a Pull Request

```sh
git push origin my-feature
```

Go to your fork on GitHub and open a Pull Request (PR) to the `main` branch of the upstream repository. Please describe your changes and reference any related issues.

## Code Style

- Use [rustfmt](https://github.com/rust-lang/rustfmt) for formatting.
- Use [clippy](https://github.com/rust-lang/rust-clippy) for linting:  
  ```sh
  cargo clippy --all-targets --all-features -- -D warnings
  ```
- Write documentation for public items.

## Reporting Issues

If you find a bug or have a feature request, please [open an issue](https://github.com/sure2web3/hotstuff2/issues) and provide as much detail as possible.

## Code of Conduct

Please be respectful and considerate in all interactions. See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) if available.

## License

By contributing, you agree that your contributions will be licensed under the MIT License.

---

Thank you for helping make hotstuff2 better!
