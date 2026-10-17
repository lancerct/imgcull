# Personal culling architecture and iteration contract

The product is one Rust engine with a thin future skill. The skill coordinates typed commands and explains uncertain decisions; it must not implement its own file pairing, decoding, XML editing, retry loops, or second review database.

## End-to-end flow and ownership

| Stage | Owner | Input / output | Allowed effects |
|---|---|---|---|
| Discover | discovery | requested paths → canonical photo identities; RAW/JPEG pairs share one target | filesystem reads |
| Prepare evidence | imaging + evidence | identity + native regions + pixel floor → immutable evidence ID, overview, native PNG details | write a new evidence directory only |
| Review | future typed review module; transport in llm | evidence ID + bounded views + group context → observations and uncertainty | append review records; no XMP |
| Validate decisions | future policy module | observations + protection/group rules → explicit star/label proposals | no filesystem mutation |
| Plan | future review store | proposals + existing human metadata + source/evidence hashes → reviewable change plan | persist plan; no XMP |
| Apply / rollback | xmp + metadata_transaction | validated plan → narrow XMP updates and recovery journal | metadata writes only |
| Verify Lightroom | future integration | applied metadata + Lightroom display → mismatch report | explicit read/sync workflow |

Preparing files is not a visual judgment. A native focus asset existing on disk does not prove a reviewer saw it or selected the right subject. Overview-only evidence must never be promoted to a final focus verdict.

The existing score/describe commands remain a legacy overview pipeline. Their output does not satisfy this personal native-detail review contract. Migration to the typed decision flow is a separate iteration; do not install a skill that treats legacy overview scores as final culling decisions.

## Shared foundations

- discovery alone resolves same-stem pairs and source preference. Callers must not duplicate extension lists or pairing rules.
- imaging alone fully decodes JPEG/raster sources or develops RAW sensor data. It normalizes orientation once and owns every crop coordinate. No embedded RAW preview fallback.
- preprocessing is only a legacy provider payload adapter over imaging. It cannot keep its former second JPEG/RAW decoder.
- evidence derives every requested view from one in-memory decode. Crop pixels are encoded as PNG without scaling or sharpening. Composition overview is capped at 2048px.
- integrity owns SHA-256 of byte buffers and streamed files, shared by evidence and metadata recovery.
- storage owns immutable-file creation and flush behavior, shared by evidence publication and transaction snapshots. Publication policy belongs to the caller.
- xmp owns namespace-aware byte-preserving property edits. metadata_transaction alone owns metadata locking, conflict detection, backup and rollback.
- llm owns provider transport. Prompts describe photographic judgment, not filesystem mechanics.
- Future review records and plans must reference evidence IDs and source hashes; they must not embed image base64, whole historical conversations, or duplicate XMP documents.

Use small concrete modules and structs. Do not introduce generic workflow frameworks or a provider-specific state machine. Add an abstraction when two real callers need identical behavior; do not hide different policies behind a shared helper merely because they both write files.

## Iteration gates

Each PR must state its place in the complete flow, public interfaces, shared code reused, invariants tested, and remaining limits. A successful isolated helper is not a completed end-to-end product.

Required checks: cargo fmt --check; cargo build --locked; cargo test --locked --no-fail-fast; cargo clippy --locked --all-targets -- -D warnings. Test actual failure modes and contracts, not a second implementation of the same algorithm. Keep changes on feature branches. This iteration is stacked on the metadata safety branch; merging/rebasing the stack is a version-control operation, not another implementation.

## Evidence interface (this iteration)

```text
imgcull prepare PHOTO --output NEW_DIRECTORY --regions REQUESTS.json --min-pixels 45000000
imgcull verify-evidence DIRECTORY
```

The 45-million-pixel floor is a conservative default for the user's approximately 45MP camera. It is an explicit source-size requirement, not an upscaling target. Older/lower-resolution cameras require their actual intended floor; 0 means no extra pixel floor, while still fully decoding the real source.

A RAW/JPEG pair uses its JPEG companion when it fully decodes and meets the floor. An undersized/unreadable JPEG causes full RAW development; failure stops preparation. RAW uses rawler's default sRGB development, not Lightroom's develop recipe. Camera-specific color matching and camera-specific RAW validation remain separate checks.

Requests are a JSON array in orientation-normalized native coordinates:

```json
[{"id":"subject-eye","role":"focus","region":{"x":3500,"y":1200,"width":800,"height":800}},
 {"id":"left-edge","role":"edge","region":{"x":0,"y":1500,"width":600,"height":1000}}]
```

Roles: focus, context, edge. Each detail edge must be 1–1024 pixels; split larger areas into explicit regions. There is no automatic subject detection in this iteration. Up to 64 regions can be prepared together without re-decoding for each crop. Re-running prepare with new requests decodes again; no persistent full-frame raster cache or memory-budgeted LRU is claimed.

The output contains evidence.json, overview.jpg, and unscaled detail-NNNN.png files. Standard output is a compact JSON summary with review_status=unreviewed. Empty/edge-only requests do not invent focus coverage. The bundle is immutable; verify and reuse it, or choose a new output. Source/view changes invalidate reuse. Hashes detect accidental corruption and stale sources; they are not signatures against someone able to rewrite the whole manifest and its hashes.

CLI output contains no image base64 or repeated review history. This reduces repeated text and enables cached evidence reuse; it does not reduce the visual evidence needed for a reliable decision, and it is not a measured token-savings claim. Local-model accuracy and context behavior still require evaluation.

## Remaining work

Native geometry and focus samples do not certify calibrated color. RenderInfo records unmanaged-raster or raw-default-srgb. Raster ICC conversion is not implemented; color-managed artistic/edit assessment must be added before final color judgments.

Automatic subject-region proposals; actual native-detail model review and coverage records; burst/stack protection; user-calibrated 3/4/5-star and red/green policy; durable job progress and resumable review ledger; plan/apply integration; independent JPEG embedded metadata synchronization; Lightroom verification; and final skill packaging.
