# imgcull integration boundaries

Code reviewed at upstream commit 022c4161302aa29a5a230c0469814507e97e385b (0.2.8).
Recheck these observations after upstream updates.

## Reusable pieces

The Rust tool provides image discovery, RAW/JPEG preprocessing, configurable scoring
guidelines, provider adapters (including Ollama), per-photo scores and XMP reports.
Keep the upstream copyright and licenses.

## Verified gaps in this revision

- src/discovery.rs enumerates RAW and JPG independently. src/xmp.rs maps both
  extensions to the same .xmp. A paired directory can therefore send concurrent
  writers to the same sidecar. Do not run the stock pipeline over paired sources.
- src/pipeline.rs replaces an unreadable/malformed XMP with a new sidecar in memory.
  It logs a backup failure but proceeds to write. Those are unsafe fallbacks for a
  library with existing develop data.
- src/xmp.rs writes through fs::write, and backup_sidecar copies to a fixed .xmp.bak;
  these are not immutable, transactionally recoverable backups.
- src/pipeline.rs writes mapped ratings unless --no-rating is set. That flag still
  allows descriptions, keywords and custom score fields to be written.
- src/preprocessing.rs uses a 2048-pixel maximum dimension on the resized path.
  Single-photo input and scalar averaging in src/scoring.rs cannot implement
  burst-aware comparisons or establish native subject sharpness.
- Skip-existing-scores behavior is useful but is not a durable group review ledger
  recording completed, protected, failed and uncertain decisions.
- Stock imgcull has no verified personal red/green decision writer in this revision.
  Its own stars use equal-weight score bands, not the owner's keeper rubric.

## First version workflow

Prefer direct agent review using local image tools and the checkpoint helper.
If imgcull scoring is useful, run only on isolated scratch copies, with a single
representative per RAW/JPG pair and bounded concurrency. Keep its scores advisory;
never import its sidecars or average scores as final personal decisions.
Scratch previews do not establish RAW focus or highlight recoverability.

Do not change model provider or load a local GPU model as a side effect of activating
the skill. Provider switching, local-model quality evaluation and context diagnostics
are later work.

## Separate writeback implementation acceptance

Before enabling source writeback, test the following on disposable copies:

1. Pairing and target uniqueness; no two jobs write the same metadata target.
2. Existing user stars/labels and current catalog settings reconciled, not silently
   overwritten. Lightroom can have unsaved edits newer than sidecars.
3. Byte-hashed preconditions, immutable backup, staged parse, semantic preservation
   of all fields except authorized Rating/Label; masks and namespaced arrays included.
4. Atomic per-file replacement, crash recovery and idempotent resume. A batch is not
   one filesystem transaction; journal each item and recover partial completion.
5. Failed backup, malformed XML, stale precondition, duplicate fields or source change
   stops that item. Re-read and rebase rather than force.
6. Rollback checks the current output hash and refuses to erase later user edits.
7. Chinese label mapping and actual catalog readback verified. Do not alter the user's
   label set just to make English labels display.
8. RAW sidecars vs JPEG embedded metadata handled separately, with original image
   pixels/entropy preserved. No direct catalog database mutations.

No such writer is shipped in this initial skill addition.
