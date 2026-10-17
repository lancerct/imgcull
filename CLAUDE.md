# Contributor instructions

This is one Rust engine. The future skill is a thin coordinator of typed commands.
The architecture source of truth is [docs/culling-architecture.md](docs/culling-architecture.md).
Read it when changing the personal culling workflow; do not copy module ownership or workflow rules into new scripts.

## Contracts

- discovery owns supported extensions, photo keys, sidecar mapping and RAW/JPEG pairing.
- imaging owns full source decode, orientation and native crop coordinates. preprocessing is only an overview adapter.
- evidence owns immutable materials and hashes, not visual verdicts or job status.
- The future review store alone owns review/job progress. Policy validates observations before a plan can reach xmp and metadata_transaction.
- integrity and storage provide shared hash/immutable-write primitives.
- Native assets existing is not proof of completed focus inspection. Legacy overview score/describe is not the personal final-culling workflow.
- Keep public interfaces small and documented. Add a shared abstraction for real repeated behavior, not speculative extension points.
- Keep CLI output compact. Persist detailed evidence and progress; do not replay full histories or image base64 through every agent prompt.
- Use synthetic test files. Never commit private photos, catalog data, real user paths, credentials or generated review bundles.

## Validation and version management

Use a feature branch and PR. Stack on an implemented dependency when needed; do not implement it again.
Run cargo fmt --check, cargo build --locked, cargo test --locked --no-fail-fast,
cargo clippy --locked --all-targets -- -D warnings, and cargo doc --locked --no-deps with RUSTDOCFLAGS="-D warnings".
When the local toolchain is unavailable, validate the exact committed code in CI before marking ready; do not claim local testing.
Test observable contracts and failure cases. Dry-run/prepare/verify must not mutate photograph pixels or XMP metadata.
Every iteration must identify its interfaces, shared foundations reused, test evidence and incomplete end-to-end stages.

## Focused references

- [Provider implementation notes](docs/provider-implementation.md): provider traits, Rig 0.42 API details and legacy request behavior. Load only for transport changes.
- [Metadata safety and recovery](docs/lightroom-metadata-safety.md): conflict checks, snapshots, rollback and Lightroom concurrency limits.
- Rust 2024; configuration precedence is CLI > config file > defaults. API keys come from environment variables.
