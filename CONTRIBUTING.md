# Contributing to EdgeDoc

Thank you for your interest. This page tells you how to set up the project. It also
tells you how to run the checks and what a change must include.

## Setup

You need Rust 1.76 or later. The tests run offline after the first build.

```bash
make check
```

`make check` runs the formatter check, clippy with warnings denied, and every test.
Run `make help` to see the other targets.

## Before you open a pull request

1. Run `make fmt` to format the code.
2. Run `make check`. It must pass with no warnings.
3. If you change the model or the writer, add a case to
   `crates/edgedoc/tests/word_features.rs` and a row to `docs/12-word-features.md`.
   The test fails when the two disagree.
4. If you change a supported Word feature, update `docs/11-format-coverage.md`.
5. Write documentation in Simplified Technical English. `make docs-lint` checks the
   mechanical rules.

## Code rules

- The workspace forbids `unsafe` code.
- Tests return `Result<(), String>`. They do not use `unwrap` or `expect`, because
  the workspace denies those methods.
- A layer calls only the layer below it. `docs/03-architecture.md` holds the rule,
  and `layer_dependencies_match_the_plan` checks it.
- One catalog, one field parser, one budget, one write path. Do not add a second.

## Commit messages

Write the subject in the imperative mood and keep it under 72 characters. Put the
reason in the body. Say what changed for a reader of the document, not which file
you edited.

## Licence

By contributing, you agree that your contribution is licensed under the Apache
License 2.0. See [LICENSE](LICENSE).
