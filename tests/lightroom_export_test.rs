use imgcull::{integrity, lightroom::{self, Manifest, Rendition}};
use std::{fs, path::{Path, PathBuf}};
use tempfile::TempDir;

fn fixture(root: &Path) -> (PathBuf, Manifest) {
    let source = root.join("source.JPG");
    image::RgbImage::new(80, 60).save(&source).unwrap();
    let output = root.join("export");
    fs::create_dir(&output).unwrap();
    let file = output.join("render-0001.jpg");
    // Current LR crop legitimately differs from the full source dimensions.
    image::RgbImage::new(40, 30).save(&file).unwrap();
    let manifest = Manifest { schema: 1, exporter: "imgcull-lr-v1".into(),
        catalog: root.join("synthetic.lrcat"), lightroom_version: "mock-sdk".into(), images: vec![Rendition {
            photo_uuid: "synthetic-uuid".into(), local_identifier: "42".into(), is_virtual_copy: false,
            source, source_sha256: String::new(), develop_sha256: "a".repeat(64), last_edit_time: 123.0,
            edit_count: 7, cropped_width: 40, cropped_height: 30,
            file: "render-0001.jpg".into(), sha256: integrity::file_digest(&file).unwrap(),
        }] };
    let mut manifest = manifest;
    manifest.images[0].source_sha256 = integrity::file_digest(&manifest.images[0].source).unwrap();
    save(&output, &manifest);
    (output, manifest)
}
fn save(output: &Path, manifest: &Manifest) {
    fs::write(output.join("manifest.json"), serde_json::to_vec(manifest).unwrap()).unwrap();
}

#[test]
fn full_current_crop_verifies_but_never_authorizes_visual_verdict_or_apply() {
    let root = TempDir::new().unwrap();
    let (output, _) = fixture(root.path());
    let before = fs::read(root.path().join("source.JPG")).unwrap();
    let summary = lightroom::verify(&output).unwrap();
    assert_eq!(summary.images, 1);
    assert!(!summary.can_apply && !summary.live_catalog_verified && !summary.contains_visual_verdict);
    assert_eq!(fs::read(root.path().join("source.JPG")).unwrap(), before);
    assert!(!root.path().join("source.xmp").exists());
}

#[test]
fn changed_original_or_current_render_invalidates_the_export() {
    let root = TempDir::new().unwrap();
    let (output, manifest) = fixture(root.path());
    let source = fs::read(&manifest.images[0].source).unwrap();
    fs::write(&manifest.images[0].source, b"changed original").unwrap();
    assert!(lightroom::verify(&output).is_err());
    fs::write(&manifest.images[0].source, source).unwrap();
    fs::write(output.join("render-0001.jpg"), b"changed rendition").unwrap();
    assert!(lightroom::verify(&output).is_err());
}

#[test]
fn a_valid_hash_for_a_thumbnail_still_fails_full_current_crop_dimensions() {
    let root = TempDir::new().unwrap();
    let (output, mut manifest) = fixture(root.path());
    let file = output.join("render-0001.jpg");
    image::RgbImage::new(8, 6).save(&file).unwrap();
    manifest.images[0].sha256 = integrity::file_digest(&file).unwrap();
    save(&output, &manifest);
    assert!(lightroom::verify(&output).is_err());
}

#[test]
fn traversal_or_an_unpublished_batch_is_rejected() {
    let root = TempDir::new().unwrap();
    let (output, mut manifest) = fixture(root.path());
    manifest.images[0].file = "../source.JPG".into();
    save(&output, &manifest);
    assert!(lightroom::verify(&output).is_err());
    fs::remove_file(output.join("manifest.json")).unwrap();
    assert!(lightroom::verify(&output).is_err());
}

#[test]
fn separate_virtual_copies_preserve_lr_identity_without_deduplicating_files() {
    let root = TempDir::new().unwrap();
    let (output, mut manifest) = fixture(root.path());
    let mut copy = manifest.images[0].clone();
    copy.photo_uuid = "virtual-copy".into();
    copy.local_identifier = "43".into();
    copy.is_virtual_copy = true;
    copy.file = "render-0002.jpg".into();
    fs::copy(output.join("render-0001.jpg"), output.join(&copy.file)).unwrap();
    manifest.images.push(copy);
    save(&output, &manifest);
    assert_eq!(lightroom::verify(&output).unwrap().virtual_copies, 1);
    manifest.images[1].photo_uuid = manifest.images[0].photo_uuid.clone();
    save(&output, &manifest);
    assert!(lightroom::verify(&output).is_err());
}

#[test]
fn unknown_fields_and_bad_dimensions_or_hashes_fail_closed() {
    let root = TempDir::new().unwrap();
    let (output, mut manifest) = fixture(root.path());
    let mut value = serde_json::to_value(&manifest).unwrap();
    value["assume_approved"] = true.into();
    fs::write(output.join("manifest.json"), serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(lightroom::verify(&output).is_err());
    manifest.images[0].cropped_width = 0;
    save(&output, &manifest);
    assert!(lightroom::verify(&output).is_err());
    manifest.images[0].cropped_width = 40;
    manifest.images[0].sha256 = "not-a-hash".into();
    save(&output, &manifest);
    assert!(lightroom::verify(&output).is_err());
}

#[test]
fn cli_exports_compact_material_status_without_approving_photo_quality() {
    let root = TempDir::new().unwrap();
    let (output, _) = fixture(root.path());
    let response = std::process::Command::new(env!("CARGO_BIN_EXE_imgcull")).arg("verify-lightroom-export").arg(output).output().unwrap();
    assert!(response.status.success());
    assert!(response.stdout.len() < 1024);
    let status: serde_json::Value = serde_json::from_slice(&response.stdout).unwrap();
    assert_eq!(status["contains_visual_verdict"], false);
    assert!(status.get("images").unwrap().is_number());
}

#[cfg(unix)]
#[test]
fn lua_sdk_mock_export_matches_the_rust_contract_in_ci() {
    if std::env::var_os("CI").is_some() {
        let path = std::env::var_os("IMGCULL_LR_FIXTURE").expect("CI must generate Lua cross-language fixture");
        assert_eq!(lightroom::verify(Path::new(&path)).unwrap().images, 1);
    }
}
