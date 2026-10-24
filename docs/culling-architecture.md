# Personal culling architecture and iteration contract

> 当前用户标准：2026-10-25 已采用 [打标标准 v1.0](culling-standard-v1.0.md)，仅使用 1／3／4／5 星和暂不评级。本文原有红绿标签、艺术分数阈值与 policy v2 描述仅说明旧实现，不再作为新的摄影判断标准。现有代码尚未完成星标 v1.0 的实现适配；不得据此自动迁移或写回。原尺寸证据、LR 当前效果、任务完整性与元数据保护约束继续有效。


The product is one Rust engine with a thin future skill. The skill coordinates typed commands and explains uncertain decisions; it must not implement its own file pairing, decoding, XML editing, retry loops, or second review database.

## End-to-end flow and ownership

| Stage | Owner | Input / output | Allowed effects |
|---|---|---|---|
| Discover | discovery | requested paths → canonical photo identities; RAW/JPEG pairs share one target | filesystem reads |
| Prepare evidence | imaging + evidence | identity + native regions + pixel floor → immutable evidence ID, overview, native PNG details | write a new evidence directory only |
| Review | review + policy types; future transport in llm | evidence ID + bounded views + group context → observations and uncertainty | append review records; no XMP |
| Validate decisions | policy | observations + protection/group rules → held or structurally valid proposals | no filesystem mutation |
| Plan | future integration over review | proposals + existing human metadata + source/evidence hashes → reviewable change plan | persist plan; no XMP |
| Apply / rollback | xmp + metadata_transaction | validated plan → narrow XMP updates and recovery journal | metadata writes only |
| Verify Lightroom | future integration | applied metadata + Lightroom display → mismatch report | explicit read/sync workflow |

Only the review store owns job/review status; evidence artifacts never duplicate or reset it. Preparing files is not a visual judgment. A native focus asset existing on disk does not prove a reviewer saw it or selected the right subject. Overview-only evidence must never be promoted to a final focus verdict.

The existing score/describe commands remain a legacy overview pipeline. Their output does not satisfy this personal native-detail review contract. Migration to the typed decision flow is a separate iteration; do not install a skill that treats legacy overview scores as final culling decisions.

## Shared foundations

- discovery alone resolves same-stem pairs and source preference. Callers must not duplicate extension lists or pairing rules.
- imaging alone fully decodes JPEG/raster sources or develops RAW sensor data. It normalizes orientation once and owns every crop coordinate. No embedded RAW preview fallback.
- preprocessing is only a legacy provider payload adapter over imaging. It cannot keep its former second JPEG/RAW decoder.
- evidence derives every requested view from one in-memory decode. Crop pixels are encoded as PNG without scaling or sharpening. Composition overview is capped at 2048px.
- integrity owns SHA-256 of byte buffers and streamed files, shared by evidence and metadata recovery.
- storage owns immutable-file creation/flush and bounded regular-file control JSON reads, shared by evidence publication, transaction snapshots, review and calibration. Publication policy and schemas belong to callers.
- xmp owns namespace-aware byte-preserving property edits. metadata_transaction alone owns metadata locking, conflict detection, backup and rollback.
- llm owns provider transport. Prompts describe photographic judgment, not filesystem mechanics.
- lightroom verifies trial current-effect exports with the existing full decoder and hashes. The thin Lua bridge delegates rendering to LR, without photo pairing, review progress, policy or XMP logic. Trial artifacts alone never unlock current-appearance review; see [LR export contract](lightroom-export.md).
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

The output contains evidence.json, overview.jpg, and unscaled detail-NNNN.png files. Standard output is a compact JSON summary with contains_visual_verdict=false. Empty/edge-only requests do not invent focus coverage. The bundle is immutable; verify and reuse it, or choose a new output. Source/view changes invalidate reuse. Hashes detect accidental corruption and stale sources; they are not signatures against someone able to rewrite the whole manifest and its hashes.

CLI output contains no image base64 or repeated review history. This reduces repeated text and enables cached evidence reuse; it does not reduce the visual evidence needed for a reliable decision, and it is not a measured token-savings claim. Local-model accuracy and context behavior still require evaluation.

## Remaining work

Native geometry and focus samples do not certify calibrated color. RenderInfo records unmanaged-raster or raw-default-srgb. Raster ICC conversion is not implemented; color-managed artistic/edit assessment must be added before final color judgments.

Provider image preprocessing can downsample even a native PNG crop. The future review transport must choose crop sizes compatible with the provider/model and record what was actually submitted; a 1024px file alone does not certify 1:1 model inspection. Unsupported RAW development fails rather than substituting a preview; library panics are contained as errors, without claiming protection against process-level out-of-memory aborts.

Automatic subject-region proposals; actual native-detail model review and coverage records; burst/stack protection; user-calibrated 3/4/5-star and red/green policy; plan/apply integration; independent JPEG embedded metadata synchronization; Lightroom verification; and final skill packaging.


## Resumable draft reviews

```text
imgcull review-init PHOTO_OR_DIRECTORY --output NEW_JOB --min-pixels 45000000
imgcull review-list JOB --offset 0 --limit 50
imgcull review-status JOB
imgcull review-show JOB PHOTO_ID
imgcull record-review JOB --expected-revision 0 --photo-id ID --evidence BUNDLE --observation JSON_FILE
imgcull review-check JOB
```

review is the only progress store. It freezes logical identities, all physical same-stem companions, existing XMP hashes and the required decoded pixel floor, even for a JPEG-only input. Strict job discovery refuses partial results for missing/unreadable requested inputs, using the same shared walker as legacy discovery. Ambiguous multiple-RAW or multiple-JPEG companion sets fail before job publication. The job floor defaults to the same shared 45MP constant as prepare; a low-floor bundle cannot bypass a stricter job floor. It reuses discovery, integrity, storage, evidence and policy; there is no Python/skill database. A photo edited or supplemented since the freeze cannot receive a new checkpoint. Source/XMP changes require a deliberate fresh job for the changed inputs; automatic migration/rebasing is not implemented.

Job and checkpoint checksums detect accidental mutation, not hostile forgery. Checkpoints form an ordered immutable hash chain rooted in job.json. Each record is flushed in a staging directory and published by rename while holding the job OS lock. A crash before publication leaves an ignored staging directory; after publication the checkpoint is recoverable. Stale expected revisions fail without overwriting work. Internal gaps, reordered entries and changed committed bytes fail instead of resetting progress. A hash chain cannot detect deletion of its entire trailing suffix without an independent trusted tip; this store does not claim protection against a person removing completed records or rewriting the complete store. This is a local-filesystem, cooperating-process contract, not a power-loss durability or distributed-store guarantee. Each short-lived CLI invocation currently scans committed checkpoint history; full-library throughput has not been benchmarked. A future long-running reviewer should reuse loaded state or amortize bounded batches rather than add a second authoritative database.

status/list/show validate committed history and return compact progress or one latest draft, not evidence validity. Photo scheduling follows stable primary-path order, not random hash order; actual chronological burst grouping still needs EXIF/group analysis. review-check rehashes current companions/XMP, verifies each latest evidence bundle and reevaluates coverage and group constraints. It fails on stale or corrupt material. Its JSON diagnostic output can be large for a full job; ordinary scheduling should use bounded list pages and compact status, not repeat the full diagnostic report in every prompt. Recorded means a draft exists, never that visual review is complete. can_apply is always false in this stage.

Observations declare reviewer identity, exact view IDs/hashes/submitted dimensions, native-inspection path, edge/background coverage, uncertainty, artistic score (0..10), unique record value, severe technical defects, sequence membership, concrete edits, explanation and proposal. Schemas reject unknown fields and out-of-range values. Observation text and view counts are bounded. Receipts are trusted reviewer declarations, not authenticated proof of what someone saw or of correct subject localization. ModelNative inspection has no accepted path until provider preprocessing has been validated; ModelUnverified stays held. HumanNative is only for explicit real 1:1 human inspection, never a label a model may assign itself.

Personal stars are 3 (worth keeping/meaningful representative), 4 (independently presentable), 5 (exceptional), separate from artistic scores. Poor-quality red requires a severe technical defect or artistic <=4 without unique record value; 5 requires a better explicitly retained comparable frame. Policy v2 separately supports redundant red advice for equivalent useful content with a sound retained equally good or better keeper, regardless of artistic score; see calibration-and-comparison.md. Unselected is not red. Unknown, astro stacking, bracket and panorama groups are protected from automatic marking, including stars. Burst groups need a retained representative; any number of good distinct moments may be kept. Replacement claims must reference a better unambiguous keeper in the same burst; red cycles cannot satisfy them. Green requires concrete edit advice, and retention/red are mutually exclusive by type. Missing observations and group blockers must be resolved before a future plan can be built; group checks alone do not certify peer evidence or a complete correctly grouped sequence.

The user chose **Lightroom's current developed appearance for artistic value**, and **original native pixels for technical inspection**. Existing camera JPEG/default RAW evidence cannot satisfy the first requirement. Current policy explicitly holds star/green/artistic-red proposals with lightroom_current_appearance_unavailable. Do not add a caller-controlled boolean to bypass this gate. A future rendition adapter must prove source/XMP linkage and correct current appearance. This stage validates draft observations; it does not implement automatic visual review, subject detection or final numerical calibration. It cannot authorize metadata writes, and legacy scoring must not be used as a shortcut around these gates.

## Versioned feedback and comparison

Policy v2/job schema 2 add a separate typed redundancy reason, immutable scene-tagged correction profiles frozen into jobs, bounded teaching-context retrieval, complete-split decision evaluation, non-destructive similarity pair proposals, and paginated draft comparison reports. See [calibration and comparison](calibration-and-comparison.md) for commands, counterexamples, data ownership and the distinction between contract tests and real visual accuracy. Existing LR/current appearance, native inspection and can_apply=false gates remain. No profile update silently changes an existing job.
