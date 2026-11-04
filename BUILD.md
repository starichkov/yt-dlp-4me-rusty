# Building and developing

This project uses Cargo and the stable Rust toolchain.

## Prerequisites

- Install the stable Rust toolchain using [rustup](https://rustup.rs/).
- Ensure `cargo` is available on your `PATH`.
- Install [`yt-dlp`](https://github.com/yt-dlp/yt-dlp#installation) if you
  want to run the wrapper against real downloads.

## Build profiles

Cargo uses its faster, unoptimized development profile by default:

```bash
cargo build
```

This places the debug executable at `target/debug/yt-dlp-4me-rusty`. For a
smaller and faster executable intended for regular use, build with the
optimized release profile:

```bash
cargo build --release
```

The release executable is written to `target/release/yt-dlp-4me-rusty`. You
can also run it directly through Cargo:

```bash
cargo run --release -- videos.md ./downloads
```

The `cargo install --path .` command builds and installs the release version
into Cargo's binary directory, making it convenient to use outside the
project directory:

```bash
cargo install --path .
```

## Validation

Run the same checks used by GitHub Actions:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo doc --no-deps
```

To generate an LCOV coverage report locally, install `cargo-llvm-cov` and run:

```bash
cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info
```

GitHub Actions uploads this report to Codecov after the coverage job completes.

To generate and open local API documentation:

```bash
cargo doc --no-deps --open
```

## Project layout

- `src/lib.rs` — reusable queue processing and download logic.
- `src/file.rs` — queue file abstraction plus Markdown and plain-text implementations.
- `src/main.rs` — command-line entry point.
- `tests/queue.rs` — integration tests for observable queue behavior.

Queue processing uses the `QueueFile` trait, with `MarkdownFile` and `TextFile`
implementations selected by the input file extension. New file formats can
implement that trait without changing download logic.