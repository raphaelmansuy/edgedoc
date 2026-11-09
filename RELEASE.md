# Releasing EdgeDoc

Use this checklist when cutting a version tag. The workflow in
[`.github/workflows/release.yml`](.github/workflows/release.yml) runs on `v*` tags.

## Version alignment

These must match before you tag:

| File | Field |
| --- | --- |
| `Cargo.toml` | `[workspace.package] version` and `workspace.dependencies` |
| `pyproject.toml` | `version` |
| `python/edgedoc/__init__.py` | `__version__` |
| `packages/edgedoc-wasm/package.json` | `version` |
| `CHANGELOG.md` | New `## [x.y.z] - date` section |

After changing the workspace version, run `cargo build` so `Cargo.lock` records the bump.

## Pre-flight (local)

```bash
make check
make display-wasm
make display-e2e
make word-e2e
make package-wasm
```

`make word-e2e` should report **41/41** checks. Screenshots live under
`specs/001-word-control/e2e/screenshots/`.

## Tag and publish

1. Commit the version and changelog updates (only when the maintainer asks).
2. Create an annotated tag: `git tag -a v0.2.0 -m "v0.2.0"`
3. Push the tag: `git push origin v0.2.0`

The release workflow then:

- Publishes the **seven** core crates on crates.io (`edgedoc` through `edgedoc-merge`).
- Builds and uploads the **PyPI** wheel.
- Builds and publishes **`edgedoc-wasm`** to npm.
- Deploys **GitHub Pages** from `site/` (run `make display-wasm` in CI so `site/display/` is current).

`edgedoc-display` and `edgedoc-display-wasm` ship in the repository and on Pages; they are
not published to crates.io in the current workflow.

## Site demos

| URL path | Purpose |
| --- | --- |
| `/display/` | Read-only paged viewer |
| `/word/` | Word-like editor for merge templates |

Local preview: `make display-serve` → http://127.0.0.1:8900/word/

## v0.2.0 highlights

- Browser Word control (`specs/001-word-control/`, `make word-e2e`).
- Display stack, Markdown round-trip, and corpus work documented in `CHANGELOG.md`
  under `[0.2.0]` and `[Unreleased]`.
