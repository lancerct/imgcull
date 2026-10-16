//! Regression tests for data loss, stale writes, pairing, and recovery.
use imgcull::scoring::ScoringResult;
use imgcull::xmp::{XmpSidecar, backup_sidecar};
use imgcull::{discovery, metadata_transaction};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
const HEADER: &str = r#"<m xmlns:r="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:p="http://ns.adobe.com/xap/1.0/" xmlns:c="http://ns.adobe.com/camera-raw-settings/1.0/" xmlns:d="http://purl.org/dc/elements/1.1/"><r:RDF>"#;
fn seed(xml: &str) -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("photo.xmp");
    fs::write(&path, format!("{HEADER}{xml}</r:RDF></m>")).unwrap();
    (dir, path)
}
fn transaction(path: &Path) -> PathBuf {
    let root = path.parent().unwrap().join(".imgcull/transactions");
    fs::read_dir(root)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
        .join("transaction.json")
}
#[test]
fn self_closing_description_and_alternate_prefixes_work() {
    let (_dir, p) = seed("<r:Description p:Rating='5' c:Exposure2012='+1.25'/>");
    let mut s = XmpSidecar::read(&p).unwrap();
    assert_eq!(s.rating(), Some(5));
    s.set_rating(3);
    s.set_label("Green");
    s.write(&p).unwrap();
    let raw = fs::read_to_string(&p).unwrap();
    roxmltree::Document::parse(&raw).unwrap();
    assert!(raw.contains("c:Exposure2012='+1.25'"));
    assert!(!raw.contains(" p:Rating="));
    let back = XmpSidecar::read(&p).unwrap();
    assert_eq!(back.rating(), Some(3));
    assert_eq!(back.label(), Some("Green"));
}
#[test]
fn element_rating_replaced_without_duplicate() {
    let (_dir, p) = seed("<r:Description><p:Rating>4</p:Rating></r:Description>");
    let mut s = XmpSidecar::read(&p).unwrap();
    assert_eq!(s.rating(), Some(4));
    s.set_rating(5);
    s.write(&p).unwrap();
    assert_eq!(XmpSidecar::read(&p).unwrap().rating(), Some(5));
}
#[test]
fn untouched_caption_keywords_masks_and_entities_are_byte_exact() {
    let original = r#"<r:Description p:Rating="-1" p:Label="Red" c:Exposure2012="+2"><d:description><r:Alt><r:li xml:lang="x-default">A &amp; B</r:li><r:li xml:lang="zh-CN">保留标题</r:li></r:Alt></d:description><d:subject><r:Bag><r:li>鸟 &amp; 水</r:li></r:Bag></d:subject><c:Mask><r:Description c:Name="保留蒙版"/></c:Mask></r:Description>"#;
    let (_dir, p) = seed(original);
    let mut s = XmpSidecar::read(&p).unwrap();
    assert_eq!(s.description(), Some("A & B"));
    assert_eq!(s.keywords(), &["鸟 & 水"]);
    assert!(s.has_rating());
    s.set_original_filename("a&b.NEF");
    s.write(&p).unwrap();
    let raw = fs::read_to_string(&p).unwrap();
    assert!(raw.contains(&original.replace("</r:Description>", "")));
    assert!(!raw.contains("amp;amp"));
    assert_eq!(
        XmpSidecar::read(&p).unwrap().original_filename(),
        Some("a&b.NEF")
    );
}
#[test]
fn malformed_and_ambiguous_xmp_are_rejected() {
    for xml in [
        "<r:Description/><r:Description/>",
        "<r:Description p:Rating='3'><p:Rating>4</p:Rating></r:Description>",
        "<r:Description>",
    ] {
        let (_d, p) = seed(xml);
        assert!(XmpSidecar::read(&p).is_err());
    }
}
#[test]
fn failed_validation_does_not_create_file() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("x.xmp");
    let mut s = XmpSidecar::new();
    s.set_rating(9);
    assert!(s.write(&p).is_err());
    assert!(!p.exists());
    s.set_rating(4);
    s.set_scores(&ScoringResult::default(), 0.0, "test");
    assert!(s.write(&p).is_err());
    assert!(!p.exists());
}

#[test]
fn unmodified_read_write_changes_no_bytes_and_creates_no_transaction() {
    let (_dir, p) = seed("<r:Description p:Rating='4'/>");
    let before = fs::read(&p).unwrap();
    XmpSidecar::read(&p).unwrap().write(&p).unwrap();
    assert_eq!(fs::read(&p).unwrap(), before);
    assert!(!p.parent().unwrap().join(".imgcull/transactions").exists());
}
#[test]
fn stale_write_preserves_lightroom_changes() {
    let (_d, p) = seed("<r:Description c:Exposure2012='1'/>");
    let mut stale = XmpSidecar::read(&p).unwrap();
    stale.set_rating(4);
    let newer = fs::read_to_string(&p).unwrap().replace("='1'", "='2'");
    fs::write(&p, &newer).unwrap();
    assert!(stale.write(&p).is_err());
    assert_eq!(fs::read_to_string(&p).unwrap(), newer);
}
#[test]
fn new_sidecar_cannot_replace_unread_existing_file() {
    let (_d, p) = seed("<r:Description/>");
    let before = fs::read(&p).unwrap();
    let mut s = XmpSidecar::new();
    s.set_rating(4);
    assert!(s.write(&p).is_err());
    assert_eq!(fs::read(&p).unwrap(), before);
}
#[test]
fn model_name_is_xml_escaped() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("x.xmp");
    let mut s = XmpSidecar::new();
    let scores:ScoringResult=serde_json::from_str(r#"{"sharpness":0.9,"exposure":0.8,"composition":0.7,"subject_clarity":0.8,"aesthetics":0.9}"#).unwrap();
    s.set_scores(&scores, scores.overall_score(), "a&b<model>");
    s.write(&p).unwrap();
    assert_eq!(
        XmpSidecar::read(&p).unwrap().scored_by(),
        Some("a&b<model>")
    );
}
#[test]
fn backups_are_immutable_and_rollback_is_guarded() {
    let (_d, p) = seed("<r:Description/>");
    let before = fs::read(&p).unwrap();
    backup_sidecar(&p).unwrap();
    let mut s = XmpSidecar::read(&p).unwrap();
    s.set_rating(4);
    s.write(&p).unwrap();
    backup_sidecar(&p).unwrap();
    assert_eq!(fs::read(p.with_extension("xmp.bak")).unwrap(), before);
    let journal = transaction(&p);
    let applied = fs::read(&p).unwrap();
    fs::write(&p, b"later edit").unwrap();
    assert!(metadata_transaction::rollback(&journal).is_err());
    assert_eq!(fs::read(&p).unwrap(), b"later edit");
    fs::write(&p, &applied).unwrap();
    metadata_transaction::rollback(&journal).unwrap();
    assert_eq!(fs::read(&p).unwrap(), before);
}
#[test]
fn new_file_rollback_removes_only_its_own_output() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("new.xmp");
    let mut s = XmpSidecar::new();
    s.set_label("Red");
    s.write(&p).unwrap();
    metadata_transaction::rollback(&transaction(&p)).unwrap();
    assert!(!p.exists());
}
#[test]
fn backup_failure_blocks_replacement() {
    let (_d, p) = seed("<r:Description/>");
    let before = fs::read(&p).unwrap();
    fs::write(p.parent().unwrap().join(".imgcull"), b"not a directory").unwrap();
    let mut s = XmpSidecar::read(&p).unwrap();
    s.set_rating(4);
    assert!(s.write(&p).is_err());
    assert_eq!(fs::read(&p).unwrap(), before);
}
#[test]
fn same_stem_pair_and_duplicate_inputs_have_one_identity() {
    let dir = TempDir::new().unwrap();
    let jpg = dir.path().join("a.JPG");
    let raw = dir.path().join("a.NEF");
    fs::write(&jpg, b"jpg").unwrap();
    fs::write(&raw, b"raw").unwrap();
    let found = discovery::deduplicate_images(vec![jpg.clone(), raw.clone(), jpg.clone()]).unwrap();
    assert_eq!(found, vec![raw.canonicalize().unwrap()]);
    assert_eq!(
        discovery::review_source(&found[0]),
        jpg.canonicalize().unwrap()
    );
}
#[test]
fn ambiguous_raw_collision_is_not_guessed() {
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.NEF");
    let b = dir.path().join("a.DNG");
    fs::write(&a, b"raw").unwrap();
    fs::write(&b, b"raw").unwrap();
    assert!(discovery::deduplicate_images(vec![a, b]).is_err());
}
#[test]
fn incomplete_out_of_range_and_nonfinite_scores_abstain() {
    for json in [r#"{}"#, r#"{"message":"unable"}"#, r#"{"sharpness":0.5}"#] {
        let s: ScoringResult = serde_json::from_str(json).unwrap();
        assert!(s.validate().is_err());
    }
    let mut s = ScoringResult {
        sharpness: Some(0.8),
        exposure: Some(0.8),
        composition: Some(0.8),
        subject_clarity: Some(0.8),
        aesthetics: Some(0.8),
        ..Default::default()
    };
    assert!(s.validate().is_ok());
    s.sharpness = Some(f64::NAN);
    assert!(s.validate().is_err());
    s.sharpness = Some(1.1);
    assert!(s.validate().is_err());
}
#[test]
fn imgcull_lock_prevents_second_writer() {
    let (_d, p) = seed("<r:Description/>");
    let locks = p.parent().unwrap().join(".imgcull/locks");
    fs::create_dir_all(&locks).unwrap();
    let lock = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(locks.join("photo.xmp.lock"))
        .unwrap();
    lock.lock().unwrap();
    let mut s = XmpSidecar::read(&p).unwrap();
    s.set_rating(4);
    assert!(s.write(&p).is_err());
}

#[test]
fn recovery_cannot_target_photo_pixels() {
    let dir = TempDir::new().unwrap();
    let photo = dir.path().join("photo.NEF");
    fs::write(&photo, b"original photo").unwrap();
    assert!(metadata_transaction::write(&photo, Some(b"original photo"), b"replacement").is_err());
    assert_eq!(fs::read(&photo).unwrap(), b"original photo");
}
