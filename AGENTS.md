# AGENTS.md

## Project purpose

`yt-dlp-4me-rusty` is a Rust CLI that mirrors the Markdown queue behavior of the Python project [`yt-dlp-4me`](https://github.com/starichkov/yt-dlp-4me). It manages queue state in place and delegates actual downloads to an external `yt-dlp` executable.

## Repository layout

- `Cargo.toml` — package metadata and dependencies.
- `Cargo.lock` — reproducible dependency lockfile.
- `src/lib.rs` — public queue parsing, cleanup, movement, download, and processing logic.
- `src/main.rs` — argument handling, directory creation, cleanup, processing, and summary output.
- `tests/queue.rs` — integration tests for Markdown behavior.
- `.github/workflows/rust.yml` — formatting, compilation, tests, Clippy, and Rustdoc CI.
- `README.md` — user-facing installation and usage documentation.
- `LICENSE.md` — MIT license and Vadim Starichkov / TemplateTasks attribution.

## Build and validation

Use the stable Rust toolchain and run commands from the repository root:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo doc --no-deps
```

Run `cargo fmt --all` only when intentionally changing formatting. Keep tests in `tests/` unless a test specifically needs private implementation details.

## CLI contract

```text
yt-dlp-4me-rusty <queue_file.md> <output_directory> [--verify|--retry] [--yt-dlp-path PATH]
```

- Normal mode processes `Not downloaded yet`.
- `--verify` processes `Downloaded`.
- `--retry` processes `Failed`.
- `--yt-dlp-path PATH` selects a custom downloader; the default is `yt-dlp` on `PATH`.
- Input paths and output paths support a leading `~/`.
- The output directory is created when needed.
- Missing input exits with status `1`; missing required arguments exits with status `2`.

Do not replace the external process with a fake downloader in production code. Tests should avoid network access and can test queue behavior without invoking `yt-dlp`.

## Markdown invariants

The queue is persistent state, so edits must preserve unrelated Markdown content and section order. Recognized section names are `Downloaded`, `Failed`, and `Not downloaded yet`, case-insensitively, under heading levels one or two.

When changing queue logic, preserve these rules:

1. Extract only the first HTTP or HTTPS URL from a line.
2. Remove duplicate links before processing, preferring `Downloaded`, then `Failed`, then pending.
3. Move a processed link out of its source section exactly once.
4. Create a missing target section without swallowing neighboring content.
5. Preserve non-link lines and unrelated sections.
6. Keep a final newline when rewriting a non-empty queue.

## Change guidance

- Prefer the standard library unless a dependency is clearly necessary.
- Keep public API documentation in Rustdoc comments (`///`) for new public items.
- Add or update integration tests for observable queue behavior and edge cases.
- Do not put tests into `src/main.rs`; use `tests/queue.rs` for end-to-end public API coverage.
- Do not commit generated `target/` artifacts.
- Do not add network-dependent tests to CI.
- Update `README.md` when CLI flags or queue semantics change.
- Keep `Cargo.lock` in sync if dependencies are added or changed.

## CI expectations

GitHub Actions runs on pushes and pull requests using `ubuntu-latest` and stable Rust. A change is ready when formatting, compilation, tests, Clippy with warnings denied, and Rustdoc all pass.