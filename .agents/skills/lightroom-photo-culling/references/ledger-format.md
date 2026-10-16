# Checkpoint format

The helper uses Python standard library SQLite and JSON, with one committed record
batch per transaction. Keep the run DB and its WAL outside the repository. It does
not execute imgcull, decode images, validate visual evidence or write source metadata.

## Manifest

Create a UTF-8 JSON object with:
- root: absolute source directory.
- photos: list of objects with id (stable string), group (stable comparison-group string),
  files (nonempty list of paths relative to root), protected (boolean),
  existing_rating (null or integer 0..5), existing_label (null or string).

Pair same-folder/same-stem RAW and JPG into ONE photo. Get existing metadata from the
actual latest metadata; null means confirmed unset, not unread. A 0 rating means
unrated. Resolve catalog/file conflicts before creating the manifest. Different
cameras/files with reused basenames in other folders are different photos.
Group boundaries must reflect actual scene/action, not just a fixed time gap.
Mark suspected composites protected. Paths are kept private in the local DB.

Example:
```json
{"root":"D:/Photos/trial","photos":[
  {"id":"camA-001","group":"portrait-01","files":["DSC_0001.NEF","DSC_0001.JPG"],
   "protected":false,"existing_rating":null,"existing_label":null}
]}
```

init refuses to overwrite a DB, validates files stay within root and rejects duplicate
source paths, reused sidecar targets and malformed manifests. Reuse the same DB on
resume. Source metadata changes must be reconciled before a later writer; this DB is
a review checkpoint, not a hash-validated write transaction.

## Decisions

record reads a JSON LIST of objects. Required fields:
- id, rating (null or 3/4/5; null preserves existing), label (null/red/green).
- reason: concrete nonempty explanation.
- evidence: list of nonempty local artifact references, e.g. a full image or subject crop.
- full_reviewed, native_detail: booleans recording the review actually performed.
- independent_value: boolean; true when an otherwise weak photo still has unique value.
- artistic_score: null or number 0..10.
- reject_kind: null, technical, artistic, or superseded.
- alternative_id: null or a retained photo ID in the same group.
- edit: null or concrete nonempty edit suggestion (required for green).

Unmarked decisions are reviewed neutral, not pending. Protected photos must remain
unmarked and cannot be rejected. Reds require full review; technical reds also need
native detail. Artistic reds need score <=4 and no independent value. Superseded reds
need score <=5, no independent value and a retained, fully reviewed same-group
alternative. No entire multi-frame group can be marked red. Four/five stars need full review and
native detail. An uncertain photo can be recorded neutral with the limitation in reason.

All records are revalidated together on every transaction so changing a retained
alternative later cannot silently invalidate earlier reds. Invalid batches roll back
entirely. Record updates use UPSERT; repeated identical records are idempotent.
status reports pending IDs; export refuses incomplete runs and writes an atomic JSON
review plan to a NEW output path (to avoid overwriting a source or previous artifact).
It does not write XMP. The plan includes manifest and review decisions; keep it private.
