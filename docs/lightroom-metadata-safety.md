# Lightroom metadata safety foundation

This branch continues the existing Rust application. It does not introduce a second Python review database. Existing upstream and audit branches remain available for comparison.

## Implemented

- Canonicalize repeated inputs and assign one writer to each same-folder/same-stem RAW/JPEG group. RAW is its identity; use the JPEG companion as review input when available. Multiple RAW formats sharing an XMP are ambiguous and stop the run before any writes.
- Parse XML by namespace, including alternate prefixes, single quotes, attribute/element ratings, and self-closing Description elements. Refuse malformed, duplicate, or multiple top-level Description layouts instead of guessing.
- Only rewrite explicitly changed properties. Unchanged Lightroom develop settings, crops, nested masks, multilingual captions, keywords, labels and ratings remain byte-for-byte intact.
- Automatic scoring preserves any existing rating (including 0 and -1), caption, keyword bag and color label. Force permits re-scoring custom imgcull fields, not overriding human metadata.
- Reject incomplete, non-finite and out-of-range scoring results. If either requested model stage fails, do not save a partial photo result.
- Serialize imgcull writers with OS locks, compare the original XMP immediately before replacement, and replace via a same-directory temporary file. Source read, backup or validation errors stop that photo and make the command fail.
- Always record immutable per-change before/after snapshots and a hash-checked transaction under the photo folder's .imgcull/transactions. Optional legacy .xmp.bak preserves its first contents rather than overwriting it.
- Bound active workers to configured concurrency (1–32). Exclude hidden cache folders and recursively encountered symlinks.
- Report successful scoring/descriptions only after saving, and return a nonzero exit status for any photo failure.

## Recovery

Each change directory contains transaction.json, after.xmp, before.xmp when replacing an existing file, and an applied marker after success.

Run:

```text
imgcull rollback /path/to/photo-folder/.imgcull/transactions/change-XXXXXX/transaction.json
```

Rollback verifies snapshot hashes and refuses if the target no longer matches this transaction's output. For a newly created sidecar, rollback removes only that exact output. Existing files restore the original bytes.

A crash may leave a prepared transaction without an applied marker. The target and before/after hashes determine whether replacement happened; guarded rollback works when the target matches after.xmp, even without the marker. Snapshots are retained for inspection.

Use a local filesystem. Atomic replacement is not a guarantee against storage hardware failure. Directory entries are not explicitly fsynced; power-loss durability is not claimed. Network filesystems may have different locking/rename behavior.

Lightroom does not participate in imgcull's locks. The final comparison detects edits up to the check; it cannot remove the narrow race with a non-cooperating editor after that check. Do not save metadata from Lightroom during the apply operation. Background analysis and games need no UI control.

## Not yet the personal culling workflow

This foundation retains upstream scoring/preprocessing behavior. It must not be used as the final 45MP culling workflow yet. Pending work:

1. Fully decode native-resolution images and inspect overview plus native-pixel subject details; no embedded RAW preview or resized overview may serve as final focus/blur evidence.
2. Review groups with the agreed 3/4/5-star policy, red/green labels, unlimited distinct portrait moments, fallback keeper, stacking protection, and bird/museum record value.
3. Persist review evidence and expose a reviewable apply plan, separate from model requests, with interruption/resume support.
4. Verify Lightroom reads ratings/labels for the RAW/JPEG pair. A JPEG treated independently by Lightroom may require embedded metadata synchronization, which this sidecar-only change does not implement.
5. Package a skill around the tested Rust interface; evaluate local model capability and context limits separately.

No real photo files were modified while developing this branch. Windows and Linux CI run synthetic-file regression tests. There is no local Rust toolchain on the current machine, so local compilation is not claimed.

