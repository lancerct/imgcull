# imgcull

> AI-assisted image review and metadata tooling for Lightroom Classic. See the [current development status](docs/project-status.md) for the personal culling workflow and its remaining integration gates.

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/henrif75/imgcull#license)
[![Build Status](https://img.shields.io/github/actions/workflow/status/lancerct/imgcull/rust.yml?branch=main)](https://github.com/lancerct/imgcull/actions/workflows/rust.yml)
[Development status and integration history](docs/project-status.md)

![imgcull — AI-powered image culling](docs/banner.svg)

---

## 🔍 Why imgcull?

### Personal culling development in this fork

The user-facing photographic policy is now [Culling standard v1.0](docs/culling-standard-v1.0.md), adopted 2026-10-25: 1 star for deletion advice, 3 for retention, 4/5 for post-processing priorities, with ungraded uncertainty. It supersedes older color-label/art-score criteria. Runtime policy v2 has not yet been adapted; do not auto-migrate existing marks or apply experiment predictions.


The implemented foundations from PRs #3–#7 are merged into `main`. See [current status and archived experiments](docs/project-status.md). The complete automatic personal workflow remains under development.

Versioned personal corrections and comparison tools are documented in [calibration and comparison](docs/calibration-and-comparison.md): good near duplicates can receive separate redundancy advice, preferences/counterexamples are immutable and frozen per job, similarity only proposes pairs, and draft reports show the explicit keeper relation. These tools do not yet run automatic calibrated visual review or authorize markings. The example corpus tests decision contracts, not real-photo model accuracy.

A thin Lightroom current-effect trial exporter is available in `lightroom/ImgCull.lrplugin`; see the [installation, trial and verification contract](docs/lightroom-export.md). The initial Lua export test requires no Rust installation. It does not yet bind renditions to review jobs or authorize metadata changes. The installer commands further below fetch upstream releases and do not include this fork's personal culling additions in `main`.

The native evidence interface is implemented as `imgcull prepare` and `imgcull verify-evidence`. It fully decodes sources and creates a hashed overview plus explicitly requested native PNG details. It does not perform visual review, make final selections, or update XMP. Resumable draft review commands freeze all companion/XMP revisions, save typed observations and validate conservative group constraints. They do not call a model or write XMP. Automatic subject localization, validated provider inspection, binding of Lightroom current renders to reviews, plan/apply integration and skill packaging remain in development.

See the [architecture and command contract](docs/culling-architecture.md) for module ownership, source-resolution requirements, compact output, reuse and current limitations. The legacy score/describe commands below use overview model inputs and do not satisfy the personal final-focus review contract.

After a shoot, photographers face hundreds (or thousands) of raw files to review. imgcull's legacy commands ask vision LLMs for overview scores/descriptions and store results in XMP sidecars, preserving existing human ratings and captions. Lightroom may require **Read Metadata from File** to display changes; independently imported JPEG metadata synchronization is not implemented in this fork yet.

## ✨ Features

- **Multi-provider support** — Claude (Anthropic), OpenAI GPT-5, Gemini, and local models via Ollama
- **XMP-native output** — scores and descriptions are written to `.xmp` sidecar files; existing Lightroom metadata (collections, color labels, crops) is preserved
- **Non-destructive** — original image pixels are unchanged; XMP updates always record independent recovery snapshots, with an optional first-version `.xmp.bak`
- **Five scoring dimensions** — sharpness, exposure, composition, subject clarity, aesthetics, averaged into an overall 0.00–1.00 score mapped to a 1–5 star rating
- **Parallel processing** — bounded concurrency with configurable worker count for fast batch runs
- **Dry-run mode** — preview which files would be processed without making any LLM calls or file writes
- **LLM-generated keywords** — photography keywords written to `dc:subject` in XMP sidecars, appearing automatically as Lightroom keywords
- **Separate describe / score commands** — run description-only, scoring-only, or both in one pass

## 🛠 Tech Stack

Built with **Rust** (edition 2024), **Tokio** (async runtime), **Rig** (LLM provider abstraction), and **clap** (CLI).

---

## 📦 Installation

### Prerequisites

- An API key for at least one supported provider, **or** a running [Ollama](https://ollama.ai) instance for local inference
- (Only if building from source) Rust `>=` 1.89 (edition 2024; dependencies may require a newer toolchain, CI uses stable)

### Install from the GitHub release (recommended)

Pre-built binaries are published for macOS (Apple Silicon and Intel), Linux (x86_64 and aarch64), and Windows (x86_64) on every tagged release. Pick the one-liner for your platform:

```bash
# macOS / Linux
curl -LsSf https://github.com/henrif75/imgcull/releases/latest/download/imgcull-installer.sh | sh

# Windows PowerShell
powershell -c "irm https://github.com/henrif75/imgcull/releases/latest/download/imgcull-installer.ps1 | iex"
```

The installer drops `imgcull` into a Cargo-style install location (`~/.cargo/bin` on Unix, `%USERPROFILE%\.cargo\bin` on Windows) and prints any PATH changes you need to make. SHA-256 checksums are published alongside each artifact.

Prefer downloading the tarball yourself? Grab the right archive for your platform from the [latest release page](https://github.com/henrif75/imgcull/releases/latest) and extract the `imgcull` binary onto your `PATH`.

### Build from source

```bash
git clone https://github.com/henrif75/imgcull.git
cd imgcull
cargo build --release
# Binary is at ./target/release/imgcull
```

### First-time setup

```bash
imgcull init
```

This creates `config.toml`, `prompts.toml`, and `.env.example` in your platform's config directory:

| Platform | Config directory |
|----------|-----------------|
| macOS | `~/Library/Application Support/imgcull/` |
| Linux | `~/.config/imgcull/` |
| Windows | `%APPDATA%\imgcull\` |

Copy `.env.example` to `.env` and fill in your API key:

```bash
# macOS
cp ~/Library/"Application Support"/imgcull/.env.example ~/Library/"Application Support"/imgcull/.env

# Linux
cp ~/.config/imgcull/.env.example ~/.config/imgcull/.env
```

---

## 🚀 Usage

### Score and describe a folder of photos

```bash
imgcull score ~/Photos/2026-03-shoot/
```

This processes every supported image (JPEG and RAW formats: ARW, CR2, CR3, DNG, ERF, MEF, MOS, MRW, NEF, NRW, ORF, PEF, RAF, RW2, SR2, SRW), writes a scene description to `dc:description` and a 1–5 star rating to `xmp:Rating` in each `.xmp` sidecar.

### Generate descriptions only

```bash
imgcull describe ~/Photos/2026-03-shoot/
```

### Score only (no description)

```bash
imgcull score --no-description ~/Photos/2026-03-shoot/
```

### Use a specific provider

```bash
imgcull score --provider openai ~/Photos/2026-03-shoot/
imgcull score --description-provider claude --scoring-provider ollama ~/Photos/
```

### Preview without making any LLM calls

```bash
imgcull score --dry-run ~/Photos/2026-03-shoot/
```

### View a report of scored images

```bash
imgcull report ~/Photos/2026-03-shoot/
```

This reads all `.xmp` sidecar files in the directory and displays a summary table with scores, ratings, keywords, and model info. It also picks up standalone `.xmp` files (e.g., from different model runs).

### Export results to CSV

```bash
imgcull report --format csv ~/Photos/2026-03-shoot/ > results.csv
```

### Advanced options

| Flag | Description |
|------|-------------|
| `--provider NAME` | Override both description and scoring provider |
| `--description-provider NAME` | Override description provider only |
| `--scoring-provider NAME` | Override scoring provider only |
| `--concurrency N` | Max parallel LLM requests (default: 4) |
| `--backup` | Back up existing `.xmp` to `.xmp.bak` before modifying |
| `--force` | Re-process images that already have scores/descriptions |
| `--no-rating` | Skip writing star rating to `xmp:Rating` |
| `--dry-run` | Show what would be processed; no LLM calls, no writes |
| `--log PATH` | Write detailed log to file |
| `--verbose` / `-v` | Verbose terminal output |
| `--quiet` / `-q` | Only show errors |

### Configuration

imgcull reads `config.toml` from the config directory shown above. Example:

```toml
[default]
description_provider = "claude"
scoring_provider     = "claude"
concurrency          = 8

[providers.claude]
model       = "claude-sonnet-4-6"
api_key_env = "ANTHROPIC_API_KEY"

[providers.ollama]
model    = "llava"
base_url = "http://localhost:11434"
```

Scoring dimensions are fixed (sharpness, exposure, composition, subject clarity, aesthetics). Custom per-dimension guidelines can still be edited in `prompts.toml` in the same config directory.

A fully-annotated example is in [`docs/examples/config.toml`](docs/examples/config.toml).

---

## 📋 Release Notes

For the full changelog, see [GitHub Releases](https://github.com/henrif75/imgcull/releases).

### v0.2.5

- Verified default model IDs against each provider's current documentation and corrected `claude-sonnet-4-6`, `gemini-2.5-pro`. Dropped DeepSeek from the default providers because its public API has no vision-capable model; the Rig backend is still wired up for users who add `[providers.deepseek]` manually.
- Hardened the JSON extractor against `{` / `}` characters inside string values (e.g. a critique containing braces).

### v0.2.4

- Locked scoring dimensions to the fixed five. Removed the `--dimensions` CLI flag and the `[scoring]` config section, which previously only accepted subsets of the hardcoded dimensions and silently dropped anything else.

### v0.2.3

- Pre-build the Rig `Agent` per provider so the underlying HTTP client and connection pool are reused across every LLM call, instead of being rebuilt per image.

### v0.2.2

- Fix XMP sidecars being rewritten on every run even when nothing had changed.
- Several internal cleanups: dead-code removal, fewer allocations on the hot path, simpler helpers.

### v0.2.1

- **Improved RAW support** — replaced manual JPEG-preview extraction with [`rawler`](https://crates.io/crates/rawler), adding support for CR3, RAF, RW2, PEF, and more formats while delivering higher-quality previews for existing formats.

### v0.2.0

- **Report command** — `imgcull report` reads XMP sidecars and displays a summary table or CSV with scores, ratings, keywords, and model info. Discovers both image-paired and standalone `.xmp` files, making it easy to compare results across different LLM providers.
- **Keywords** — the scoring prompt now requests 5-15 descriptive photography keywords (genre, subject, mood, lighting, technique, location type). Keywords are written to `dc:subject` in XMP sidecars and appear automatically as keywords in Adobe Lightroom Classic.

### v0.1.0

- Initial release with multi-provider LLM scoring, XMP sidecar output, and parallel processing.

---

## 🤝 Contributing

Contributions are welcome. Please open an issue first to discuss what you'd like to change.

- Bug reports and feature requests → [GitHub Issues](https://github.com/henrif75/imgcull/issues)
- Questions → [GitHub Discussions](https://github.com/henrif75/imgcull/discussions)

When contributing code, run the full check suite before submitting a PR:

```bash
cargo fmt && cargo clippy -- -D warnings && cargo test
```

---

## 📄 License

Distributed under the **MIT OR Apache-2.0** license. See [`LICENSE-MIT`](LICENSE-MIT) and [`LICENSE-APACHE`](LICENSE-APACHE) for details.
