use std::fs;
use std::path::{Path, PathBuf};
use image::{GenericImageView, metadata::Orientation};
use imgcull::{evidence::{self, DetailRequest, DetailRole}, imaging::{NativePhoto, Region}, integrity};
use tempfile::TempDir;

#[test]
fn unsupported_blackiszero_raw_is_an_error_not_a_panic_or_preview() {
    let dir=TempDir::new().unwrap(); let raw=dir.path().join("unsupported.DNG");
    let mut bytes=include_bytes!("fixtures/native-synthetic.dng").to_vec();
    let count=u16::from_le_bytes([bytes[8],bytes[9]]) as usize;
    for entry in bytes[10..10+12*count].chunks_exact_mut(12) {
        if u16::from_le_bytes([entry[0],entry[1]])==262 { entry[8..10].copy_from_slice(&1u16.to_le_bytes()); }
    }
    fs::write(&raw,bytes).unwrap();
    let result=std::panic::catch_unwind(|| NativePhoto::load(&raw,0));
    assert!(result.is_ok()); assert!(result.unwrap().is_err());
    let out=dir.path().join("failed-evidence"); assert!(evidence::prepare(&raw,&out,&[],0).is_err()); assert!(!out.exists());
}

fn jpeg(dir: &Path, w: u32, h: u32) -> PathBuf {
    let path = dir.join("photo.JPG");
    image::RgbImage::from_fn(w,h,|x,y| image::Rgb([(x%251) as u8,(y%241) as u8,((x+y)%239) as u8])).save(&path).unwrap();
    path
}
fn focus(region: Region) -> DetailRequest {
    DetailRequest { id: "subject-focus".into(), role: DetailRole::Focus, region }
}
fn orientation_jpeg(path: &Path, orientation: u8) {
    let bytes=fs::read(path).unwrap();
    let tiff=[b'I',b'I',42,0,8,0,0,0,1,0,0x12,1,3,0,1,0,0,0,orientation,0,0,0,0,0,0,0];
    let mut payload=b"Exif\0\0".to_vec(); payload.extend_from_slice(&tiff);
    let mut result=bytes[..2].to_vec(); result.extend_from_slice(&[255,225]);
    result.extend_from_slice(&((payload.len()+2) as u16).to_be_bytes()); result.extend(payload); result.extend_from_slice(&bytes[2..]);
    fs::write(path,result).unwrap();
}

#[test]
fn native_45mp_decode_and_lossless_detail_preserve_original_pixels() {
    let dir=TempDir::new().unwrap(); let path=dir.path().join("photo.JPG");
    let mut image=image::RgbImage::from_pixel(8256,5504,image::Rgb([63,127,191]));
    for y in 5360..5500 { for x in 8080..8230 {
        image.put_pixel(x,y,image::Rgb([(x%251) as u8,(y%241) as u8,((x+y)%239) as u8]));
    }}
    image.save(&path).unwrap();
    let original_hash=integrity::file_digest(&path).unwrap();
    let photo=NativePhoto::load(&path,45_000_000).unwrap();
    assert_eq!((photo.info.width,photo.info.height),(8256,5504));
    let region=Region { x:8100,y:5400,width:100,height:80 };
    let output=dir.path().join("evidence");
    drop(photo); // Keep peak memory bounded on CI before the second end-to-end decode.
    let manifest=evidence::prepare(&path,&output,&[focus(region)],45_000_000).unwrap();
    let detail=image::open(output.join("detail-0000.png")).unwrap().to_rgb8();
    let expected=image::open(&path).unwrap().crop_imm(region.x,region.y,region.width,region.height).to_rgb8();
    assert_eq!(detail,expected);
    assert_eq!(manifest.focus_ids(),vec!["subject-focus"]);
    assert_eq!(evidence::verify(&output).unwrap().evidence_id,manifest.evidence_id);
    assert_eq!(integrity::file_digest(&path).unwrap(),original_hash);
    assert!(!path.with_extension("xmp").exists());
}
#[test]
fn exif_orientation_is_applied_once_and_overview_has_no_exif() {
    let dir=TempDir::new().unwrap(); let path=jpeg(dir.path(),120,80); orientation_jpeg(&path,6);
    let photo=NativePhoto::load(&path,0).unwrap();
    assert_eq!((photo.info.encoded_width,photo.info.encoded_height),(120,80));
    assert_eq!((photo.info.width,photo.info.height),(80,120));
    let mut expected=image::open(&path).unwrap(); expected.apply_orientation(Orientation::Rotate90);
    let region=Region {x:3,y:12,width:20,height:15};
    assert_eq!(photo.detail(region).unwrap().to_rgb8(),expected.crop_imm(3,12,20,15).to_rgb8());
    let result=imgcull::preprocessing::preprocess_image(&path).unwrap();
    use base64::Engine;
    let bytes=base64::prelude::BASE64_STANDARD.decode(result.base64).unwrap();
    assert!(!bytes.windows(6).any(|w| w==b"Exif\0\0"));
    assert_eq!(image::load_from_memory(&bytes).unwrap().dimensions(),(80,120));
}
#[test]
fn raw_without_preview_is_fully_developed_and_oriented() {
    let dir=TempDir::new().unwrap(); let raw=dir.path().join("photo.DNG");
    fs::write(&raw,include_bytes!("fixtures/native-synthetic.dng")).unwrap();
    let photo=NativePhoto::load(&raw,600).unwrap();
    assert_eq!(photo.info.decoding,"full-raw-srgb");
    assert_eq!((photo.info.encoded_width,photo.info.encoded_height),(32,24));
    assert_eq!((photo.info.width,photo.info.height),(24,32));
    let pixels=photo.detail(Region{x:0,y:0,width:24,height:32}).unwrap().to_rgb16();
    assert!(pixels.pixels().any(|p| p.0[0]>0));
    assert_ne!(pixels.get_pixel(6,6),pixels.get_pixel(6,18));
}
#[test]
fn undersized_jpeg_companion_falls_back_to_full_raw() {
    let dir=TempDir::new().unwrap(); let raw=dir.path().join("photo.DNG");
    fs::write(&raw,include_bytes!("fixtures/native-synthetic.dng")).unwrap();
    jpeg(dir.path(),8,8);
    let photo=NativePhoto::load(&raw,600).unwrap();
    assert_eq!(photo.info.source,raw.canonicalize().unwrap()); assert_eq!(photo.info.decoding,"full-raw-srgb");
}

#[test]
fn raw_bundle_keeps_16bit_details_with_a_valid_8bit_jpeg_overview() {
    let dir=TempDir::new().unwrap(); let raw=dir.path().join("photo.DNG");
    fs::write(&raw,include_bytes!("fixtures/native-synthetic.dng")).unwrap();
    let output=dir.path().join("raw-evidence");
    evidence::prepare(&raw,&output,&[focus(Region{x:0,y:0,width:20,height:20})],600).unwrap();
    assert_eq!(image::open(output.join("detail-0000.png")).unwrap().color().bits_per_pixel(),48);
    assert_eq!(image::open(output.join("overview.jpg")).unwrap().dimensions(),(24,32));
    evidence::verify(&output).unwrap();
}

#[test]
fn pair_has_one_evidence_identity_regardless_of_entry_extension() {
    let dir=TempDir::new().unwrap(); let raw=dir.path().join("photo.DNG");
    fs::write(&raw,include_bytes!("fixtures/native-synthetic.dng")).unwrap();
    let jpg=jpeg(dir.path(),80,60);
    let first=evidence::prepare(&raw,&dir.path().join("from-raw"),&[],0).unwrap();
    let second=evidence::prepare(&jpg,&dir.path().join("from-jpg"),&[],0).unwrap();
    assert_eq!(first.render.identity,second.render.identity);
    assert_eq!(first.evidence_id,second.evidence_id);
}
#[test]
fn minimum_pixels_never_upsamples_or_fakes_45mp() {
    let dir=TempDir::new().unwrap(); let path=jpeg(dir.path(),80,60); let out=dir.path().join("bundle");
    assert!(evidence::prepare(&path,&out,&[],45_000_000).is_err()); assert!(!out.exists());
}
#[test]
fn invalid_regions_duplicate_ids_and_traversal_are_rejected_without_partial_bundle() {
    let dir=TempDir::new().unwrap(); let path=jpeg(dir.path(),128,96);
    for (index,region) in [
        Region{x:0,y:0,width:0,height:1},Region{x:u32::MAX,y:0,width:2,height:2},
        Region{x:127,y:95,width:2,height:2},Region{x:0,y:0,width:1025,height:1}
    ].into_iter().enumerate() {
        let out=dir.path().join(format!("invalid-{index}"));
        assert!(evidence::prepare(&path,&out,&[focus(region)],0).is_err()); assert!(!out.exists());
    }
    let request=focus(Region{x:0,y:0,width:10,height:10});
    assert!(evidence::prepare(&path,&dir.path().join("duplicate"),&[request.clone(),request],0).is_err());
    let mut request=focus(Region{x:0,y:0,width:10,height:10}); request.id="../escape".into();
    assert!(evidence::prepare(&path,&dir.path().join("unsafe"),&[request],0).is_err());
}
#[test]
fn overview_only_is_not_focus_evidence_and_existing_bundle_is_immutable() {
    let dir=TempDir::new().unwrap(); let path=jpeg(dir.path(),80,60); let out=dir.path().join("bundle");
    let manifest=evidence::prepare(&path,&out,&[],0).unwrap(); assert!(manifest.focus_ids().is_empty());
    let before=fs::read(out.join("evidence.json")).unwrap();
    assert!(evidence::prepare(&path,&out,&[],0).is_err()); assert_eq!(fs::read(out.join("evidence.json")).unwrap(),before);
}
#[test]
fn source_view_and_manifest_changes_invalidate_reuse() {
    let dir=TempDir::new().unwrap(); let path=jpeg(dir.path(),80,60); let out=dir.path().join("bundle");
    evidence::prepare(&path,&out,&[],0).unwrap(); let view=out.join("overview.jpg"); let before=fs::read(&view).unwrap();
    fs::write(&view,b"broken").unwrap(); assert!(evidence::verify(&out).is_err()); fs::write(&view,before).unwrap();
    let source=fs::read(&path).unwrap(); fs::write(&path,b"changed").unwrap();
    assert!(evidence::verify(&out).is_err()); fs::write(&path,source).unwrap();
    let mut manifest:serde_json::Value=serde_json::from_slice(&fs::read(out.join("evidence.json")).unwrap()).unwrap();
    manifest["render"]["width"]=serde_json::json!(1);
    fs::write(out.join("evidence.json"),serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(evidence::verify(&out).is_err());
}
#[test]
fn detail_role_is_preserved_without_inventing_a_focus_verdict() {
    let dir=TempDir::new().unwrap(); let path=jpeg(dir.path(),80,60); let out=dir.path().join("bundle");
    let mut request=focus(Region{x:0,y:0,width:20,height:20}); request.role=DetailRole::Edge;
    let manifest=evidence::prepare(&path,&out,&[request],0).unwrap();
    assert!(manifest.focus_ids().is_empty()); evidence::verify(&out).unwrap();
}
#[test]
fn cli_prepares_and_verifies_without_provider_setup_or_xmp_writes() {
    let dir=TempDir::new().unwrap(); let path=jpeg(dir.path(),80,60); let out=dir.path().join("bundle");
    let output=std::process::Command::new(env!("CARGO_BIN_EXE_imgcull"))
        .arg("prepare").arg(&path).arg("--output").arg(&out).args(["--min-pixels","0"]).output().unwrap();
    assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
    let response:serde_json::Value=serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["review_status"],"unreviewed");
    assert!(response.get("views").is_none());
    assert!(output.stdout.len()<2048,"the default response should be a compact index, not full evidence/history");
    let verify=std::process::Command::new(env!("CARGO_BIN_EXE_imgcull")).arg("verify-evidence").arg(&out).output().unwrap();
    assert!(verify.status.success(),"{}",String::from_utf8_lossy(&verify.stderr)); assert!(!path.with_extension("xmp").exists());
}
