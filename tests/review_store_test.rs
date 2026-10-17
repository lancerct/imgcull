use imgcull::{
    evidence::{self, DetailRequest, DetailRole, Evidence},
    imaging::Region,
    policy::{self, Inspection, Observation, Proposal, SeenView, SequenceKind},
    review,
};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

fn setup(root: &Path) -> (PathBuf, PathBuf, Evidence, String) {
    let photo = root.join("photo.JPG");
    image::RgbImage::from_fn(80, 60, |x, y| image::Rgb([x as u8, y as u8, 90]))
        .save(&photo)
        .unwrap();
    let job = root.join("job");
    review::create(std::slice::from_ref(&photo), &job, 0).unwrap();
    let id = review::page(&job, 0, 1).unwrap().photos[0].photo_id.clone();
    let bundle = root.join("bundle");
    let evidence = evidence::prepare(
        &photo,
        &bundle,
        &[DetailRequest {
            id: "eye".into(),
            role: DetailRole::Focus,
            region: Region {
                x: 0,
                y: 0,
                width: 16,
                height: 16,
            },
        }],
        0,
    )
    .unwrap();
    (job, bundle, evidence, id)
}

fn observation(evidence: &Evidence) -> Observation {
    Observation {
        evidence_id: evidence.evidence_id.clone(),
        reviewer: "synthetic-human".into(),
        seen: evidence
            .views
            .iter()
            .map(|v| SeenView {
                id: v.id.clone(),
                sha256: v.sha256.clone(),
                width: v.width,
                height: v.height,
            })
            .collect(),
        inspection: Inspection::HumanNative,
        edges_checked: true,
        uncertain: false,
        artistic_score: Some(4),
        record_value: false,
        technical_failure: None,
        group: "test-group".into(),
        sequence: SequenceKind::Independent,
        edits: vec![],
        reason: "Synthetic observation for contract tests, not a real visual verdict".into(),
        proposal: Proposal::Leave,
    }
}

fn redundant(keeper: &str) -> Proposal {
    Proposal::Redundant { comparison: policy::Comparison {
        keeper: keeper.into(), content: policy::ContentRelation::Equivalent,
        quality: policy::AlternativeQuality::Equivalent,
        reason: "Same useful dish/composition; a verified equally good keeper is sufficient".into(),
    } }
}

#[test]
fn equal_good_table_frames_allow_redundancy_without_low_artistic_score() {
    let root = TempDir::new().unwrap();
    let (_, _, evidence, _) = setup(root.path());
    let mut a = observation(&evidence);
    a.sequence = SequenceKind::Burst;
    a.artistic_score = Some(8);
    a.proposal = Proposal::Retain { stars: Some(4), green: false };
    let mut b = a.clone();
    b.proposal = redundant("a");
    assert!(policy::group_blockers(&[("a", &a), ("b", &b)]).unwrap().is_empty());
    let v = policy::validate(&b, &evidence).unwrap();
    // The new redundancy reason is valid structurally; missing LR-current
    // appearance still holds it. No caller flag bypasses that separate gate.
    assert_eq!(v.blockers, vec!["lightroom_current_appearance_unavailable"]);
    b.artistic_score = None;
    assert_eq!(policy::validate(&b, &evidence).unwrap().blockers, v.blockers);
}

#[test]
fn redundancy_does_not_cross_distinct_moments_or_unique_record_value() {
    let root = TempDir::new().unwrap();
    let (_, _, evidence, _) = setup(root.path());
    let mut o = observation(&evidence);
    o.sequence = SequenceKind::Burst;
    o.proposal = redundant("keeper");
    if let Proposal::Redundant { comparison } = &mut o.proposal {
        comparison.content = policy::ContentRelation::Distinct;
    }
    assert!(policy::validate(&o, &evidence).unwrap().blockers.contains(&"distinct_or_unresolved_content".into()));
    o.proposal = redundant("keeper");
    o.record_value = true;
    assert!(policy::validate(&o, &evidence).unwrap().blockers.contains(&"unique_record_value".into()));
}

#[test]
fn redundant_alternative_must_be_retained_sound_and_same_group() {
    let root = TempDir::new().unwrap();
    let (_, _, evidence, _) = setup(root.path());
    let mut a = observation(&evidence);
    a.sequence = SequenceKind::Burst;
    a.proposal = Proposal::Retain { stars: None, green: false };
    let mut b = a.clone(); b.proposal = redundant("a");
    a.technical_failure = Some("Severe subject defocus".into());
    assert!(!policy::group_blockers(&[("a", &a), ("b", &b)]).unwrap().is_empty());
    a.technical_failure = None; a.group = "other".into();
    assert!(!policy::group_blockers(&[("a", &a), ("b", &b)]).unwrap().is_empty());
    a.group = b.group.clone(); a.proposal = redundant("b");
    assert!(!policy::group_blockers(&[("a", &a), ("b", &b)]).unwrap().is_empty());
    b.proposal = redundant("b");
    assert!(!policy::group_blockers(&[("b", &b)]).unwrap().is_empty());
}

#[test]
fn redundancy_keeps_protection_and_native_coverage_requirements() {
    let root = TempDir::new().unwrap();
    let (_, _, evidence, _) = setup(root.path());
    let mut o = observation(&evidence);
    o.proposal = redundant("keeper");
    for kind in [SequenceKind::AstroStack, SequenceKind::Bracket, SequenceKind::Panorama, SequenceKind::Unknown] {
        o.sequence = kind;
        assert!(policy::validate(&o, &evidence).unwrap().blockers.contains(&"protected_or_unknown_sequence".into()));
    }
    o.sequence = SequenceKind::Burst;
    o.inspection = Inspection::ModelUnverified;
    o.uncertain = true;
    let blockers = policy::validate(&o, &evidence).unwrap().blockers;
    assert!(blockers.contains(&"native_inspection_unverified".into()));
    assert!(blockers.contains(&"uncertain".into()));
}

#[test]
fn comparison_report_is_escaped_immutable_and_never_writes_xmp() {
    let root = TempDir::new().unwrap();
    let (job, bundle, evidence, id) = setup(root.path());
    let mut o = observation(&evidence);
    o.reason = "<script>alert('not executable')</script>".into();
    o.group = "<img src=x onerror=alert(1)>".into();
    o.proposal = redundant(&id);
    o.sequence = SequenceKind::Burst;
    review::record(&job, 0, &id, &bundle, o).unwrap();
    let output = root.path().join("report.html");
    review::write_report(&job, &output, 0, 50).unwrap();
    let report = fs::read_to_string(&output).unwrap();
    assert!(!report.contains("<script>"));
    assert!(report.contains("&lt;script&gt;"));
    assert!(report.contains("data:image/jpeg;base64,"));
    assert!(report.contains("lightroom_current_appearance_unavailable"));
    assert!(report.contains("保留替代"));
    assert!(review::write_report(&job, &output, 0, 50).is_err());
    assert!(!root.path().join("photo.xmp").exists());
    assert_eq!(review::status(&job).unwrap().revision, 1);
}

#[test]
fn checkpoint_survives_reload_and_stale_worker_cannot_overwrite() {
    let root = TempDir::new().unwrap();
    let (job, bundle, evidence, id) = setup(root.path());
    let mut draft = observation(&evidence);
    draft.proposal = Proposal::Retain {
        stars: Some(4),
        green: false,
    };
    let first = review::record(&job, 0, &id, &bundle, draft.clone()).unwrap();
    assert_eq!((first.revision, first.recorded, first.pending), (1, 1, 0));
    assert!(!first.can_apply);
    let bytes = fs::read(job.join("checkpoints/00000000000000000001/record.json")).unwrap();
    assert!(review::record(&job, 0, &id, &bundle, draft.clone()).is_err());
    assert_eq!(
        fs::read(job.join("checkpoints/00000000000000000001/record.json")).unwrap(),
        bytes
    );
    assert_eq!(review::status(&job).unwrap().revision, 1);
    let recovered = review::show(&job, &id).unwrap();
    assert_eq!(recovered.photo_revision, Some(1));
    assert_eq!(recovered.observation.unwrap().proposal, draft.proposal);
    let report = review::check(&job).unwrap();
    assert!(
        report.blockers[&id]
            .iter()
            .any(|v| v == "lightroom_current_appearance_unavailable")
    );
    draft.proposal = Proposal::Leave;
    assert_eq!(
        review::record(&job, 1, &id, &bundle, draft)
            .unwrap()
            .revision,
        2
    );
    assert_eq!(
        review::show(&job, &id)
            .unwrap()
            .observation
            .unwrap()
            .proposal,
        Proposal::Leave
    );
    assert!(!review::check(&job).unwrap().status.can_apply);
    assert!(!root.path().join("photo.xmp").exists());
}

#[test]
fn every_raw_jpeg_companion_is_frozen_even_for_jpeg_only_input() {
    let root = TempDir::new().unwrap();
    let jpg = root.path().join("photo.JPG");
    let raw = root.path().join("photo.DNG");
    fs::write(&raw, include_bytes!("fixtures/native-synthetic.dng")).unwrap();
    image::RgbImage::new(80, 60).save(&jpg).unwrap();
    let xmp = root.path().join("photo.xmp");
    let metadata = b"<human-edits crop='keep' rating='5'/>";
    fs::write(&xmp, metadata).unwrap();
    let job = root.path().join("job");
    assert_eq!(review::create(&[jpg], &job, 0).unwrap().total, 1);
    let page = review::page(&job, 0, 1).unwrap();
    assert!(page.photos[0].primary.ends_with("photo.DNG"));
    let manifest: review::Job =
        serde_json::from_slice(&fs::read(job.join("job.json")).unwrap()).unwrap();
    assert_eq!(manifest.photos[0].assets.len(), 2);
    let bundle = root.path().join("bundle");
    let evidence = evidence::prepare(&raw, &bundle, &[], 0).unwrap();
    // Only the unseen RAW changes: the rendered companion JPEG is unchanged.
    fs::write(&raw, b"changed raw revision").unwrap();
    assert!(
        review::record(
            &job,
            0,
            &page.photos[0].photo_id,
            &bundle,
            observation(&evidence)
        )
        .is_err()
    );
    assert_eq!(review::status(&job).unwrap().revision, 0);
    assert_eq!(fs::read(xmp).unwrap(), metadata);
}

#[test]
fn sidecar_created_or_edited_after_freeze_blocks_observation() {
    let root = TempDir::new().unwrap();
    let (job, bundle, evidence, id) = setup(root.path());
    let xmp = root.path().join("photo.xmp");
    fs::write(&xmp, b"new Lightroom crop").unwrap();
    assert!(review::record(&job, 0, &id, &bundle, observation(&evidence)).is_err());
    assert_eq!(fs::read(xmp).unwrap(), b"new Lightroom crop");
}

#[test]
fn added_companion_invalidates_the_frozen_identity() {
    let root = TempDir::new().unwrap();
    let (job, bundle, evidence, id) = setup(root.path());
    fs::write(
        root.path().join("photo.DNG"),
        include_bytes!("fixtures/native-synthetic.dng"),
    )
    .unwrap();
    assert!(review::record(&job, 0, &id, &bundle, observation(&evidence)).is_err());
}

#[test]
fn history_gap_or_tampering_fails_instead_of_resetting_progress() {
    let root = TempDir::new().unwrap();
    let (job, bundle, evidence, id) = setup(root.path());
    review::record(&job, 0, &id, &bundle, observation(&evidence)).unwrap();
    let path = job.join("checkpoints/00000000000000000001");
    fs::rename(&path, job.join("checkpoints/00000000000000000002")).unwrap();
    assert!(review::status(&job).is_err());
    fs::rename(job.join("checkpoints/00000000000000000002"), &path).unwrap();
    let file = path.join("record.json");
    let mut record: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    record["observation"]["reason"] = "silently changed".into();
    fs::write(file, serde_json::to_vec(&record).unwrap()).unwrap();
    assert!(review::status(&job).is_err());
}

#[test]
fn interrupted_staging_is_ignored_and_job_cannot_be_replaced() {
    let root = TempDir::new().unwrap();
    let (job, _, _, _) = setup(root.path());
    let partial = job.join("checkpoints/.checkpoint-interrupted");
    fs::create_dir(&partial).unwrap();
    fs::write(partial.join("record.json"), b"{partial").unwrap();
    assert_eq!(review::status(&job).unwrap().revision, 0);
    assert!(review::create(&[root.path().join("photo.JPG")], &job, 0).is_err());
    assert_eq!(review::status(&job).unwrap().total, 1);
}

#[test]
fn invented_resized_or_missing_views_do_not_become_focus_proof() {
    let root = TempDir::new().unwrap();
    let (_, _, evidence, _) = setup(root.path());
    let mut o = observation(&evidence);
    o.proposal = Proposal::Red { replacement: None };
    o.technical_failure = Some("Severe subject shake seen in native detail".into());
    assert!(policy::validate(&o, &evidence).unwrap().proposal_valid);
    o.seen[1].width /= 2;
    assert!(policy::validate(&o, &evidence).is_err());
    o = observation(&evidence);
    o.proposal = Proposal::Retain {
        stars: None,
        green: false,
    };
    o.seen.truncate(1);
    assert!(!policy::validate(&o, &evidence).unwrap().proposal_valid);
    o = observation(&evidence);
    o.proposal = Proposal::Retain {
        stars: None,
        green: false,
    };
    o.inspection = Inspection::ModelUnverified;
    assert!(
        policy::validate(&o, &evidence)
            .unwrap()
            .blockers
            .contains(&"native_inspection_unverified".into())
    );
    o.seen[0].id = "invented".into();
    assert!(policy::validate(&o, &evidence).is_err());
}

#[test]
fn stars_green_and_artistic_red_require_the_current_lightroom_appearance() {
    let root = TempDir::new().unwrap();
    let (_, _, evidence, _) = setup(root.path());
    for proposal in [
        Proposal::Retain {
            stars: Some(4),
            green: false,
        },
        Proposal::Retain {
            stars: None,
            green: true,
        },
        Proposal::Red { replacement: None },
    ] {
        let mut o = observation(&evidence);
        o.proposal = proposal;
        let result = policy::validate(&o, &evidence).unwrap();
        assert!(
            result
                .blockers
                .contains(&"lightroom_current_appearance_unavailable".into())
        );
        if matches!(o.proposal, Proposal::Retain { green: true, .. }) {
            assert!(
                result
                    .blockers
                    .contains(&"green_requires_concrete_edit".into())
            );
        }
    }
}

#[test]
fn uncertainty_unique_records_and_stacking_sequences_preserve_frames() {
    let root = TempDir::new().unwrap();
    let (_, _, evidence, _) = setup(root.path());
    let mut o = observation(&evidence);
    o.proposal = Proposal::Red { replacement: None };
    o.technical_failure = Some("Severe shake".into());
    for kind in [
        SequenceKind::AstroStack,
        SequenceKind::Bracket,
        SequenceKind::Panorama,
        SequenceKind::Unknown,
    ] {
        o.sequence = kind;
        assert!(
            policy::validate(&o, &evidence)
                .unwrap()
                .blockers
                .contains(&"protected_or_unknown_sequence".into())
        );
    }
    o.sequence = SequenceKind::Independent;
    o.record_value = true;
    o.uncertain = true;
    let blockers = policy::validate(&o, &evidence).unwrap().blockers;
    assert!(
        blockers.contains(&"unique_record_value".into()) && blockers.contains(&"uncertain".into())
    );
}

#[test]
fn replacements_need_a_better_keeper_and_bursts_need_a_fallback() {
    let root = TempDir::new().unwrap();
    let (_, _, evidence, _) = setup(root.path());
    let mut a = observation(&evidence);
    a.sequence = SequenceKind::Burst;
    a.artistic_score = Some(5);
    a.proposal = Proposal::Red {
        replacement: Some("b".into()),
    };
    assert!(!policy::group_blockers(&[("a", &a)]).unwrap().is_empty());
    let mut b = a.clone();
    b.proposal = Proposal::Retain {
        stars: None,
        green: false,
    };
    assert!(
        !policy::group_blockers(&[("a", &a), ("b", &b)])
            .unwrap()
            .is_empty()
    );
    b.artistic_score = Some(6);
    assert!(
        policy::group_blockers(&[("a", &a), ("b", &b)])
            .unwrap()
            .is_empty()
    );
    // Any number of distinct moments may be kept; no top-N pruning.
    let c = b.clone();
    assert!(
        policy::group_blockers(&[("a", &a), ("b", &b), ("c", &c)])
            .unwrap()
            .is_empty()
    );
    b.uncertain = true;
    assert!(
        !policy::group_blockers(&[("a", &a), ("b", &b)])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn malformed_scores_and_personal_stars_are_rejected() {
    let root = TempDir::new().unwrap();
    let (_, _, evidence, _) = setup(root.path());
    let mut o = observation(&evidence);
    o.artistic_score = Some(11);
    assert!(policy::validate(&o, &evidence).is_err());
    o.artistic_score = Some(5);
    o.proposal = Proposal::Retain {
        stars: Some(2),
        green: false,
    };
    assert!(policy::validate(&o, &evidence).is_err());
    o.proposal = Proposal::Red { replacement: None };
    assert!(
        policy::validate(&o, &evidence)
            .unwrap()
            .blockers
            .contains(&"score_five_requires_better_retained_alternative".into())
    );
}

#[test]
fn cli_progress_is_small_and_does_not_include_history_or_pixels() {
    let root = TempDir::new().unwrap();
    let (job, _, _, _) = setup(root.path());
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_imgcull"))
        .arg("review-status")
        .arg(&job)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stdout.len() < 1024);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["can_apply"], false);
    assert!(json.get("observations").is_none() && json.get("base64").is_none());
    assert!(review::page(&job, 0, 201).is_err());
    assert!(review::show(&job, "unknown-photo").is_err());
    assert!(
        review::show(&job, &review::page(&job, 0, 1).unwrap().photos[0].photo_id)
            .unwrap()
            .observation
            .is_none()
    );
    assert_eq!(review::page(&job, 5, 1).unwrap().photos.len(), 0);
}

#[test]
fn another_process_holding_the_job_lock_prevents_any_progress_write() {
    let root = TempDir::new().unwrap();
    let (job, _, _, _) = setup(root.path());
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(job.join("review.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_imgcull"))
        .arg("review-status")
        .arg(&job)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("locked"));
    drop(lock);
    assert_eq!(review::status(&job).unwrap().revision, 0);
}

#[test]
fn a_saved_review_is_invalidated_by_later_evidence_mutation() {
    let root = TempDir::new().unwrap();
    let (job, bundle, evidence, id) = setup(root.path());
    review::record(&job, 0, &id, &bundle, observation(&evidence)).unwrap();
    fs::write(bundle.join("detail-0000.png"), b"changed detail").unwrap();
    // Progress is history, not a claim of evidence/source validity.
    assert_eq!(review::status(&job).unwrap().recorded, 1);
    assert!(review::check(&job).is_err());
}

#[test]
fn completed_locks_are_released_during_concurrent_process_spawns() {
    let a = TempDir::new().unwrap();
    let b = TempDir::new().unwrap();
    let (job_a, _, _, _) = setup(a.path());
    let (job_b, _, _, _) = setup(b.path());
    let worker = std::thread::spawn(move || {
        for _ in 0..16 {
            let output = std::process::Command::new(env!("CARGO_BIN_EXE_imgcull"))
                .arg("review-status")
                .arg(&job_b)
                .output()
                .unwrap();
            assert!(output.status.success());
        }
    });
    // Only this thread accesses job_a. Child process creation in another thread
    // must not transiently keep a completed lock alive through an inherited FD.
    for _ in 0..1000 {
        assert_eq!(review::status(&job_a).unwrap().revision, 0);
    }
    worker.join().unwrap();
}

#[test]
fn lowering_one_bundle_floor_cannot_bypass_the_frozen_job_requirement() {
    let root = TempDir::new().unwrap();
    let (_, bundle, evidence, _) = setup(root.path());
    let strict = root.path().join("strict-job");
    review::create(
        &[root.path().join("photo.JPG")],
        &strict,
        imgcull::imaging::DEFAULT_MIN_PIXELS,
    )
    .unwrap();
    let id = review::page(&strict, 0, 1).unwrap().photos[0]
        .photo_id
        .clone();
    assert!(review::record(&strict, 0, &id, &bundle, observation(&evidence)).is_err());
    assert_eq!(review::status(&strict).unwrap().revision, 0);
    let mut o = observation(&evidence);
    o.sequence = SequenceKind::AstroStack;
    o.proposal = Proposal::Retain {
        stars: Some(3),
        green: false,
    };
    assert!(
        policy::validate(&o, &evidence)
            .unwrap()
            .blockers
            .contains(&"protected_or_unknown_sequence".into())
    );
}

#[test]
fn ambiguous_physical_companions_abort_before_creating_a_job() {
    let root = TempDir::new().unwrap();
    let jpg = root.path().join("photo.JPG");
    fs::write(&jpg, b"identity only; no decode in discovery").unwrap();
    fs::write(root.path().join("photo.jpeg"), b"another JPEG").unwrap();
    let output = root.path().join("job");
    assert!(review::create(std::slice::from_ref(&jpg), &output, 0).is_err());
    assert!(!output.exists());
    fs::remove_file(root.path().join("photo.jpeg")).unwrap();
    fs::write(root.path().join("photo.NEF"), b"raw one").unwrap();
    fs::write(root.path().join("photo.DNG"), b"raw two").unwrap();
    assert!(review::create(&[jpg], &output, 0).is_err());
    assert!(!output.exists());
}

#[test]
fn a_missing_requested_input_cannot_silently_create_a_partial_job() {
    let root = TempDir::new().unwrap();
    let jpg = root.path().join("valid.JPG");
    fs::write(&jpg, b"identity only").unwrap();
    let job = root.path().join("partial-job");
    let missing = root.path().join("missing-folder");
    let inputs = [jpg, missing];
    assert!(review::create(&inputs, &job, 0).is_err());
    assert!(!job.exists());
}
