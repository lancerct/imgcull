use imgcull::{evidence, similarity};
use std::{fs, path::{Path, PathBuf}};
use tempfile::TempDir;

fn bundle(root: &Path, name: &str, reverse: bool) -> PathBuf {
    let source = root.join(format!("{name}.jpg"));
    image::RgbImage::from_fn(90, 80, |x,_| {
        let v = if reverse { 250-x*2 } else { x*2 } as u8;
        image::Rgb([v,v,v])
    }).save(&source).unwrap();
    let output = root.join(name);
    evidence::prepare(&source, &output, &[], 0).unwrap();
    output
}

#[test]
fn identical_content_is_a_pair_hint_not_a_keeper_or_deletion() {
    let root = TempDir::new().unwrap();
    let a = bundle(root.path(), "a", false);
    let b = bundle(root.path(), "b", false);
    let c = bundle(root.path(), "c", true);
    let suggestions = similarity::suggest(&[a,b,c], 0).unwrap();
    assert_eq!(suggestions.photos, 3);
    assert_eq!(suggestions.pairs.len(), 1);
    assert_eq!(suggestions.pairs[0].distance, 0);
    assert!(!suggestions.can_apply && !suggestions.contains_visual_verdict);
    assert!(!root.path().join("a.xmp").exists());
}

#[test]
fn duplicate_identity_and_corrupt_evidence_cannot_form_candidates() {
    let root = TempDir::new().unwrap();
    let a = bundle(root.path(), "a", false);
    let b = bundle(root.path(), "b", false);
    assert!(similarity::suggest(&[a.clone(),a.clone()], 6).is_err());
    let view = evidence::verify(&b).unwrap().views[0].file.clone();
    fs::write(b.join(view), b"corrupt").unwrap();
    assert!(similarity::suggest(&[a,b], 6).is_err());
}

#[test]
fn similarity_is_bounded_and_does_not_hide_an_incomplete_window() {
    let root = TempDir::new().unwrap();
    let a = bundle(root.path(), "a", false);
    assert!(similarity::suggest(std::slice::from_ref(&a), 6).is_err());
    assert!(similarity::suggest(&vec![a.clone();65], 6).is_err());
    assert!(similarity::suggest(&[a.clone(),a.clone()], 13).is_err());
    assert!(similarity::suggest(&[a,root.path().join("missing")], 6).is_err());
}
