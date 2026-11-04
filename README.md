# yt-dlp Markdown Wrapper (Rust)

[![Author](https://img.shields.io/badge/Author-Vadim%20Starichkov-blue?style=for-the-badge)](https://github.com/starichkov)
[![GitHub License](https://img.shields.io/github/license/starichkov/yt-dlp-4me-rusty?style=for-the-badge)](https://github.com/starichkov/yt-dlp-4me-rusty/blob/main/LICENSE.md)
[![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/starichkov/yt-dlp-4me-rusty/rust.yml?style=for-the-badge)](https://github.com/starichkov/yt-dlp-4me-rusty/actions/workflows/rust.yml)
[![Codecov](https://img.shields.io/codecov/c/github/starichkov/yt-dlp-4me-rusty?style=for-the-badge)](https://codecov.io/gh/starichkov/yt-dlp-4me-rusty)
![Rust](https://img.shields.io/badge/Rust-2024-orange?style=for-the-badge)

A Rust command-line wrapper for [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) that manages a video download queue in Markdown or plain text. It is the Rust counterpart of the original [Python project](https://github.com/starichkov/yt-dlp-4me).

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

For Rust build profiles, release binaries, and development checks, see
[`BUILD.md`](BUILD.md).

## Usage

```bash
yt-dlp-4me-rusty <queue_file.md|queue_file.txt> <output_directory>
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

## Queue formats

### Markdown

Files with a `.md` extension use a stateful Markdown queue. The wrapper recognizes these headers case-insensitively at Markdown heading levels one or two:

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

### Plain text

Files with a `.txt` extension use a simple URL inbox. Put one URL per line; non-URL lines are preserved and the first HTTP or HTTPS URL on each line is processed:

```text
https://www.youtube.com/watch?v=dQw4w9WgXcQ
https://youtu.be/aqz-KE-bpKQ
```

Duplicate URLs are removed before processing. Plain-text queues use companion files beside the inbox: `queue.failed.txt` stores failed URLs and `queue.done.txt` stores successful URLs. Normal mode reads `queue.txt`, `--retry` reads `queue.failed.txt`, and `--verify` reads `queue.done.txt`; successful retries move to the done file, while failed retries remain in the failed file. Non-URL lines remain in their current file.

## Attribution

This project is licensed under the [MIT License](LICENSE.md).

It is based on the behavior of [yt-dlp-4me](https://github.com/starichkov/yt-dlp-4me), the original Python implementation by Vadim Starichkov and TemplateTasks.

If you use this code in your own projects, attribution is required under the MIT License:

```text
Based on yt-dlp-4me by Vadim Starichkov, TemplateTasks

https://github.com/starichkov/yt-dlp-4me-rusty
```

Copyright © 2026 Vadim Starichkov, TemplateTasks.