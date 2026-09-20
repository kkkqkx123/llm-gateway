# AGENTS.md

**Language**
Always use English in code files(include config files, comments) and use Simplified Chinese in docs.

**Plan/Design Document**
Avoid including complete code snippets. Mainly using concise natural language descriptions.

**Security Assurance**
Always avoid the use of unwrap. In testing, substitute with expect.
Refrain from using unsafe methods except where directly involving low-level operations.
All instances of unsafe usage must be explicitly documented in the unsafe.md file within the docs\archive directory.

**Type Design Guidelines**
Minimise the use of dynamic dispatch forms such as `dyn`, always prioritising deterministic types.
All instances of dynamic dispatch must be explicitly documented in the `dynamic.md` file within the `docs\archive` directory.

## Project Overview

Single Rust crate (`llm-gateway`). A multi-provider AI API gateway that translates between OpenAI, Claude, Gemini, Codex, and Antigravity formats. Reference implementation in `ref/CLIProxyAPI-7.1.19/` (Go).

## Build & Test Commands

```sh
cargo build
cargo test          # all tests
cargo test --lib    # lib tests only
cargo clippy        # linting
cargo fmt --check   # format check
```

No workspace, no task runner, no pre-commit hooks. Run all above before committing.

## Module Architecture

- `src/lib.rs` — public library root; re-exports all modules via `pub use`.
- `src/main.rs` — standalone demo binary (uses `cli_proxy_api` crate name internally, not `llm-gateway`).
- `src/format/` — format registry, pipeline, and traits. `Format` enum: OpenAI, OpenAIResponse, Claude, Gemini, GeminiCLI, Codex, Antigravity.
- `src/converters/` — request/response transformers (9 request + 18 response + 2 cross-format). Auto-registered via `register_default_transformers()`.
- `src/scheduler/` — min-heap based refresh scheduler with concurrent workers.
- `src/config/` — YAML config types and loading.
- `src/thinking/` — thinking parameter extraction, suffix parsing, and application.
- `src/logging/` — tracing-based structured logging (file + streaming + middleware).
- `src/watcher.rs` — notify-based config file hot-reload with debounce.
- `src/registry.rs` — model registry with predefined models and remote update.
- `src/cache.rs` — LRU cache with TTL.

## Testing Conventions

Tests are inline `#[cfg(test)] mod tests` within each `.rs` file (~30 test modules). Use both `#[test]` and `#[tokio::test]`. Use `tempfile` for file-based tests. Do not add integration test files under `tests/`.

## Key Patterns

- All traits use `Send + Sync` bounds.
- Transformers registered at startup (no lazy static).
- `Format::from_str` panics on unknown format — match exhaustively when adding new variants.
- Scheduler uses `tokio::sync::broadcast` for job dispatch.

## Adding a New Format

1. Add variant to `Format` enum in `src/format/types.rs`.
2. Implement `RequestTransformer`, `StreamResponseTransformer`, `NonStreamResponseTransformer` traits.
3. Register in the registration function (called from `register_default_transformers`).