//! One resumable review store: immutable job and ordered immutable checkpoints.
//! No image/XMP mutation, model requests, or dependence on conversation history.
use crate::{calibration, discovery, evidence, integrity, policy, storage};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Fingerprint of one physical photo file; every same-stem companion is frozen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    /// Canonical file path.
    pub path: PathBuf,
    /// Streamed SHA-256 of complete bytes.
    pub sha256: String,
}

/// Logical photo identity plus source and existing metadata revisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Photo {
    /// Opaque stable key for compact records.
    pub photo_id: String,
    /// Preferred RAW identity, or JPEG when no RAW exists.
    pub primary: PathBuf,
    /// Shared XMP key from discovery.
    pub sidecar: PathBuf,
    /// None means the sidecar was absent, not empty or unreadable.
    pub xmp_sha256: Option<String>,
    /// All supported same-stem physical files, not only the rendered JPEG.
    pub assets: Vec<Asset>,
}

/// Immutable job definition. Progress exists only in checkpoint records.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    /// Store schema.
    pub schema: u32,
    /// Selection policy version prevents silently changing an existing job.
    pub policy_version: u32,
    /// Exact preference/correction revision, embedded so external edits cannot
    /// silently change a running job. None retains only engine safety policy.
    #[serde(default)]
    pub calibration: Option<calibration::Profile>,
    /// Required decoded native area, frozen independently of individual bundles.
    pub min_pixels: u64,
    /// Canonical JSON checksum with this field cleared.
    pub job_id: String,
    /// Frozen logical photos, with companion assets counted once.
    pub photos: Vec<Photo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    schema: u32,
    revision: u64,
    previous: String,
    checkpoint_id: String,
    photo_id: String,
    evidence: PathBuf,
    observation: policy::Observation,
}

/// A compact progress summary, without image bytes or repeated observations.
#[derive(Debug, Serialize)]
pub struct Status {
    /// Frozen engine judgment policy.
    pub policy_version: u32,
    /// Frozen personal preference revision, if supplied at creation.
    pub profile_id: Option<String>,
    /// Immutable job ID.
    pub job_id: String,
    /// Current compare-and-swap revision for the next checkpoint.
    pub revision: u64,
    /// Logical photos in this job.
    pub total: usize,
    /// Photos with at least one stored draft observation.
    pub recorded: usize,
    /// Photos without any observation.
    pub pending: usize,
    /// This stage cannot authorize metadata application.
    pub can_apply: bool,
}

/// Revalidated proposal report; not a metadata change plan.
#[derive(Debug, Serialize)]
pub struct CheckReport {
    /// Compact durable progress.
    pub status: Status,
    /// Per-photo reasons to hold the latest proposal; empty is still not apply authorization.
    pub blockers: BTreeMap<String, Vec<String>>,
    /// Cross-photo constraints over latest observations.
    pub group_blockers: Vec<String>,
}

/// A bounded work page; prior observation histories stay in the store.
#[derive(Debug, Serialize)]
pub struct Page {
    /// Progress and revision for this page.
    pub status: Status,
    /// Starting offset in stable primary-path order.
    pub offset: usize,
    /// Opaque ID, source path, and whether a draft exists.
    pub photos: Vec<PhotoItem>,
}

/// Minimal scheduling information for one logical photo.
#[derive(Debug, Serialize)]
pub struct PhotoItem {
    /// Use this ID for record-review.
    pub photo_id: String,
    /// Primary file to pass to prepare.
    pub primary: PathBuf,
    /// Recorded does not mean visually validated or approved.
    pub recorded: bool,
}

/// Latest draft for a single frame, without replaying prior revisions.
#[derive(Debug, Serialize)]
pub struct Draft {
    /// Current global revision for the next write.
    pub status: Status,
    /// Requested photo identity.
    pub photo_id: String,
    /// Last checkpoint for this frame, if any.
    pub photo_revision: Option<u64>,
    /// Bundle referenced by the draft; not revalidated by this read.
    pub evidence: Option<PathBuf>,
    /// Latest reviewer declaration; not an approved verdict.
    pub observation: Option<policy::Observation>,
}

struct Loaded {
    job: Job,
    revision: u64,
    tip: String,
    latest: BTreeMap<String, Checkpoint>,
}

fn read_json<T: DeserializeOwned>(path: &Path, limit: u64) -> Result<T> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.is_file() && !meta.file_type().is_symlink() && meta.len() <= limit,
        "invalid/oversized store file"
    );
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn xmp_hash(path: &Path) -> Result<Option<String>> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            ensure!(
                meta.is_file() && !meta.file_type().is_symlink(),
                "invalid XMP sidecar"
            );
            Ok(Some(integrity::file_digest(path)?))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn capture(path: &Path) -> Result<Photo> {
    let assets = discovery::photo_assets(path)?;
    let primary = discovery::deduplicate_images(assets.clone())?.remove(0);
    let sidecar = discovery::photo_key(&primary)?;
    let photo_id = integrity::digest(&serde_json::to_vec(&sidecar)?);
    let xmp_sha256 = xmp_hash(&sidecar)?;
    let assets = assets
        .into_iter()
        .map(|path| {
            let sha256 = integrity::file_digest(&path)?;
            Ok(Asset { path, sha256 })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Photo {
        photo_id,
        primary,
        sidecar,
        xmp_sha256,
        assets,
    })
}

fn job_checksum(job: &Job) -> Result<String> {
    let mut copy = job.clone();
    copy.job_id.clear();
    Ok(integrity::digest(&serde_json::to_vec(&copy)?))
}

fn checkpoint_checksum(checkpoint: &Checkpoint) -> Result<String> {
    let mut copy = checkpoint.clone();
    copy.checkpoint_id.clear();
    Ok(integrity::digest(&serde_json::to_vec(&copy)?))
}

fn store_root(directory: &Path) -> Result<PathBuf> {
    let meta = fs::symlink_metadata(directory)?;
    ensure!(
        meta.is_dir() && !meta.file_type().is_symlink(),
        "invalid review directory"
    );
    let directory = directory.canonicalize()?;
    let meta = fs::symlink_metadata(directory.join("checkpoints"))?;
    ensure!(
        meta.is_dir() && !meta.file_type().is_symlink(),
        "invalid checkpoint directory"
    );
    Ok(directory)
}

fn load(directory: &Path) -> Result<Loaded> {
    let job: Job = read_json(&directory.join("job.json"), 64 * 1024 * 1024)?;
    ensure!(
        job.schema == 2 && job.policy_version == policy::POLICY_VERSION,
        "unsupported review schema/policy; keep old job unchanged and create a new job deliberately"
    );
    ensure!(job.job_id == job_checksum(&job)?, "job manifest changed");
    if let Some(profile) = &job.calibration {
        calibration::verify(profile)?;
    }
    let ids: BTreeSet<_> = job.photos.iter().map(|p| &p.photo_id).collect();
    ensure!(
        !ids.is_empty() && ids.len() == job.photos.len(),
        "invalid job photo identities"
    );
    let mut paths = Vec::new();
    for entry in fs::read_dir(directory.join("checkpoints"))? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().context("invalid checkpoint name")?;
        // Incomplete temp directories are never committed checkpoints. A crash
        // before rename leaves these harmless; do not delete them automatically.
        if name.starts_with(".checkpoint-") && entry.file_type()?.is_dir() {
            continue;
        }
        ensure!(
            name.len() == 20 && name.bytes().all(|b| b.is_ascii_digit()),
            "unknown checkpoint entry"
        );
        ensure!(entry.file_type()?.is_dir(), "invalid checkpoint directory");
        paths.push(entry.path());
    }
    paths.sort();
    let mut revision = 0;
    let mut tip = job.job_id.clone();
    let mut latest = BTreeMap::new();
    for path in paths {
        revision += 1;
        ensure!(
            path.file_name().unwrap() == format!("{revision:020}").as_str(),
            "checkpoint sequence gap"
        );
        let checkpoint: Checkpoint = read_json(&path.join("record.json"), 512 * 1024)?;
        ensure!(
            checkpoint.schema == 1 && checkpoint.revision == revision && checkpoint.previous == tip,
            "checkpoint history mismatch"
        );
        ensure!(
            checkpoint.checkpoint_id == checkpoint_checksum(&checkpoint)?,
            "checkpoint changed"
        );
        ensure!(
            ids.contains(&checkpoint.photo_id),
            "unknown checkpoint photo"
        );
        tip.clone_from(&checkpoint.checkpoint_id);
        latest.insert(checkpoint.photo_id.clone(), checkpoint);
    }
    Ok(Loaded {
        job,
        revision,
        tip,
        latest,
    })
}

impl Loaded {
    fn status(&self) -> Status {
        Status {
            policy_version: self.job.policy_version,
            profile_id: self.job.calibration.as_ref().map(|p| p.profile_id.clone()),
            job_id: self.job.job_id.clone(),
            revision: self.revision,
            total: self.job.photos.len(),
            recorded: self.latest.len(),
            pending: self.job.photos.len() - self.latest.len(),
            can_apply: false,
        }
    }
}

/// Freeze a new job, including every companion and existing XMP revision.
/// Does not decode photos, contact a model, or overwrite an existing output.
pub fn create(paths: &[PathBuf], output: &Path, min_pixels: u64) -> Result<Status> {
    create_with_profile(paths, output, min_pixels, None)
}

/// Create a job with an exact immutable personal preference snapshot.
/// Existing jobs are never reinterpreted under a changed profile/policy.
pub fn create_with_profile(
    paths: &[PathBuf],
    output: &Path,
    min_pixels: u64,
    profile: Option<&Path>,
) -> Result<Status> {
    let calibration = profile.map(calibration::load).transpose()?;
    ensure!(
        min_pixels <= crate::imaging::MAX_PIXELS,
        "pixel floor exceeds renderer limit"
    );
    ensure!(!output.try_exists()?, "review job already exists");
    let inputs = discovery::deduplicate_images(discovery::discover_images_checked(paths)?)?;
    ensure!(!inputs.is_empty(), "no photos discovered");
    let mut photos = BTreeMap::new();
    for input in inputs {
        let photo = capture(&input)?;
        photos.insert(photo.photo_id.clone(), photo);
    }
    let mut photos: Vec<_> = photos.into_values().collect();
    photos.sort_by(|a, b| a.primary.cmp(&b.primary));
    let mut job = Job {
        schema: 2,
        policy_version: policy::POLICY_VERSION,
        calibration,
        min_pixels,
        job_id: String::new(),
        photos,
    };
    job.job_id = job_checksum(&job)?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let staging = tempfile::Builder::new()
        .prefix(".imgcull-job-")
        .tempdir_in(parent)?;
    fs::create_dir(staging.path().join("checkpoints"))?;
    let bytes = serde_json::to_vec_pretty(&job)?;
    ensure!(bytes.len() <= 64 * 1024 * 1024, "job manifest is too large");
    storage::write_new(&staging.path().join("job.json"), &bytes)?;
    ensure!(
        !output.try_exists()?,
        "review output appeared during creation"
    );
    fs::rename(staging.path(), output)?;
    status(output)
}

/// Read durable progress after a restart. This checks history integrity, not
/// current photo bytes; use check() before relying on any stored proposal.
pub fn status(directory: &Path) -> Result<Status> {
    let root = store_root(directory)?;
    let _lock = storage::lock_file(&root.join("review.lock"))?;
    Ok(load(&root)?.status())
}

/// Retrieve scene-relevant teaching context from this job's frozen revision.
pub fn context(directory: &Path, tags: &[String]) -> Result<calibration::ReviewContext> {
    let root = store_root(directory)?;
    let _lock = storage::lock_file(&root.join("review.lock"))?;
    let loaded = load(&root)?;
    calibration::context(
        loaded
            .job
            .calibration
            .as_ref()
            .context("job has no calibration profile")?,
        tags,
    )
}

/// List a bounded page without loading images or repeating checkpoint history.
pub fn page(directory: &Path, offset: usize, limit: usize) -> Result<Page> {
    ensure!((1..=200).contains(&limit), "page limit must be 1..=200");
    let root = store_root(directory)?;
    let _lock = storage::lock_file(&root.join("review.lock"))?;
    let loaded = load(&root)?;
    let photos = loaded
        .job
        .photos
        .iter()
        .skip(offset)
        .take(limit)
        .map(|photo| PhotoItem {
            photo_id: photo.photo_id.clone(),
            primary: photo.primary.clone(),
            recorded: loaded.latest.contains_key(&photo.photo_id),
        })
        .collect();
    Ok(Page {
        status: loaded.status(),
        offset,
        photos,
    })
}

/// Recover one latest draft after an agent/model restart. Use check() separately
/// to validate current sources and coverage before relying on its proposals.
pub fn show(directory: &Path, photo_id: &str) -> Result<Draft> {
    let root = store_root(directory)?;
    let _lock = storage::lock_file(&root.join("review.lock"))?;
    let loaded = load(&root)?;
    ensure!(
        loaded.job.photos.iter().any(|p| p.photo_id == photo_id),
        "unknown photo identity"
    );
    let record = loaded.latest.get(photo_id);
    Ok(Draft {
        status: loaded.status(),
        photo_id: photo_id.into(),
        photo_revision: record.map(|r| r.revision),
        evidence: record.map(|r| r.evidence.clone()),
        observation: record.map(|r| r.observation.clone()),
    })
}

fn validate_record(
    photo: &Photo,
    record: &Checkpoint,
    min_pixels: u64,
) -> Result<policy::Validation> {
    ensure!(
        capture(&photo.primary)? == *photo,
        "photo companions or XMP changed since job creation"
    );
    let evidence = evidence::verify(&record.evidence)?;
    ensure!(
        u64::from(evidence.render.width) * u64::from(evidence.render.height) >= min_pixels,
        "evidence does not meet the job's native pixel floor"
    );
    ensure!(
        evidence.render.identity == photo.sidecar,
        "evidence belongs to another photo"
    );
    ensure!(
        photo
            .assets
            .iter()
            .any(|a| a.path == evidence.render.source && a.sha256 == evidence.render.source_sha256),
        "evidence source not in frozen asset set"
    );
    policy::validate(&record.observation, &evidence)
}

/// Append one draft observation using an expected global revision. A stale
/// worker, changed companion/XMP, invalid receipt or interrupted publication
/// cannot overwrite a completed checkpoint. Held judgments can still be saved.
pub fn record(
    directory: &Path,
    expected_revision: u64,
    photo_id: &str,
    evidence_directory: &Path,
    observation: policy::Observation,
) -> Result<Status> {
    let root = store_root(directory)?;
    let _lock = storage::lock_file(&root.join("review.lock"))?;
    let loaded = load(&root)?;
    ensure!(
        loaded.revision == expected_revision,
        "stale review revision; reload progress"
    );
    let photo = loaded
        .job
        .photos
        .iter()
        .find(|p| p.photo_id == photo_id)
        .context("unknown photo identity")?;
    let mut checkpoint = Checkpoint {
        schema: 1,
        revision: expected_revision
            .checked_add(1)
            .context("revision overflow")?,
        previous: loaded.tip,
        checkpoint_id: String::new(),
        photo_id: photo_id.into(),
        evidence: evidence_directory.canonicalize()?,
        observation,
    };
    validate_record(photo, &checkpoint, loaded.job.min_pixels)?;
    checkpoint.checkpoint_id = checkpoint_checksum(&checkpoint)?;
    let parent = root.join("checkpoints");
    let staging = tempfile::Builder::new()
        .prefix(".checkpoint-")
        .tempdir_in(&parent)?;
    storage::write_new(
        &staging.path().join("record.json"),
        &serde_json::to_vec_pretty(&checkpoint)?,
    )?;
    let target = parent.join(format!("{:020}", checkpoint.revision));
    ensure!(!target.try_exists()?, "checkpoint already committed");
    fs::rename(staging.path(), target)?;
    Ok(load(&root)?.status())
}

/// Revalidate latest observations, frozen sources/XMP and cross-frame claims.
/// Missing observations and blocked proposals never become completed approvals.
pub fn check(directory: &Path) -> Result<CheckReport> {
    let root = store_root(directory)?;
    let _lock = storage::lock_file(&root.join("review.lock"))?;
    let loaded = load(&root)?;
    let mut blockers = BTreeMap::new();
    for photo in &loaded.job.photos {
        let reasons = match loaded.latest.get(&photo.photo_id) {
            Some(record) => validate_record(photo, record, loaded.job.min_pixels)?.blockers,
            None => vec!["observation_missing".into()],
        };
        if !reasons.is_empty() {
            blockers.insert(photo.photo_id.clone(), reasons);
        }
    }
    let members: Vec<_> = loaded
        .latest
        .iter()
        .map(|(id, r)| (id.as_str(), &r.observation))
        .collect();
    Ok(CheckReport {
        status: loaded.status(),
        blockers,
        group_blockers: policy::group_blockers(&members)?,
    })
}

fn html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn hold_text(code: &str) -> &str {
    match code {
        "lightroom_current_appearance_unavailable" => "尚未接通 Lightroom 当前裁剪、调色的审阅证据",
        "native_inspection_unverified" => "尚未完成可验证的原尺寸主体细节检查",
        "edges_not_reviewed" => "尚未检查画面边缘和背景干扰",
        "composition_not_reviewed" => "尚未查看完整构图",
        "protected_or_unknown_sequence" => "合成素材或尚未确认用途的序列，暂不标记",
        "uncertain" => "判断仍不确定，暂不标记",
        "unique_record_value" => "存在独立记录价值，不宜建议删除",
        "distinct_or_unresolved_content" => "存在值得保留的内容差异，或尚未比较清楚",
        "alternative_quality_unresolved_or_worse" => "替代照片更差，或尚未比较清楚",
        "redundancy_requires_reviewed_burst" => "尚未确认属于可比较的同组照片",
        "burst_requires_retained_representative" => "同组尚未确定保留的代表照片",
        "redundancy_requires_equivalent_content_and_sound_retained_keeper" => {
            "冗余片尚缺少内容等价、明确保留且没有严重技术问题的替代照片"
        }
        "replacement_must_be_better_retained_comparable_frame" => {
            "替代照片必须属于同组、表现更好且已明确保留"
        }
        "green_requires_concrete_edit" => "绿色建议需要说明具体可改善的地方",
        "red_score_too_high_or_missing" => "质量废片理由不足；内容冗余应单独说明替代关系",
        "score_five_requires_better_retained_alternative" => {
            "此分数下建议删除仍需明确更好的保留替代"
        }
        _ => code,
    }
}

/// Write a bounded self-contained comparison page, never a completed selection
/// certificate. Captions/notes are escaped; no script or external resources.
/// Reuses the review store, evidence verification and the single image decoder.
pub fn write_report(directory: &Path, output: &Path, offset: usize, limit: usize) -> Result<()> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    ensure!((1..=200).contains(&limit), "report limit must be 1..=200");
    let root = store_root(directory)?;
    let _lock = storage::lock_file(&root.join("review.lock"))?;
    let loaded = load(&root)?;
    let members: Vec<_> = loaded
        .latest
        .iter()
        .map(|(id, r)| (id.as_str(), &r.observation))
        .collect();
    let group_holds = policy::group_blockers(&members)?;
    let mut page = format!(
        "<!doctype html><html lang=\"zh-CN\"><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src data:; style-src 'unsafe-inline'\"><title>选片复核草稿</title><style>body{{font:16px sans-serif;margin:24px}}main{{display:flex;flex-wrap:wrap;gap:16px}}article{{width:440px;border:1px solid #aaa;padding:12px}}img{{max-width:100%}}pre{{white-space:pre-wrap;overflow-wrap:anywhere}}.hold{{color:#954200}}</style><h1>选片复核草稿</h1><p>未写回任何标记。预览只供构图对比，不证明原尺寸清晰度。当前页从 {offset} 开始，最多 {limit} 张；页外照片和缺失观察未在本页全面复核。</p><details><summary>版本与追溯信息</summary><p>策略 {}；偏好版本 {}</p></details><pre class=\"hold\">组内待解决：{}</pre><main>",
        loaded.job.policy_version,
        html(
            loaded
                .job
                .calibration
                .as_ref()
                .map_or("未绑定", |p| p.profile_id.as_str())
        ),
        html(
            &group_holds
                .iter()
                .map(|s| hold_text(s))
                .collect::<Vec<_>>()
                .join("\n")
        )
    );
    for photo in loaded.job.photos.iter().skip(offset).take(limit) {
        page.push_str(&format!(
            "<article><h2>{}</h2><details><summary>照片追溯编号</summary><p>{}</p></details>",
            html(
                &photo
                    .primary
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            ),
            html(&photo.photo_id)
        ));
        if let Some(record) = loaded.latest.get(&photo.photo_id) {
            let validation = validate_record(photo, record, loaded.job.min_pixels)?;
            let bundle = evidence::verify(&record.evidence)?;
            let view = bundle
                .views
                .iter()
                .find(|v| v.id == "overview")
                .context("overview missing")?;
            let image = crate::imaging::NativePhoto::load(&record.evidence.join(&view.file), 0)?;
            ensure!(
                image.info.source_sha256 == view.sha256,
                "report overview changed"
            );
            let bytes = crate::imaging::encode(&image.overview(512)?, image::ImageFormat::Jpeg)?;
            let proposed = calibration::Expected::from(&record.observation.proposal);
            let action = match proposed.action.as_str() {
                "retain" => "保留",
                "red" => "红色：质量问题",
                "redundant" => "红色：内容冗余",
                _ => "暂不标记",
            };
            let stars = proposed
                .stars
                .map_or_else(|| "无星".into(), |n| format!("{n} 星"));
            let target = proposed
                .keeper
                .as_deref()
                .and_then(|id| loaded.job.photos.iter().find(|p| p.photo_id == id));
            let keeper = target
                .map(|p| {
                    p.primary
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                })
                .or(proposed.keeper.clone())
                .unwrap_or_default();
            page.push_str(&format!("<img alt=\"构图预览\" src=\"data:image/jpeg;base64,{}\"><p>组：{}</p><p>草稿：{}；{}；后期建议 {}</p><p>保留替代：{}</p><pre>{}</pre><pre>后期建议：{}</pre><pre class=\"hold\">待解决：{}</pre>", STANDARD.encode(bytes), html(&record.observation.group), action, stars, if proposed.green { "有" } else { "无" }, html(&keeper), html(&record.observation.reason), html(&record.observation.edits.join("\n")), html(&validation.blockers.iter().map(|s| hold_text(s)).collect::<Vec<_>>().join("\n"))));
            if let Some(comparison) = calibration::comparison(&record.observation.proposal) {
                page.push_str(&format!(
                    "<pre>替代比较：{}</pre>",
                    html(&comparison.reason)
                ));
            }
        } else {
            page.push_str("<p class=\"hold\">尚未审阅</p>");
        }
        page.push_str("</article>");
    }
    page.push_str("</main></html>");
    storage::write_new(output, page.as_bytes())
}
