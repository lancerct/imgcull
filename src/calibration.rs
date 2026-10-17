//! Immutable personal preferences and correction cases, not model training or
//! a second review/progress store. Evaluation measures supplied decisions only.
use crate::{
    integrity,
    policy::{Comparison, Proposal},
    storage,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

/// Hard bound for local preference/control JSON; never contains image payloads.
pub const MAX_BYTES: u64 = 2 * 1024 * 1024;
/// Keep retrieved teaching context bounded independently of the whole corpus.
pub const MAX_CONTEXT_BYTES: usize = 32 * 1024;

/// Separate teaching examples from regression cases withheld from context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Split {
    /// Examples available to a reviewer.
    Teaching,
    /// Cases used by evaluate, excluded from context.
    Validation,
}

/// Why a user corrected a judgment; informs how to improve the review.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    /// A visible object, defect or detail was missed or invented.
    Perception,
    /// Comparability, redundancy or sequence purpose was misunderstood.
    Grouping,
    /// Artistic/record-value expectations differed.
    Preference,
    /// The explanation did not support the proposed marking.
    Explanation,
}

/// Outcome projection shared by corrections and predictions; no parallel
/// star/red policy implementation. Detailed comparisons remain in Proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expected {
    /// Projection of the engine's typed proposal.
    pub action: String,
    /// Preserved representative for redundant/red-with-replacement proposals.
    pub keeper: Option<String>,
    /// Exact expected stars; None means unstarred.
    pub stars: Option<u8>,
    /// Concrete post-processing recommendation requested.
    pub green: bool,
}

impl From<&Proposal> for Expected {
    fn from(p: &Proposal) -> Self {
        let (action, keeper, stars, green) = match p {
            Proposal::Leave => ("leave", None, None, false),
            Proposal::Retain { stars, green } => ("retain", None, *stars, *green),
            Proposal::Red { replacement } => ("red", replacement.clone(), None, false),
            Proposal::Redundant { comparison } => {
                ("redundant", Some(comparison.keeper.clone()), None, false)
            }
        };
        Self {
            action: action.into(),
            keeper,
            stars,
            green,
        }
    }
}

/// One frame within a correction case; identifiers are local to that case.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    /// Opaque case-local name, never a filesystem path.
    pub id: String,
    /// Concrete observations/assumptions, explicitly separate from the label.
    pub facts: Vec<String>,
    /// Old decision when recording an actual correction.
    pub previous: Option<Expected>,
    /// User-approved outcome under these facts.
    pub expected: Expected,
}

/// A correction or counterexample. Tags support overlapping subjects/purposes
/// (e.g. bird + flight + low_light), without one fixed genre taxonomy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    /// Stable ID across profile revisions, allowing case-level comparison.
    pub id: String,
    /// Optional local immutable evidence IDs for actual image-based corrections.
    /// Hash references only; no photo bytes or private source paths in profiles.
    #[serde(default)]
    pub evidence_ids: Vec<String>,
    /// Teaching or withheld validation.
    pub split: Split,
    /// Scene/purpose tags; not inferred proof of sequence membership.
    pub tags: Vec<String>,
    /// Actual user correction, preference example or synthetic counterexample.
    pub origin: String,
    /// Root cause under discussion.
    pub failure: FailureKind,
    /// What changed and the boundary of this lesson.
    pub lesson: String,
    /// All frames, including the keeper and meaningful alternatives.
    pub frames: Vec<Frame>,
    /// Other equally acceptable complete group outcomes. Whole-group variants
    /// avoid imposing an arbitrary single keeper when several choices are valid.
    #[serde(default)]
    pub alternatives: Vec<BTreeMap<String, Expected>>,
}

/// A small retrievable preference, rather than a growing conversation replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// Stable rule ID.
    pub id: String,
    /// Empty applies globally; otherwise retrieved by matching scene tags.
    pub tags: Vec<String>,
    /// Preference cannot override evidence, sequence protection or data safety.
    pub text: String,
}

/// Publish a new file for every revision. No mutable 'latest' pointer and no
/// automatic activation/overwriting of the profile of an existing review job.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Format version.
    pub schema: u32,
    /// Canonical SHA-256 with this field cleared; empty in an input draft.
    pub profile_id: String,
    /// Previous profile ID, if this deliberately revises a prior preference set.
    pub parent_id: Option<String>,
    /// Plain revision explanation; no credentials or private image payloads.
    pub change: String,
    /// Rules with explicit applicability.
    pub rules: Vec<Rule>,
    /// Complete correction and counterexample corpus for this revision.
    pub cases: Vec<Case>,
}

fn text_ok(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 4096
}
fn id_ok(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn hash_ok(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
fn tags_ok(tags: &[String]) -> bool {
    tags.len() <= 16
        && tags.iter().all(|s| id_ok(s))
        && tags.iter().collect::<BTreeSet<_>>().len() == tags.len()
}
fn checksum(profile: &Profile) -> Result<String> {
    let mut copy = profile.clone();
    copy.profile_id.clear();
    Ok(integrity::digest(&serde_json::to_vec(&copy)?))
}

/// Read bounded regular-file control data. Unknown fields are rejected by the
/// individual schemas, rather than silently accepting misspelled corrections.
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    storage::read_json(path, MAX_BYTES)
}

fn validate_expected(e: &Expected, ids: &BTreeSet<&str>) -> Result<()> {
    ensure!(
        matches!(e.action.as_str(), "leave" | "retain" | "red" | "redundant"),
        "invalid expected action"
    );
    ensure!(
        e.stars.is_none_or(|n| (3..=5).contains(&n)),
        "invalid expected stars"
    );
    ensure!(
        e.action == "retain" || (e.stars.is_none() && !e.green),
        "only retain can have stars/green"
    );
    ensure!(
        e.keeper.is_none() || matches!(e.action.as_str(), "red" | "redundant"),
        "unexpected keeper"
    );
    ensure!(
        e.action != "redundant" || e.keeper.is_some(),
        "redundancy needs keeper"
    );
    ensure!(
        e.keeper.as_deref().is_none_or(|id| ids.contains(id)),
        "unknown case keeper"
    );
    Ok(())
}

fn validate_outcome(outcome: &BTreeMap<String, Expected>, ids: &BTreeSet<&str>) -> Result<()> {
    ensure!(
        outcome.len() == ids.len() && outcome.keys().all(|id| ids.contains(id.as_str())),
        "alternative must cover the complete group"
    );
    for (id, expected) in outcome {
        validate_expected(expected, ids)?;
        if let Some(keeper) = &expected.keeper {
            ensure!(
                keeper != id && outcome.get(keeper).is_some_and(|e| e.action == "retain"),
                "case keeper must be directly retained, not a cycle"
            );
        }
    }
    Ok(())
}

fn validate(profile: &Profile) -> Result<()> {
    ensure!(
        profile.schema == 1 && text_ok(&profile.change),
        "invalid calibration schema/change"
    );
    ensure!(
        profile.parent_id.as_deref().is_none_or(hash_ok),
        "invalid parent profile ID"
    );
    ensure!(
        profile.rules.len() <= 64 && (1..=128).contains(&profile.cases.len()),
        "calibration size limit"
    );
    let mut rule_ids = BTreeSet::new();
    for rule in &profile.rules {
        ensure!(
            id_ok(&rule.id)
                && rule_ids.insert(&rule.id)
                && tags_ok(&rule.tags)
                && text_ok(&rule.text),
            "invalid/duplicate rule"
        );
    }
    let mut case_ids = BTreeSet::new();
    for case in &profile.cases {
        ensure!(
            id_ok(&case.id)
                && case_ids.insert(&case.id)
                && tags_ok(&case.tags)
                && !case.tags.is_empty(),
            "invalid/duplicate case"
        );
        ensure!(
            case.evidence_ids.len() <= 32
                && case.evidence_ids.iter().all(|id| hash_ok(id))
                && case.evidence_ids.iter().collect::<BTreeSet<_>>().len()
                    == case.evidence_ids.len(),
            "invalid case evidence references"
        );
        ensure!(
            matches!(
                case.origin.as_str(),
                "user_correction" | "user_preference" | "synthetic_counterexample"
            ) && text_ok(&case.lesson),
            "invalid case origin/lesson"
        );
        ensure!((1..=16).contains(&case.frames.len()), "case frame limit");
        let ids: BTreeSet<_> = case.frames.iter().map(|f| f.id.as_str()).collect();
        ensure!(
            ids.len() == case.frames.len() && ids.iter().all(|id| id_ok(id)),
            "invalid/duplicate frame"
        );
        for frame in &case.frames {
            ensure!(
                (1..=16).contains(&frame.facts.len()) && frame.facts.iter().all(|s| text_ok(s)),
                "invalid case facts"
            );
            validate_expected(&frame.expected, &ids)?;
            if let Some(previous) = &frame.previous {
                validate_expected(previous, &ids)?;
            }
        }
        let canonical = case
            .frames
            .iter()
            .map(|f| (f.id.clone(), f.expected.clone()))
            .collect();
        validate_outcome(&canonical, &ids)?;
        ensure!(
            case.alternatives.len() <= 4,
            "too many acceptable group alternatives"
        );
        for alternative in &case.alternatives {
            validate_outcome(alternative, &ids)?;
        }
    }
    Ok(())
}

/// Validate and publish a new immutable profile; never replaces an old version.
pub fn publish(input: &Path, output: &Path) -> Result<String> {
    let mut profile: Profile = read_json(input)?;
    ensure!(
        profile.profile_id.is_empty(),
        "publish expects a draft with empty profile_id"
    );
    validate(&profile)?;
    profile.profile_id = checksum(&profile)?;
    let bytes = serde_json::to_vec_pretty(&profile)?;
    ensure!(
        bytes.len() as u64 <= MAX_BYTES,
        "published profile exceeds size limit"
    );
    storage::write_new(output, &bytes)?;
    Ok(profile.profile_id)
}

/// Load an exact published revision, rejecting accidental mutation.
pub fn load(path: &Path) -> Result<Profile> {
    let profile: Profile = read_json(path)?;
    verify(&profile)?;
    Ok(profile)
}

/// Check an embedded immutable profile as well as a standalone one.
pub fn verify(profile: &Profile) -> Result<()> {
    validate(profile)?;
    ensure!(
        hash_ok(&profile.profile_id) && profile.profile_id == checksum(profile)?,
        "profile changed or is unpublished"
    );
    Ok(())
}

/// Compact reviewer context. Validation answers are deliberately excluded.
#[derive(Debug, Serialize)]
pub struct ReviewContext {
    /// Exact profile used, to attach to a review job/evaluation.
    pub profile_id: String,
    /// Matching preferences, including global rules.
    pub rules: Vec<Rule>,
    /// Matching teaching examples only.
    pub examples: Vec<Case>,
    /// Context never constitutes image inspection or metadata authorization.
    pub can_apply: bool,
}

/// Retrieve a bounded context for supplied scene/purpose tags. These tags are
/// reviewer input, not an automatic scene classifier or a safety-policy bypass.
pub fn context(profile: &Profile, tags: &[String]) -> Result<ReviewContext> {
    verify(profile)?;
    ensure!(
        tags_ok(tags) && !tags.is_empty(),
        "provide 1..=16 unique scene tags"
    );
    let matches = |wanted: &[String]| wanted.is_empty() || wanted.iter().any(|t| tags.contains(t));
    let rules: Vec<_> = profile
        .rules
        .iter()
        .filter(|r| matches(&r.tags))
        .cloned()
        .collect();
    let examples: Vec<_> = profile
        .cases
        .iter()
        .filter(|c| c.split == Split::Teaching && matches(&c.tags))
        .cloned()
        .collect();
    ensure!(
        rules.len() <= 16 && examples.len() <= 8,
        "context too broad; narrow tags or curate profile"
    );
    let context = ReviewContext {
        profile_id: profile.profile_id.clone(),
        rules,
        examples,
        can_apply: false,
    };
    ensure!(
        serde_json::to_vec(&context)?.len() <= MAX_CONTEXT_BYTES,
        "context exceeds byte budget; curate shorter rules/cases"
    );
    Ok(context)
}

/// Results from one declared model/prompt run, using the existing Proposal type.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Predictions {
    /// Must match the exact profile evaluated.
    pub profile_id: String,
    /// Model and prompt revision, not an API credential.
    pub reviewer: String,
    /// Exact case/frame coverage for the chosen split.
    pub cases: BTreeMap<String, BTreeMap<String, Proposal>>,
}

/// Per-frame discrepancy. A total score alone must not hide false red advice.
#[derive(Debug, Serialize)]
pub struct Mismatch {
    /// Case identifier.
    pub case_id: String,
    /// Case-local frame.
    pub frame_id: String,
    /// Expected user-calibrated marking.
    pub expected: Expected,
    /// Marking projected from the submitted proposal.
    pub actual: Expected,
    /// False means the comparison claimed redundancy without supporting facts.
    pub comparison_supported: bool,
}

/// Reproducible decision comparison, never evidence of vision accuracy by itself.
#[derive(Debug, Serialize)]
pub struct Evaluation {
    /// Evaluated immutable preference set.
    pub profile_id: String,
    /// Declared model/prompt run.
    pub reviewer: String,
    /// Teaching self-check and withheld validation are reported separately.
    pub split: Split,
    /// Total evaluated frames; omissions are rejected rather than ignored.
    pub total: usize,
    /// All exact decision mismatches, including wrong keeper/stars/green.
    pub mismatches: Vec<Mismatch>,
    /// Red/redundant advice where leave/retain was expected.
    pub false_red: usize,
    /// Failed to mark an expected redundant frame red/redundant.
    pub missed_redundancy: usize,
    /// Exact matches only; no arbitrary tolerance or hidden weighting.
    pub passed: bool,
    /// Evaluator cannot authorize metadata application or certify image viewing.
    pub can_apply: bool,
}

fn is_red(e: &Expected) -> bool {
    matches!(e.action.as_str(), "red" | "redundant")
}

/// Evaluate a complete split. The predictions must be generated independently;
/// this function does not call/train a model or validate visual evidence.
pub fn evaluate(profile: &Profile, predictions: &Predictions, split: Split) -> Result<Evaluation> {
    verify(profile)?;
    ensure!(
        predictions.profile_id == profile.profile_id && text_ok(&predictions.reviewer),
        "prediction profile/reviewer mismatch"
    );
    let cases: Vec<_> = profile.cases.iter().filter(|c| c.split == split).collect();
    ensure!(
        !cases.is_empty() && predictions.cases.len() == cases.len(),
        "evaluation needs complete nonempty split"
    );
    let mut result = Evaluation {
        profile_id: profile.profile_id.clone(),
        reviewer: predictions.reviewer.clone(),
        split,
        total: 0,
        mismatches: vec![],
        false_red: 0,
        missed_redundancy: 0,
        passed: true,
        can_apply: false,
    };
    for case in cases {
        let supplied = predictions
            .cases
            .get(&case.id)
            .context("missing evaluation case")?;
        ensure!(
            supplied.len() == case.frames.len(),
            "incomplete/extra evaluation frames"
        );
        ensure!(
            case.frames.iter().all(|f| supplied.contains_key(&f.id)),
            "missing evaluation frame"
        );
        let canonical: BTreeMap<_, _> = case
            .frames
            .iter()
            .map(|f| (f.id.clone(), f.expected.clone()))
            .collect();
        // Choose a COMPLETE acceptable variant, never independent per-frame
        // allowances that could accidentally approve all-red or all-retained.
        let expected_group = std::iter::once(&canonical)
            .chain(case.alternatives.iter())
            .min_by_key(|variant| {
                variant
                    .iter()
                    .filter(|(id, expected)| Expected::from(&supplied[*id]) != **expected)
                    .count()
            })
            .context("missing expected group")?;
        for frame in &case.frames {
            let proposal = supplied
                .get(&frame.id)
                .context("missing evaluation frame")?;
            let actual = Expected::from(proposal);
            let expected = &expected_group[&frame.id];
            let comparison_supported = comparison(proposal).is_none_or(|c| {
                c.supports_redundancy()
                    && text_ok(&c.reason)
                    && c.keeper != frame.id
                    && supplied
                        .get(&c.keeper)
                        .is_some_and(|p| matches!(p, Proposal::Retain { .. }))
            });
            result.total += 1;
            result.false_red += usize::from(is_red(&actual) && !is_red(expected));
            result.missed_redundancy +=
                usize::from(expected.action == "redundant" && !is_red(&actual));
            if actual != *expected || !comparison_supported {
                result.mismatches.push(Mismatch {
                    case_id: case.id.clone(),
                    frame_id: frame.id.clone(),
                    expected: expected.clone(),
                    actual,
                    comparison_supported,
                });
            }
        }
    }
    result.passed = result.mismatches.is_empty();
    Ok(result)
}

/// Shared extraction for reports, without treating a comparison as a judgment.
pub fn comparison(proposal: &Proposal) -> Option<&Comparison> {
    match proposal {
        Proposal::Redundant { comparison } => Some(comparison),
        _ => None,
    }
}

/// Input-only frame for independent evaluation; no old/corrected marking.
#[derive(Debug, Serialize)]
pub struct ChallengeFrame {
    /// Case-local frame ID for the prediction.
    pub id: String,
    /// Facts supplied to the reviewer, not an expected decision.
    pub facts: Vec<String>,
}

/// Input-only case; correction lessons and root-cause hints are excluded.
#[derive(Debug, Serialize)]
pub struct ChallengeCase {
    /// Stable case ID.
    pub id: String,
    /// Scene and purpose context.
    pub tags: Vec<String>,
    /// Local material references; not proof of correct image inspection.
    pub evidence_ids: Vec<String>,
    /// Frames without expected or previous labels.
    pub frames: Vec<ChallengeFrame>,
}

/// A bounded page of validation/teaching questions, without answer fields.
#[derive(Debug, Serialize)]
pub struct Challenge {
    /// Exact immutable profile evaluated.
    pub profile_id: String,
    /// Chosen split; evaluation requires predictions for the complete split.
    pub split: Split,
    /// Total cases across all pages of this split.
    pub total: usize,
    /// Starting case offset.
    pub offset: usize,
    /// Input-only material for this page.
    pub cases: Vec<ChallengeCase>,
    /// Questions do not authorize markings.
    pub can_apply: bool,
}

/// Export questions separately from teaching answers. This limits accidental
/// label leakage, not a malicious reviewer's access to the full profile.
pub fn challenge(
    profile: &Profile,
    split: Split,
    offset: usize,
    limit: usize,
) -> Result<Challenge> {
    verify(profile)?;
    ensure!(
        (1..=16).contains(&limit),
        "challenge page limit must be 1..=16"
    );
    let all: Vec<_> = profile.cases.iter().filter(|c| c.split == split).collect();
    ensure!(
        !all.is_empty() && offset < all.len(),
        "empty/out-of-range challenge page"
    );
    let cases = all
        .iter()
        .skip(offset)
        .take(limit)
        .map(|c| ChallengeCase {
            id: c.id.clone(),
            tags: c.tags.clone(),
            evidence_ids: c.evidence_ids.clone(),
            frames: c
                .frames
                .iter()
                .map(|f| ChallengeFrame {
                    id: f.id.clone(),
                    facts: f.facts.clone(),
                })
                .collect(),
        })
        .collect();
    let result = Challenge {
        profile_id: profile.profile_id.clone(),
        split,
        total: all.len(),
        offset,
        cases,
        can_apply: false,
    };
    ensure!(
        serde_json::to_vec(&result)?.len() <= MAX_CONTEXT_BYTES,
        "challenge exceeds byte budget; reduce page size or shorten facts"
    );
    Ok(result)
}
