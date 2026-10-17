//! Typed photographic observations and conservative proposal validation.
//! This module never writes metadata or infers a visual verdict from file existence.
use crate::evidence::{DetailRole, Evidence};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Versioned user policy, stored with every review job.
pub const POLICY_VERSION: u32 = 2;

/// Meaningful content differences, not a perceptual-hash threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentRelation {
    /// Same useful moment, composition and documentary content.
    Equivalent,
    /// A worthwhile different pose, behavior, angle or detail.
    Distinct,
    /// Semantic comparison is incomplete.
    Unknown,
}

/// Quality of the retained alternative relative to this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlternativeQuality {
    /// A demonstrably stronger alternative.
    Better,
    /// No meaningful quality advantage either way; one can represent both.
    Equivalent,
    /// The proposed alternative loses useful quality.
    Worse,
    /// Native detail/current appearance comparison is incomplete.
    Unknown,
}

/// Explicit semantic comparison. Similarity metrics cannot create this verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Comparison {
    /// Explicitly retained member of the same reviewed burst.
    pub keeper: String,
    /// Whether any meaningful content would be lost.
    pub content: ContentRelation,
    /// Quality of the retained alternative; numerical scores need not differ.
    pub quality: AlternativeQuality,
    /// Concrete explanation of equivalence and the keeper choice.
    pub reason: String,
}

impl Comparison {
    /// Semantic precondition only; peer retention and evidence are checked separately.
    pub fn supports_redundancy(&self) -> bool {
        self.content == ContentRelation::Equivalent
            && matches!(self.quality, AlternativeQuality::Better | AlternativeQuality::Equivalent)
    }
}

/// Sequence membership must be established before automatic deletion advice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SequenceKind {
    /// An individually reviewed photograph.
    Independent,
    /// Comparable moments; keep all good distinct actions and a weak-group fallback.
    Burst,
    /// Astronomy stacking inputs are protected.
    AstroStack,
    /// Exposure bracket inputs are protected.
    Bracket,
    /// Panorama inputs are protected.
    Panorama,
    /// Unresolved grouping must not lead to deletion advice.
    Unknown,
}

/// What an actual reviewer saw, not merely which assets were prepared.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeenView {
    /// Evidence view ID.
    pub id: String,
    /// Encoded view fingerprint actually shown.
    pub sha256: String,
    /// Pixel width submitted to the reviewer.
    pub width: u32,
    /// Pixel height submitted to the reviewer.
    pub height: u32,
}

/// Native inspection needs a validated path through the reviewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Inspection {
    /// A human explicitly inspected the native crop at 1:1.
    HumanNative,
    /// Model processing has not yet been validated against downsampling.
    ModelUnverified,
    /// No native-detail inspection happened.
    Missing,
}

/// Proposed markings are independent of the numeric artistic score.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Proposal {
    /// Preserve metadata and leave the photograph undecided.
    Leave,
    /// Keep a meaningful frame, optionally award stars and suggest editing.
    Retain {
        /// Only calibrated 3, 4 or 5 stars; None permits an unstarred fallback.
        stars: Option<u8>,
        /// Green requires a concrete improvement and current edited appearance.
        green: bool,
    },
    /// Conservative deletion recommendation, never a delete operation.
    Red {
        /// Another retained photo in the same burst, if claiming redundancy.
        replacement: Option<String>,
    },
    /// Red deletion advice for redundancy, independent of artistic score.
    Redundant {
        /// Comparison against a retained, non-defective alternative.
        comparison: Comparison,
    },
}

/// A bounded, explicit observation supplied by a reviewer or future transport.
/// These are declarations, not authenticated proof of human/model behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    /// Must equal the immutable bundle being reviewed.
    pub evidence_id: String,
    /// Provider/model or human reviewer identifier, never credentials.
    pub reviewer: String,
    /// Views actually presented, including the whole composition.
    pub seen: Vec<SeenView>,
    /// Relevant subject/detail was examined through a validated inspection path.
    pub inspection: Inspection,
    /// Reviewer checked all edges/background, including intrusive passersby.
    pub edges_checked: bool,
    /// Uncertainty preserves the frame without marking.
    pub uncertain: bool,
    /// Artistic score 0..=10, separate from Lightroom stars.
    pub artistic_score: Option<u8>,
    /// Unique personal/documentary/behavior/detail value protects a weak frame.
    pub record_value: bool,
    /// Explicit severe focus, shake or accidental-capture defect; not aesthetic dislike.
    pub technical_failure: Option<String>,
    /// Same semantic group ID for actual comparable sequences.
    pub group: String,
    /// Protected sequences and unresolved groups cannot receive red advice.
    pub sequence: SequenceKind,
    /// Concrete crop/color/exposure/perspective changes, bounded notes.
    pub edits: Vec<String>,
    /// Explanation, including why a moment is distinct or replaceable.
    pub reason: String,
    /// Desired metadata proposal; validation does not authorize applying it.
    pub proposal: Proposal,
}

/// Validation result recomputed from the review store. Blocked observations may be
/// resumed or revised; none of these states constitutes XMP authorization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Validation {
    /// True only for the currently implemented observation/proposal checks.
    pub proposal_valid: bool,
    /// Stable machine-readable reasons why a proposal must remain held.
    pub blockers: Vec<String>,
}

fn note(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 4096
}

/// Validate structure and coverage against verified evidence. Missing coverage
/// holds a proposal; malformed or invented view references are rejected.
pub fn validate(observation: &Observation, evidence: &Evidence) -> Result<Validation> {
    ensure!(
        observation.evidence_id == evidence.evidence_id,
        "review/evidence mismatch"
    );
    ensure!(
        note(&observation.reviewer) && note(&observation.reason),
        "missing/oversized review notes"
    );
    ensure!(note(&observation.group), "missing group identity");
    ensure!(
        observation.artistic_score.is_none_or(|n| n <= 10),
        "artistic score must be 0..=10"
    );
    ensure!(
        observation.technical_failure.as_deref().is_none_or(note),
        "invalid technical explanation"
    );
    ensure!(
        observation.edits.len() <= 16 && observation.edits.iter().all(|n| note(n)),
        "invalid edit suggestions"
    );
    ensure!(observation.seen.len() <= 65, "too many viewed assets");
    let mut seen = BTreeSet::new();
    let mut focus = false;
    for receipt in &observation.seen {
        ensure!(seen.insert(receipt.id.as_str()), "duplicate viewed asset");
        let view = evidence
            .views
            .iter()
            .find(|v| v.id == receipt.id)
            .ok_or_else(|| anyhow::anyhow!("invented viewed asset"))?;
        ensure!(
            receipt.sha256 == view.sha256
                && receipt.width == view.width
                && receipt.height == view.height,
            "viewed asset changed or was resized"
        );
        focus |= view
            .detail
            .as_ref()
            .is_some_and(|d| d.role == DetailRole::Focus);
    }
    if let Proposal::Retain {
        stars: Some(stars), ..
    } = observation.proposal
    {
        ensure!((3..=5).contains(&stars), "personal stars must be 3, 4 or 5");
    }
    let mut blockers = Vec::new();
    let mut hold = |condition: bool, code: &str| {
        if condition {
            blockers.push(code.to_string());
        }
    };
    if observation.proposal != Proposal::Leave {
        hold(observation.uncertain, "uncertain");
        hold(!seen.contains("overview"), "composition_not_reviewed");
        hold(
            !focus || observation.inspection != Inspection::HumanNative,
            "native_inspection_unverified",
        );
        hold(!observation.edges_checked, "edges_not_reviewed");
        hold(
            !matches!(
                observation.sequence,
                SequenceKind::Independent | SequenceKind::Burst
            ),
            "protected_or_unknown_sequence",
        );
        // The user chose LR's current developed appearance for artistic value.
        // Current evidence is original JPEG/default RAW. Until a verified LR
        // rendition adapter exists, never accept an artistic/green/star verdict.
        let needs_appearance = match observation.proposal {
            Proposal::Leave => false,
            Proposal::Retain { stars, green } => stars.is_some() || green,
            Proposal::Red { .. } => observation.technical_failure.is_none(),
            Proposal::Redundant { .. } => true,
        };
        hold(needs_appearance, "lightroom_current_appearance_unavailable");
    }
    match &observation.proposal {
        Proposal::Redundant { comparison } => {
            ensure!(note(&comparison.keeper) && note(&comparison.reason), "invalid comparison");
            hold(observation.record_value, "unique_record_value");
            hold(observation.sequence != SequenceKind::Burst, "redundancy_requires_reviewed_burst");
            hold(comparison.content != ContentRelation::Equivalent, "distinct_or_unresolved_content");
            hold(!matches!(comparison.quality, AlternativeQuality::Better | AlternativeQuality::Equivalent), "alternative_quality_unresolved_or_worse");
        }
        Proposal::Leave => (),
        Proposal::Retain { green, .. } => {
            hold(
                *green && observation.edits.is_empty(),
                "green_requires_concrete_edit",
            );
        }
        Proposal::Red { replacement } => {
            hold(observation.record_value, "unique_record_value");
            if observation.technical_failure.is_none() {
                hold(
                    !observation.artistic_score.is_some_and(|n| n <= 5),
                    "red_score_too_high_or_missing",
                );
                hold(
                    observation.artistic_score == Some(5) && replacement.is_none(),
                    "score_five_requires_better_retained_alternative",
                );
            }
        }
    }
    Ok(Validation {
        proposal_valid: blockers.is_empty(),
        blockers,
    })
}

/// Check cross-frame claims using the complete latest group observations.
/// Unlimited distinct good moments are allowed; no top-N rule is imposed.
/// Incomplete groups remain held until their fallback and replacement claims
/// can be verified. The caller must also validate each member's evidence.
pub fn group_blockers(members: &[(&str, &Observation)]) -> Result<Vec<String>> {
    let mut ids = BTreeSet::new();
    for (id, _) in members {
        ensure!(ids.insert(*id), "duplicate photo in group");
    }
    let mut blockers = BTreeSet::new();
    for (id, observation) in members {
        let peers: Vec<_> = members
            .iter()
            .filter(|(_, other)| other.group == observation.group)
            .collect();
        if let Proposal::Redundant { comparison } = &observation.proposal {
            let valid = observation.sequence == SequenceKind::Burst
                && !observation.uncertain
                && !observation.record_value
                && comparison.supports_redundancy()
                && peers.iter().any(|(peer_id, other)| {
                    *peer_id == comparison.keeper && *peer_id != *id
                        && matches!(other.proposal, Proposal::Retain { .. })
                        && !other.uncertain && other.technical_failure.is_none()
                });
            if !valid {
                blockers.insert("redundancy_requires_equivalent_content_and_sound_retained_keeper".to_string());
            }
        }
        ensure!(
            peers
                .iter()
                .all(|(_, other)| other.sequence == observation.sequence),
            "conflicting sequence classification"
        );
        if observation.sequence == SequenceKind::Burst
            && !peers
                .iter()
                .any(|(_, p)| matches!(p.proposal, Proposal::Retain { .. }) && !p.uncertain)
        {
            blockers.insert("burst_requires_retained_representative".to_string());
        }
        if let Proposal::Red {
            replacement: Some(replacement),
        } = &observation.proposal
        {
            let better = peers.iter().find(|(peer_id, _)| peer_id == replacement);
            let valid = better.is_some_and(|(peer_id, other)| {
                *peer_id != *id
                    && observation.sequence == SequenceKind::Burst
                    && !other.uncertain
                    && matches!(other.proposal, Proposal::Retain { .. })
                    && other
                        .artistic_score
                        .zip(observation.artistic_score)
                        .is_some_and(|(a, b)| a > b)
            });
            if !valid {
                blockers.insert("replacement_must_be_better_retained_comparable_frame".to_string());
            }
        }
    }
    Ok(blockers.into_iter().collect())
}
