# EdgeDoc — developer entry points.
#
# Every target prints one line per action so a reader can see what happened.
# `make` with no target prints the help.

.DEFAULT_GOAL := help
SHELL := /bin/bash
CARGO ?= cargo
OUT ?= target/examples
PACKAGE := edgedoc

# Colours, disabled when the output is not a terminal.
BOLD := $(shell tput bold 2>/dev/null)
DIM := $(shell tput dim 2>/dev/null)
GREEN := $(shell tput setaf 2 2>/dev/null)
BLUE := $(shell tput setaf 4 2>/dev/null)
RESET := $(shell tput sgr0 2>/dev/null)

.PHONY: help
help: ## Show this help
	@printf '%sEdgeDoc%s — a Rust library for Word documents\n\n' '$(BOLD)' '$(RESET)'
	@printf '%sUsage%s  make <target>\n\n' '$(BOLD)' '$(RESET)'
	@awk 'BEGIN { FS = ":.*?## " } \
		/^[a-zA-Z0-9_-]+:.*?## / { printf "  %s%-18s%s %s\n", "$(GREEN)", $$1, "$(RESET)", $$2 } \
		/^##@/ { printf "\n%s%s%s\n", "$(BLUE)", substr($$0, 5), "$(RESET)" }' $(MAKEFILE_LIST)
	@printf '\n%sOutput of the examples goes to %s%s%s\n' '$(DIM)' '$(BOLD)' '$(OUT)' '$(RESET)'

##@ Everyday

.PHONY: check
check: fmt-check lint test ## Run every check that CI runs
	@printf '%s✓ all checks passed%s\n' '$(GREEN)' '$(RESET)'

.PHONY: test
test: ## Run every test
	@printf '%s→%s cargo test\n' '$(BLUE)' '$(RESET)'
	@$(CARGO) test --workspace --all-targets

.PHONY: test-quiet
test-quiet: ## Run every test and print only the summary
	@$(CARGO) test --workspace --all-targets --quiet 2>&1 | grep -E 'test result|panicked|error' || true

.PHONY: lint
lint: ## Run clippy with warnings denied
	@printf '%s→%s cargo clippy\n' '$(BLUE)' '$(RESET)'
	@$(CARGO) clippy --workspace --all-targets -- -D warnings

.PHONY: fmt
fmt: ## Format the code
	@printf '%s→%s cargo fmt\n' '$(BLUE)' '$(RESET)'
	@$(CARGO) fmt --all

.PHONY: fmt-check
fmt-check: ## Check formatting without writing
	@printf '%s→%s cargo fmt --check\n' '$(BLUE)' '$(RESET)'
	@$(CARGO) fmt --all -- --check

##@ Documents

.PHONY: examples
examples: ## Build the example documents into target/examples
	@printf '%s→%s building the example documents\n' '$(BLUE)' '$(RESET)'
	@$(CARGO) run --quiet -p edgedoc-demos --bin write-demos -- $(OUT)

.PHONY: open
open: examples ## Build the example documents and open the output folder
	@if command -v open >/dev/null 2>&1; then open $(OUT); \
	elif command -v xdg-open >/dev/null 2>&1; then xdg-open $(OUT); \
	else printf '%sOpen %s yourself.%s\n' '$(DIM)' '$(OUT)' '$(RESET)'; fi

.PHONY: assets
assets: ## Redraw the PNG assets that the examples embed
	@printf '%s→%s redrawing the assets\n' '$(BLUE)' '$(RESET)'
	@python3 tools/make_assets.py

.PHONY: verify
verify: examples schema ## Check every document with WordCraft and the ECMA-376 schema
	@printf '%s→%s verifying with WordCraft\n' '$(BLUE)' '$(RESET)'
	@python3 tools/verify_with_wordcraft.py

.PHONY: schema
schema: examples ## Validate every document against the ECMA-376 schema
	@printf '%s→%s validating against the ECMA-376 schema\n' '$(BLUE)' '$(RESET)'
	@python3 tools/validate_ooxml.py

.PHONY: clean-examples
clean-examples: ## Remove the generated example documents
	@rm -rf $(OUT)
	@printf '%s✓ removed %s%s\n' '$(GREEN)' '$(OUT)' '$(RESET)'

##@ Documentation

.PHONY: doc
doc: ## Build the API documentation
	@printf '%s→%s cargo doc\n' '$(BLUE)' '$(RESET)'
	@RUSTDOCFLAGS="-D warnings" $(CARGO) doc --workspace --no-deps

.PHONY: doc-open
doc-open: doc ## Build the API documentation and open it
	@$(CARGO) doc --workspace --no-deps --open

.PHONY: docs-lint
docs-lint: ## Check the prose for Simplified Technical English errors
	@printf '%s→%s checking the documentation\n' '$(BLUE)' '$(RESET)'
	@python3 tools/ste_lint.py docs/ README.md || true

##@ Examples

.PHONY: example-python
example-python: examples ## Run the Python examples (needs maturin and a virtualenv)
	@printf '%s→%s running the Python examples\n' '$(BLUE)' '$(RESET)'
	@python3 -c "import maturin" 2>/dev/null || printf '%sInstall maturin in a virtualenv: python3 -m venv .venv && .venv/bin/pip install maturin pytest%s\n' '$(DIM)' '$(RESET)'
	@VIRTUAL_ENV=$${VIRTUAL_ENV:-.venv} maturin develop --manifest-path crates/edgedoc-py/Cargo.toml --features python
	@$${VIRTUAL_ENV:-.venv}/bin/python examples/python/basic.py

.PHONY: test-python
test-python: examples ## Run the Python test suite
	@printf '%s→%s pytest examples/python\n' '$(BLUE)' '$(RESET)'
	@$${VIRTUAL_ENV:-.venv}/bin/python -m pytest examples/python -q

.PHONY: example-wasm
example-wasm: examples ## Run the WebAssembly examples in Node
	@printf '%s→%s running the WebAssembly examples\n' '$(BLUE)' '$(RESET)'
	@node examples/wasm/run.mjs

##@ Display

.PHONY: display-test
display-test: ## Run the display library tests
	@printf '%s→%s cargo test -p edgedoc-display -p edgedoc-display-wasm\n' '$(BLUE)' '$(RESET)'
	@$(CARGO) test -p edgedoc-display -p edgedoc-display-wasm

.PHONY: display-wasm
display-wasm: ## Build the display module and its glue into site/display
	@printf '%s→%s building the display module\n' '$(BLUE)' '$(RESET)'
	@$(CARGO) build --target wasm32-unknown-unknown -p edgedoc-display-wasm --features wasm --release
	@wasm-bindgen --target web --out-dir site/display --out-name edgedoc_display --no-typescript \
		target/wasm32-unknown-unknown/release/edgedoc_display_wasm.wasm
	@printf '%s✓%s site/display holds the module\n' '$(GREEN)' '$(RESET)'

.PHONY: display-e2e
display-e2e: display-wasm ## Run the display end-to-end test and write docs/display/e2e
	@printf '%s→%s node tools/display_e2e.mjs\n' '$(BLUE)' '$(RESET)'
	@node tools/display_e2e.mjs

.PHONY: display-serve
display-serve: ## Serve the demo site on http://127.0.0.1:8900
	@printf '%s→%s python3 -m http.server 8900 --directory site\n' '$(BLUE)' '$(RESET)'
	@python3 -m http.server 8900 --directory site

.PHONY: display-clean
display-clean: ## Remove the generated display module
	@rm -rf site/display/edgedoc_display.js site/display/edgedoc_display_bg.wasm
	@printf '%s✓ removed the generated module%s\n' '$(GREEN)' '$(RESET)'

##@ Corpus

.PHONY: corpus
corpus: ## Download the Word test corpus into tests/corpus/downloaded
	@printf '%s→%s tools/fetch_corpus.sh\n' '$(BLUE)' '$(RESET)'
	@tools/fetch_corpus.sh

.PHONY: corpus-report
corpus-report: corpus ## Lay out every corpus document and print one line each
	@$(CARGO) run -q -p edgedoc-display --example corpus -- tests/corpus/downloaded

.PHONY: corpus-e2e
corpus-e2e: display-wasm corpus ## Run the browser test over the corpus
	@printf '%s→%s node tools/display_corpus_e2e.mjs\n' '$(BLUE)' '$(RESET)'
	@node tools/display_corpus_e2e.mjs

.PHONY: corpus-external
corpus-external: ## Download and verify 75 commit-pinned POI/docx4j DOCX documents
	@python3 tools/fetch_external_corpus.py

.PHONY: corpus-external-expanded
corpus-external-expanded: ## Download 181 additional pinned upstream fixtures
	@python3 tools/fetch_external_corpus.py --manifest tests/corpus/external-expanded-manifest.json --directory target/external-docx-expanded

.PHONY: corpus-external-check
corpus-external-check: ## Verify downloaded DOCX integrity and exercise bounded native layout
	@python3 tools/fetch_external_corpus.py --verify-only
	@$(CARGO) test -p edgedoc-display --features serde --test external_corpus -- --ignored

##@ Packaging

.PHONY: wasm
wasm: ## Build the WebAssembly module
	@printf '%s→%s cargo build --target wasm32-unknown-unknown\n' '$(BLUE)' '$(RESET)'
	@$(CARGO) build --target wasm32-unknown-unknown -p edgedoc-wasm --features wasm

.PHONY: package-wasm
package-wasm: ## Build the npm package in packages/edgedoc-wasm
	@printf '%s→%s wasm-bindgen into packages/edgedoc-wasm\n' '$(BLUE)' '$(RESET)'
	@$(CARGO) build --release --target wasm32-unknown-unknown -p edgedoc-wasm --features wasm
	@wasm-bindgen --target nodejs --out-dir packages/edgedoc-wasm --out-name edgedoc \
		target/wasm32-unknown-unknown/release/edgedoc_wasm.wasm

.PHONY: python
python: ## Build the Python wheel with maturin
	@printf '%s→%s maturin build\n' '$(BLUE)' '$(RESET)'
	@maturin build --release --out target/wheels

.PHONY: python-dev
python-dev: ## Install the Python package into the current environment
	@printf '%s→%s maturin develop\n' '$(BLUE)' '$(RESET)'
	@maturin develop

##@ Maintenance

.PHONY: clean
clean: clean-examples ## Remove build output
	@printf '%s→%s cargo clean\n' '$(BLUE)' '$(RESET)'
	@$(CARGO) clean

.PHONY: outdated
outdated: ## List dependencies that have a newer version
	@$(CARGO) tree --workspace -e normal --depth 1

.PHONY: audit
audit: ## Check the dependencies for known vulnerabilities
	@printf '%s→%s cargo audit\n' '$(BLUE)' '$(RESET)'
	@$(CARGO) audit
