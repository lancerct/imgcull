//! Integration tests for the processing pipeline.
//!
//! These tests use an Ollama provider config pointing at an unreachable
//! address: constructing `LlmClients` for Ollama needs no API key and makes
//! no network calls, and the scenarios below never reach an LLM stage.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use imgcull::config::{Config, DefaultSettings, Prompts, ProviderConfig};
use imgcull::llm::LlmClients;
use imgcull::pipeline::{PipelineOptions, run_pipeline};
use imgcull::scoring::ScoringResult;
use imgcull::xmp::XmpSidecar;
use tempfile::TempDir;

/// Config whose description and scoring providers are both Ollama at an
/// unreachable local address — client construction succeeds offline.
fn offline_config() -> Config {
    let mut providers = HashMap::new();
    providers.insert(
        "ollama".to_string(),
        ProviderConfig {
            model: "test-model".to_string(),
            api_key_env: None,
            base_url: Some("http://127.0.0.1:9".to_string()),
        },
    );
    Config {
        default_settings: DefaultSettings {
            description_provider: "ollama".to_string(),
            scoring_provider: "ollama".to_string(),
            ..Default::default()
        },
        providers,
    }
}


#[derive(Clone)]
struct TestProvider { valid: bool, calls: Arc<std::sync::atomic::AtomicUsize> }
#[async_trait::async_trait]
impl imgcull::llm::DescriptionProvider for TestProvider {
    async fn describe(&self, _: &str, _: &str) -> anyhow::Result<String> { Ok("generated caption".into()) }
}
#[async_trait::async_trait]
impl imgcull::llm::ScoringProvider for TestProvider {
    async fn score(&self, _: &str, _: &str) -> anyhow::Result<ScoringResult> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if !self.valid { return Ok(ScoringResult::default()); }
        Ok(ScoringResult { sharpness:Some(0.5), exposure:Some(0.5), composition:Some(0.5),
            subject_clarity:Some(0.5), aesthetics:Some(0.5), keywords:Some(vec!["new tag".into()]), ..Default::default() })
    }
}
fn fake_clients(valid: bool) -> (Arc<LlmClients>, Arc<std::sync::atomic::AtomicUsize>) {
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let provider = TestProvider { valid, calls: calls.clone() };
    (Arc::new(LlmClients::from_providers(provider.clone(), provider)), calls)
}
#[tokio::test]
async fn forced_rescore_of_raw_jpeg_pair_preserves_human_metadata_and_runs_once() {
    let tmp=TempDir::new().unwrap(); let jpg=tmp.path().join("pair.JPG"); let raw=tmp.path().join("pair.NEF");
    image::RgbImage::new(32,32).save(&jpg).unwrap(); std::fs::write(&raw,b"not decoded because JPEG is present").unwrap();
    let mut original=XmpSidecar::new(); original.set_description("人工标题 & caption"); original.set_rating(5);
    original.set_label("Green"); original.set_keywords(&["人工关键词".into()]); original.write(&raw.with_extension("xmp")).unwrap();
    let config=offline_config(); let prompts=Prompts::default(); let (clients,calls)=fake_clients(true);
    let options=PipelineOptions { force:true, ..default_options() };
    run_pipeline(vec![jpg,raw.clone(),raw.clone()],&config,&prompts,clients,options).await.unwrap();
    assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed),1);
    let saved=XmpSidecar::read(&raw.with_extension("xmp")).unwrap();
    assert_eq!(saved.description(),Some("人工标题 & caption")); assert_eq!(saved.rating(),Some(5));
    assert_eq!(saved.label(),Some("Green")); assert_eq!(saved.keywords(), &["人工关键词"]); assert!(saved.has_scores());
}
#[tokio::test(start_paused = true)]
async fn invalid_model_result_fails_without_partial_metadata_write() {
    let tmp=TempDir::new().unwrap(); let jpg=tmp.path().join("photo.jpg"); image::RgbImage::new(32,32).save(&jpg).unwrap();
    let config=offline_config(); let prompts=Prompts::default(); let (clients,_)=fake_clients(false);
    assert!(run_pipeline(vec![jpg.clone()],&config,&prompts,clients,default_options()).await.is_err());
    assert!(!jpg.with_extension("xmp").exists(), "a generated description must not be saved after scoring fails");
}
#[tokio::test]
async fn malformed_sidecar_fails_and_remains_byte_exact() {
    let tmp=TempDir::new().unwrap(); let jpg=tmp.path().join("photo.jpg"); std::fs::write(&jpg,b"not read").unwrap();
    let path=jpg.with_extension("xmp"); std::fs::write(&path,b"<broken").unwrap();
    let config=offline_config(); let prompts=Prompts::default(); let (clients,calls)=fake_clients(true);
    assert!(run_pipeline(vec![jpg],&config,&prompts,clients,default_options()).await.is_err());
    assert_eq!(std::fs::read(path).unwrap(),b"<broken"); assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed),0);
}
#[tokio::test]
async fn zero_concurrency_fails_before_any_work() {
    let mut config=offline_config(); config.default_settings.concurrency=0;
    let prompts=Prompts::default(); let (clients,_)=fake_clients(true);
    assert!(run_pipeline(vec![],&config,&prompts,clients,default_options()).await.is_err());
}

fn default_options() -> PipelineOptions {
    PipelineOptions {
        no_description: false,
        no_rating: false,
        backup: false,
        force: false,
        describe_only: false,
    }
}

/// Write a sidecar that already has a description and scores (but no
/// original filename) next to `image_path`.
fn write_complete_sidecar(image_path: &Path) {
    let scores = ScoringResult {
        sharpness: Some(0.9),
        exposure: Some(0.8),
        composition: Some(0.85),
        subject_clarity: Some(0.85),
        aesthetics: Some(0.85),
        ..Default::default()
    };
    let mut sidecar = XmpSidecar::new();
    sidecar.set_description("already described");
    sidecar.set_scores(&scores, 0.85, "test/model");
    sidecar
        .write(&image_path.with_extension("xmp"))
        .expect("should write sidecar");
}

/// When the sidecar already carries a description and scores, no LLM stage
/// runs and the pipeline must skip preprocessing entirely: an unreadable
/// image file is not an error, and the run still completes far enough to
/// stamp the original filename into the sidecar.
#[tokio::test]
async fn pipeline_skips_preprocess_when_sidecar_is_complete() {
    let tmp = TempDir::new().unwrap();
    let image = tmp.path().join("photo.jpg");
    // Garbage bytes: preprocessing this file would fail.
    std::fs::write(&image, b"not a real jpeg").unwrap();
    write_complete_sidecar(&image);

    let config = offline_config();
    let prompts = Prompts::default();
    let clients = Arc::new(LlmClients::new(&config, &prompts).expect("offline client build"));

    run_pipeline(
        vec![image.clone()],
        &config,
        &prompts,
        clients,
        default_options(),
    )
    .await
    .expect("pipeline should not fail");

    let sidecar = XmpSidecar::read(&image.with_extension("xmp")).expect("should re-read");
    assert_eq!(sidecar.description(), Some("already described"));
    assert!(sidecar.has_scores());
    // Only reachable if the unreadable image was never preprocessed.
    assert_eq!(sidecar.original_filename(), Some("photo.jpg"));
}

/// With --force the LLM stages are needed again, so preprocessing runs and
/// an unreadable image is skipped before the sidecar is ever rewritten.
#[tokio::test]
async fn pipeline_force_still_fails_unreadable_images_early() {
    let tmp = TempDir::new().unwrap();
    let image = tmp.path().join("photo.jpg");
    std::fs::write(&image, b"not a real jpeg").unwrap();
    write_complete_sidecar(&image);
    let sidecar_path = image.with_extension("xmp");
    let before = std::fs::read_to_string(&sidecar_path).unwrap();

    let config = offline_config();
    let prompts = Prompts::default();
    let clients = Arc::new(LlmClients::new(&config, &prompts).expect("offline client build"));

    let options = PipelineOptions {
        force: true,
        ..default_options()
    };
    run_pipeline(vec![image.clone()], &config, &prompts, clients, options)
        .await
        .expect_err("unreadable photo must fail the run");

    let after = std::fs::read_to_string(&sidecar_path).unwrap();
    assert_eq!(
        before, after,
        "unreadable image must leave sidecar untouched"
    );
}
