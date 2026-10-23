# yt-dlp Markdown Wrapper (Rust)

[![Author](https://img.shields.io/badge/Author-Vadim%20Starichkov-blue?style=for-the-badge)](https://github.com/starichkov)
[![GitHub License](https://img.shields.io/github/license/starichkov/yt-dlp-4me-rusty?style=for-the-badge)](https://github.com/starichkov/yt-dlp-4me-rusty/blob/main/LICENSE.md)
[![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/starichkov/yt-dlp-4me-rusty/rust.yml?style=for-the-badge)](https://github.com/starichkov/yt-dlp-4me-rusty/actions/workflows/rust.yml)
![Rust](https://img.shields.io/badge/Rust-2024-orange?style=for-the-badge)

A Rust command-line wrapper for [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) that manages a video download queue in a Markdown file. It is the Rust counterpart of the original [Python project](https://github.com/starichkov/yt-dlp-4me).

## Features

- Markdown-based queue with `Downloaded`, `Failed`, and `Not downloaded yet` sections.
- Automatic movement of links after successful or failed downloads.
- Duplicate cleanup with downloaded links taking priority over failed and pending links.
- Download, verify, and retry modes.
- Plain URLs, Markdown links, and list items are supported.
- Live `yt-dlp` output and a session summary.
- Relative paths and `~/` path expansion.
- Custom `yt-dlp` executable paths.
- No Rust runtime dependencies beyond the standard library.

## Requirements

- Rust stable toolchain (for building from source).
- A working `yt-dlp` executable on `PATH`, or a path supplied with `--yt-dlp-path`.

Install `yt-dlp` using the [official instructions](https://github.com/yt-dlp/yt-dlp#installation). The wrapper invokes it as an external process and does not provide a downloader itself.

## Installation

```bash
git clone https://github.com/starichkov/yt-dlp-4me-rusty.git
cd yt-dlp-4me-rusty
cargo install --path .
```

Alternatively, build and run locally with `cargo run --`.

## Usage

```bash
yt-dlp-4me-rusty <queue_file.md> <output_directory>
```

For example:

```bash
yt-dlp-4me-rusty videos.md ./downloads
```

### Verify mode

Process links already in `## Downloaded`. `yt-dlp` decides whether a download is still present and whether it needs to be retrieved again.

```bash
yt-dlp-4me-rusty videos.md ./downloads --verify
```

### Retry mode

Retry links currently in `## Failed`:

```bash
yt-dlp-4me-rusty videos.md ./downloads --retry
```

### Custom executable

```bash
yt-dlp-4me-rusty videos.md ~/Videos --yt-dlp-path ~/bin/yt-dlp
```

Use `cargo run --` instead of the installed command when developing:

```bash
cargo run -- videos.md ./downloads
```

## Queue format

The wrapper recognizes these headers case-insensitively at Markdown heading levels one or two:

- `## Not downloaded yet` — links waiting for a normal download run.
- `## Downloaded` — links that succeeded, and links inspected by verify mode.
- `## Failed` — links that failed and are candidates for retry mode.

Other Markdown content is preserved. Links can be plain URLs or labeled Markdown links:

```markdown
# My Video Queue

## Not downloaded yet
- https://www.youtube.com/watch?v=dQw4w9WgXcQ
- [Big Buck Bunny](https://www.youtube.com/watch?v=aqz-KE-bpKQ)

## Downloaded

## Failed
```

Before processing, duplicate links are removed. A link in `Downloaded` wins over the same link in `Failed`, and a link in `Failed` wins over one still pending.

## Development

Run the same checks used by GitHub Actions:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo doc --no-deps
```

The reusable queue implementation is in `src/lib.rs`, the executable entry point is in `src/main.rs`, and integration tests are in `tests/queue.rs`.

## Rustdoc

Generate local API documentation with:

```bash
cargo doc --no-deps --open
```

## Attribution

This project is licensed under the [MIT License](LICENSE.md).

It is based on the behavior of [yt-dlp-4me](https://github.com/starichkov/yt-dlp-4me), the original Python implementation by Vadim Starichkov and TemplateTasks.

If you use this code in your own projects, attribution is required under the MIT License:

```text
Based on yt-dlp-4me by Vadim Starichkov, TemplateTasks

https://github.com/starichkov/yt-dlp-4me
```

Copyright © 2026 Vadim Starichkov, TemplateTasks.