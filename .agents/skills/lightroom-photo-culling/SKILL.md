---
name: lightroom-photo-culling
description: Review and select Lightroom photos in groups, preserve distinct good moments and composite sequences, and prepare traceable star/red/green decisions with resumable checkpoints.
---

# Lightroom photo culling

Use imgcull as an optional analysis engine. The agent must actually inspect images and
compare related frames; a numeric score is supporting evidence, never a deletion rule.
This skill does not switch providers or establish a local model's accuracy.

## Start or resume

1. Read [selection rules](references/selection-rules.md).
2. Read [imgcull integration and metadata boundaries](references/integration.md) before
   running imgcull or planning metadata changes.
3. Confirm scope from the request; reuse an existing run rather than rescanning or
   overwriting completed work. Inventory real photos, excluding thumbnail/cache trees.
   Pair same-directory, same-stem RAW/JPEG as one photo; filenames alone across folders
   are not unique. Check orientation, capture time, camera and existing metadata.
4. Keep source images, private paths, previews, ledgers and backups outside Git.
   Use a separate local run directory. Do not open Lightroom or steal desktop focus
   during background review. Start with one worker; local GPU inference can affect games.

## Review

- Compare a scene/burst together. Use contact sheets for navigation, full images for
  composition and edges, and native subject/eye crops for focus or motion judgments.
  Do not use a fixed center crop when the subject is elsewhere.
- Protect astrophotography stacking sequences; conservatively protect suspected
  brackets and panoramas until their purpose is resolved.
- Inspect RAW or the current developed rendering when JPEG evidence cannot settle
  exposure recovery or focus. Record the limitation if unavailable; leave uncertain
  cases unmarked.
- Save every completed group immediately. Store decisions outside chat context.
  On resume, read run status and only the current group and relevant rules.
- A red decision is a suggestion, never a file deletion. A group without a good frame
  still needs its relatively best fallback when it has record value.
- Run the checkpoint helper through Python 3.10+:
  `python <skill-dir>/scripts/review_ledger.py init <run.db> <manifest.json>`
  `python <skill-dir>/scripts/review_ledger.py record <run.db> <decisions.json>`
  `python <skill-dir>/scripts/review_ledger.py status <run.db>`
  `python <skill-dir>/scripts/review_ledger.py export <run.db> <review-plan.json>`
  See [ledger format](references/ledger-format.md). It validates records, not image truth.

## Finish and writeback boundary

The helper only writes a checkpoint database and review plan; it never changes photos
or XMP. Do not imply that exporting this plan applies it to Lightroom.

Before any separate writer is used, require: latest metadata reconciled with the
catalog, unchanged source preconditions, immutable backups, staged XML validation,
preservation of unrelated develop/mask fields, atomic replacement and a recoverable
transaction log. Malformed or conflicting metadata blocks that item, not the whole
review. No direct writes to a .lrcat database. No blind source-wide imgcull command.

Preserve user ratings/labels. Use literal labels matching the active Lightroom set
(Chinese default: 红色 / 绿色), not an assumed universal English mapping.
JPEG-only metadata needs a format-aware embedded writer; a sidecar alone is not a
verified Lightroom solution.

Report reviewed/remaining counts, proposals vs applied changes, protected/uncertain
items and actual verification. The current package has no automatic XMP transaction
writer, no automatic burst detector and no certified local-model adapter.
