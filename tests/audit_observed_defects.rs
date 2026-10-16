//! Investigation probes for upstream 0.2.8. These assert OBSERVED DEFECTS,
//! not desired production behavior. Keep on the audit branch; replace with
//! safety regression assertions when each defect is fixed.
use imgcull::scoring::{ScoringResult, score_to_stars};
use imgcull::xmp::{XmpSidecar, backup_sidecar};
use tempfile::TempDir;

fn packet(body: &str, attrs: &str) -> String {
    format!(r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" {attrs}>{body}</rdf:Description></rdf:RDF></x:xmpmeta>"#)
}

#[test]
fn probe_self_closing_description_places_attributes_after_slash() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("p.xmp");
    let base = packet("", r#"crs:Exposure2012="1.0""#)
        .replace(r#"crs:Exposure2012="1.0"></rdf:Description>"#, r#"crs:Exposure2012="1.0"/>"#);
    std::fs::write(&path, base).unwrap();
    let mut sidecar = XmpSidecar::read(&path).unwrap();
    sidecar.set_rating(4);
    sidecar.write(&path).unwrap();
    let output = std::fs::read_to_string(path).unwrap();
    assert!(output.contains("/\n      xmp:Rating="), "{output}");
    assert!(!output.contains("</rdf:Description>"));
}

#[test]
fn probe_single_quoted_rating_is_unread_and_duplicated() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("p.xmp");
    std::fs::write(&path, packet("", "xmp:Rating='5'")).unwrap();
    let mut sidecar = XmpSidecar::read(&path).unwrap();
    assert_eq!(sidecar.rating(), None);
    sidecar.set_rating(3);
    sidecar.write(&path).unwrap();
    let output = std::fs::read_to_string(path).unwrap();
    assert_eq!(output.matches("xmp:Rating=").count(), 2);
}

#[test]
fn probe_escaped_caption_is_double_escaped_on_unrelated_update() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("p.xmp");
    std::fs::write(&path, packet(
        r#"<dc:description><rdf:Alt><rdf:li xml:lang="x-default">A &amp; B</rdf:li></rdf:Alt></dc:description>"#, "")).unwrap();
    let mut sidecar = XmpSidecar::read(&path).unwrap();
    sidecar.set_original_filename("p.jpg");
    sidecar.write(&path).unwrap();
    assert!(std::fs::read_to_string(path).unwrap().contains("A &amp;amp; B"));
}

#[test]
fn probe_stale_snapshot_overwrites_intervening_develop_edit() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("p.xmp");
    std::fs::write(&path, packet("", r#"crs:Exposure2012="1.0""#)).unwrap();
    let mut sidecar = XmpSidecar::read(&path).unwrap();
    std::fs::write(&path, packet("", r#"crs:Exposure2012="2.0""#)).unwrap();
    sidecar.set_original_filename("p.jpg");
    sidecar.write(&path).unwrap();
    let output = std::fs::read_to_string(path).unwrap();
    assert!(output.contains(r#"crs:Exposure2012="1.0""#));
    assert!(!output.contains(r#"crs:Exposure2012="2.0""#));
}

#[test]
fn probe_second_backup_replaces_first_backup() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("p.xmp");
    std::fs::write(&path, "original").unwrap();
    backup_sidecar(&path).unwrap();
    std::fs::write(&path, "modified").unwrap();
    backup_sidecar(&path).unwrap();
    assert_eq!(std::fs::read_to_string(path.with_extension("xmp.bak")).unwrap(), "modified");
}

#[test]
fn probe_non_score_json_can_become_persistent_one_star_score() {
    let scores: ScoringResult = serde_json::from_str(r#"{"message":"unable to judge"}"#).unwrap();
    assert_eq!(scores.overall_score(), 0.0);
    assert_eq!(score_to_stars(scores.overall_score()), 1);
    let mut sidecar = XmpSidecar::new();
    sidecar.set_scores(&scores, scores.overall_score(), "test/model");
    assert!(sidecar.has_scores());
}

#[test]
fn probe_model_identifier_is_not_xml_escaped() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("p.xmp");
    let mut sidecar = XmpSidecar::new();
    sidecar.set_scores(&ScoringResult::default(), 0.5, "local/a&b");
    sidecar.write(&path).unwrap();
    assert!(std::fs::read_to_string(path).unwrap().contains("<imgcull:scored_by>local/a&b</imgcull:scored_by>"));
}
