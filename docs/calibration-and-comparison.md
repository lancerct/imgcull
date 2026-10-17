# Personal correction and comparison contract

This iteration improves the existing Rust engine, not a second Python workflow.
The LR plugin is unchanged. No private photo, screenshot, catalog or generated
review data is committed. The example corpus is anonymized declarative material,
not evidence that a model correctly inspected the user's originals.

## Separate poor quality from redundancy

`red` remains the conservative technical/low-artistic-quality proposal. New
`redundant` is also intended to become a red label, but has a different reason:
an explicitly retained equally good or better frame represents the same useful
content. Neither a low artistic score nor a strictly higher alternative score
is required. Two good frames can have the same score and only one be necessary.

The table/food correction motivates this distinction. It does NOT certify the
sharpness of either original from the screenshot. Full native subject inspection
and LR current appearance remain required. `Comparison` states the keeper,
content equivalence, relative quality and a concrete reason. Distinct/unknown
content, worse/unknown alternative quality, unique record value, uncertainty,
protected/unclassified sequences, self references, cycles, missing keepers,
different groups and severely defective keepers prevent redundancy approval.
Unlimited worthwhile different poses, expressions, behaviors and documentary
details can still be retained. Retain does not require stars for a weak-group
representative. A semantic group is reviewer-declared, not proved by file order.

The LR-current-appearance and model-native-inspection gates are NOT unlocked by
this work. Every CLI here is read-only for photos/XMP; `can_apply` remains false.
The new proposal is not wired to a metadata apply path. Legacy `score` is still
an overview pipeline, not the calibrated personal judgment path.

## Similarity only schedules comparisons

`suggest-similar` takes a bounded window of 2..64 verified evidence bundles.
It reuses evidence integrity checks and the single NativePhoto decoder, then
computes a small difference hash of the composition overviews. Pairwise distance
proposes comparisons, never keepers, quality scores, sequence membership or
rejection. No transitive clustering is presented as semantic equivalence. All
frames remain reviewable, including likely duplicates. Supply capture/session
windows rather than comparing unrelated libraries; EXIF-based window discovery
and semantic model comparison remain to be implemented. Low-texture frames can
collide; this is harmless only while hash matches remain hints. The maximum 256
candidate pairs requires a smaller window for excessively repetitive inputs.

Inspired by AI Photo Manager's local similarity stage and explicit `dup_of`
report relation, but intentionally does not reproduce its pre-AI hash rejection,
global 512px sharpness keeper selection or fixed Top-N policy. Its eight scoring
dimensions may inform explanations; they are not eight mandatory scores for
every genre. No source code was copied from that project in this iteration.

## Corrections persist as immutable preferences and cases

`calibration` owns preference artifacts, not job/progress state. `review` alone
owns progress. Publish a new file for every profile revision. Its canonical ID
binds all rules/cases; `parent_id` documents the intended predecessor but is not
an authenticated history or a check that an unavailable ancestor still exists.
Profiles use free scene/purpose tags, allowing overlapping contexts such as
portrait + motion + low_light or exhibit + inscription + reflective_glass.
There is no fixed list of genres that would need a code change for every scene.
Cases can specify up to four additional acceptable COMPLETE group outcomes.
For two truly equal unmarked frames, either may be the keeper; evaluation must
not teach an arbitrary first-frame preference. Alternatives cover the entire
group and require direct retained keepers, so per-frame flexibility cannot
accidentally approve all-red or all-retained outcomes. Reports choose the nearest
acceptable whole-group outcome for discrepancy accounting.

A case contains concrete observations/assumptions, a prior marking where known,
the corrected expected markings, the reason and scope, and a root cause:
perception, grouping, preference or explanation. A new correction should say
whether the model misread the picture, compared the wrong frames or applied the
wrong preference. A preference cannot repair missing pixels or prove focus.
Store only case-local opaque IDs in publishable examples. Real-photo review
evidence belongs in local immutable evidence/review records, not GitHub.
Actual image-based cases may carry bounded SHA-256 `evidence_ids` references to
those local bundles; the profile does not embed photo bytes or source paths.
These references identify material but do not by themselves verify viewing.

The shipped table case records the user's correction of leaving the second
near duplicate unmarked. Portrait and bird teaching cases preserve distinct
moments; uncertain and weak-group cases preserve the necessary boundaries.
Astro, exhibit details and a different equivalent pair are synthetic validation
counterexamples. Synthetic declared facts test decision contracts, NOT visual
perception or real held-out accuracy.

Teaching context is retrieved by tags, includes global rules and is bounded to
16 rules / 8 examples / 32 KiB serialized context. Broad matches fail explicitly instead of silently dropping
rules. Validation answers are excluded from context. A reviewer with access to
the entire profile can still read those answers: this is not a secure blinded
benchmark. Independent real-photo test material and recorded model/prompt
versions are needed before claiming quality improvements.

`review-init --calibration` embeds the published profile in the immutable job.
Replacing/removing the external profile does not change that job. Policy version
2 and job schema 2 refuse older jobs rather than silently reinterpret them. Keep
old jobs for their original engine; deliberately create a new job to adopt a new
policy/profile. Automatic migration or reuse of prior judgments is not provided.
No version is automatically activated merely because it was published.

## Validation loop

1. Capture a concrete user correction and its boundary; add counterexamples.
2. Publish a new profile draft with the previous profile ID and a change note.
3. Retrieve only relevant teaching cases for the reviewer, not the whole chat.
4. Generate predictions independently for a complete validation split; include
   exact profile ID and a model/prompt revision identifier.
   `calibration-challenge` exports bounded pages of input-only cases: no expected
   decisions, previous labels, correction lessons or root-cause hints. Retrieve
   relevant teaching context separately, and combine predictions across pages.
5. Evaluate exact marking/keeper/stars/green differences; false red and missed
   redundancy are reported separately. Missing/extra cases/frames and stale
   profile IDs fail. A failed evaluation exits nonzero after emitting JSON.
6. Inspect disagreements; decide whether to revise preference, improve image
   evidence, or change the model. A passing synthetic evaluation alone cannot
   authorize live markings or claim the model actually saw the right details.

This is prompt/case calibration, not weight fine-tuning or autonomous learning.
The evaluator checks supplied typed decisions, including redundancy support;
the real image/provider workflow remains a separate necessary integration.

## CLI examples

```text
imgcull calibration-publish examples/calibration-draft.json --output profile-v1.json
imgcull calibration-context profile-v1.json --tags food near_duplicate
imgcull calibration-challenge profile-v1.json --split validation --offset 0 --limit 5
imgcull review-init PHOTO_OR_DIRECTORY --output NEW_JOB --calibration profile-v1.json
imgcull review-context NEW_JOB --tags bird burst
imgcull suggest-similar EVIDENCE_A EVIDENCE_B --distance 6
imgcull review-report NEW_JOB --output review-page-1.html --offset 0 --limit 50
imgcull calibration-evaluate profile-v1.json predictions.json --split validation
```

The report reuses latest drafts and revalidates the selected page's sources and
evidence. It shows keeper filenames/IDs, stars/green, reasons, concrete edits and
hold conditions. It embeds only small composition previews, escapes user text,
has no scripts/external resources and refuses an existing output. It explicitly
does not certify page-external evidence, complete grouping, or native sharpness.
Pagination follows stable job order, not an inferred semantic grouping.

`predictions.json` maps case IDs to frame IDs to the SAME engine `Proposal`:

```json
{
  "profile_id": "PUBLISHED_ID",
  "reviewer": "MODEL_AND_PROMPT_REVISION",
  "cases": {
    "CASE_ID": {
      "a": {"action":"retain", "stars":4, "green":false},
      "b": {"action":"redundant", "comparison":{
        "keeper":"a", "content":"equivalent", "quality":"equivalent",
        "reason":"Same useful content; a is an equally good retained representative"
      }}
    }
  }
}
```

All cases/frames of the chosen split must be provided. This illustration alone
is intentionally not a complete validation run for the supplied example profile.

## Remaining end-to-end work

Actual LR current-effect binding into a job/reviewer; validated provider-native
inspection; real subject localization and semantic group comparison; independent
image-based calibration runs; metadata plan/apply integration with current human
markings and RAW/JPEG synchronization; final skill/local-model workflow. None
are inferred complete from these contract tests. Original imgcull internals may
be replaced when justified; this iteration reuses the audited decoder, evidence,
storage, metadata foundations and sole review store rather than rebuilding them.
