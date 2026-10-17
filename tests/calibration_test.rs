use imgcull::{calibration::{self, Expected, Predictions, Profile, Split}, policy::{AlternativeQuality, Comparison, ContentRelation, Proposal}, review};
use std::{fs, path::{Path, PathBuf}, process::Command};
use tempfile::TempDir;

fn profile(root: &Path) -> (PathBuf, Profile) {
    let input = root.join("draft.json");
    fs::write(&input, include_str!("../examples/calibration-draft.json")).unwrap();
    let output = root.join("profile.json");
    calibration::publish(&input, &output).unwrap();
    let profile = calibration::load(&output).unwrap();
    (output, profile)
}

fn proposal(expected: &Expected) -> Proposal {
    match expected.action.as_str() {
        "retain" => Proposal::Retain { stars: expected.stars, green: expected.green },
        "leave" => Proposal::Leave,
        "red" => Proposal::Red { replacement: expected.keeper.clone() },
        "redundant" => Proposal::Redundant { comparison: Comparison { keeper: expected.keeper.clone().unwrap(), content: ContentRelation::Equivalent, quality: AlternativeQuality::Equivalent, reason: "Equivalent useful content with sound retained alternative".into() } },
        _ => panic!("invalid expectation"),
    }
}

fn predictions(profile: &Profile, split: Split) -> Predictions {
    Predictions { profile_id: profile.profile_id.clone(), reviewer: "synthetic-contract-not-a-model-benchmark".into(), cases: profile.cases.iter().filter(|c| c.split == split).map(|c| (c.id.clone(), c.frames.iter().map(|f| (f.id.clone(), proposal(&f.expected))).collect())).collect() }
}

#[test]
fn profiles_are_immutable_and_mutation_is_detected() {
    let root = TempDir::new().unwrap();
    let (path, p) = profile(root.path());
    let bytes = fs::read(&path).unwrap();
    assert!(calibration::publish(&root.path().join("draft.json"), &path).is_err());
    assert_eq!(bytes, fs::read(&path).unwrap());
    let mut changed = p;
    changed.rules[0].text = "Changed expectation".into();
    fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
    assert!(calibration::load(&path).is_err());
}

#[test]
fn context_is_bounded_tagged_and_excludes_validation_answers() {
    let root = TempDir::new().unwrap();
    let (_, p) = profile(root.path());
    let context = calibration::context(&p, &["food".into(), "near_duplicate".into()]).unwrap();
    assert_eq!(context.profile_id, p.profile_id);
    assert!(!context.can_apply);
    assert!(context.examples.iter().any(|c| c.id == "table-equivalent"));
    assert!(context.examples.iter().all(|c| c.split == Split::Teaching));
    assert!(!serde_json::to_string(&context).unwrap().contains("equivalent-landscape"));
    assert!(calibration::context(&p, &[]).is_err());
    assert!(calibration::context(&p, &["bird".into(), "bird".into()]).is_err());
    let mut too_broad = p.clone();
    for i in 0..9 {
        let mut case = p.cases[0].clone();
        case.id = format!("extra-{i}");
        too_broad.cases.push(case);
    }
    assert!(calibration::context(&too_broad, &["food".into()]).is_err());
}

#[test]
fn evaluation_counts_false_red_and_missing_redundancy_separately() {
    let root = TempDir::new().unwrap();
    let (_, p) = profile(root.path());
    let mut run = predictions(&p, Split::Validation);
    let pass = calibration::evaluate(&p, &run, Split::Validation).unwrap();
    assert!(pass.passed && !pass.can_apply);
    assert_eq!(pass.total, 6);
    // A stack false-red is not hidden by correctly choosing the landscape keeper.
    run.cases.get_mut("protected-astro").unwrap().insert("a".into(), Proposal::Red { replacement: None });
    run.cases.get_mut("equivalent-landscape").unwrap().insert("b".into(), Proposal::Leave);
    let failed = calibration::evaluate(&p, &run, Split::Validation).unwrap();
    assert!(!failed.passed);
    assert_eq!((failed.false_red, failed.missed_redundancy, failed.mismatches.len()), (1, 1, 2));
}

#[test]
fn evaluation_rejects_omissions_wrong_profile_and_unknown_frames() {
    let root = TempDir::new().unwrap();
    let (_, p) = profile(root.path());
    let mut run = predictions(&p, Split::Validation);
    run.profile_id = "wrong".into();
    assert!(calibration::evaluate(&p, &run, Split::Validation).is_err());
    run = predictions(&p, Split::Validation);
    run.cases.remove("protected-astro");
    assert!(calibration::evaluate(&p, &run, Split::Validation).is_err());
    run = predictions(&p, Split::Validation);
    let frames = run.cases.get_mut("protected-astro").unwrap();
    frames.remove("b");
    frames.insert("unknown".into(), Proposal::Leave);
    assert!(calibration::evaluate(&p, &run, Split::Validation).is_err());
}

#[test]
fn matching_label_with_unsupported_comparison_does_not_pass() {
    let root = TempDir::new().unwrap();
    let (_, p) = profile(root.path());
    let mut run = predictions(&p, Split::Teaching);
    let Proposal::Redundant { comparison } = run.cases.get_mut("table-equivalent").unwrap().get_mut("b").unwrap() else { panic!() };
    comparison.quality = AlternativeQuality::Unknown;
    let result = calibration::evaluate(&p, &run, Split::Teaching).unwrap();
    assert!(!result.passed);
    assert!(!result.mismatches[0].comparison_supported);
}

#[test]
fn malformed_correction_and_keeper_cycles_cannot_be_published() {
    let root = TempDir::new().unwrap();
    let input = root.path().join("bad.json");
    let mut p: Profile = serde_json::from_str(include_str!("../examples/calibration-draft.json")).unwrap();
    p.cases[0].frames[0].expected = Expected { action: "redundant".into(), keeper: Some("b".into()), stars: None, green: false };
    fs::write(&input, serde_json::to_vec(&p).unwrap()).unwrap();
    assert!(calibration::publish(&input, &root.path().join("bad-profile.json")).is_err());
    assert!(!root.path().join("bad-profile.json").exists());
    let bytes = include_str!("../examples/calibration-draft.json").replace("\"lesson\":", "\"misspelled_lesson\":");
    fs::write(&input, bytes).unwrap();
    assert!(calibration::publish(&input, &root.path().join("bad-profile.json")).is_err());
}

#[test]
fn existing_job_embeds_exact_preferences_and_does_not_follow_external_edits() {
    let root = TempDir::new().unwrap();
    let (path, p) = profile(root.path());
    let photo = root.path().join("source.jpg");
    image::RgbImage::new(32, 24).save(&photo).unwrap();
    let job = root.path().join("job");
    review::create_with_profile(std::slice::from_ref(&photo), &job, 0, Some(&path)).unwrap();
    fs::write(&path, b"new external preferences must not change old job").unwrap();
    let context = review::context(&job, &["food".into()]).unwrap();
    assert_eq!(context.profile_id, p.profile_id);
    assert_eq!(review::status(&job).unwrap().profile_id, Some(p.profile_id));
    assert!(!photo.with_extension("xmp").exists());
    let no_profile = root.path().join("without-profile");
    review::create(&[photo], &no_profile, 0).unwrap();
    assert!(review::context(&no_profile, &["food".into()]).is_err());
}

#[test]
fn cli_evaluation_emits_report_and_fails_the_process_on_regression() {
    let root = TempDir::new().unwrap();
    let (path, p) = profile(root.path());
    let mut run = predictions(&p, Split::Validation);
    run.cases.get_mut("protected-astro").unwrap().insert("a".into(), Proposal::Red { replacement: None });
    let input = root.path().join("run.json");
    fs::write(&input, serde_json::to_vec(&run).unwrap()).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_imgcull")).arg("calibration-evaluate").arg(path).arg(input).output().unwrap();
    assert!(!result.status.success());
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["false_red"], 1);
    assert_eq!(report["passed"], false);
    assert_eq!(report["can_apply"], false);
}

#[test]
fn unknown_prediction_fields_are_not_silently_accepted() {
    let data = r#"{"profile_id":"x","reviewer":"model","cases":{},"pretend_native":true}"#;
    assert!(serde_json::from_str::<Predictions>(data).is_err());
}
