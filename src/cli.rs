//! Command-line interface definitions for imgcull.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// AI-powered image culling tool.
#[derive(Parser, Debug)]
#[command(name = "imgcull", version, about = "AI-powered image culling tool")]
pub struct Cli {
    /// The subcommand to run.
    #[command(subcommand)]
    pub command: Commands,
}

/// Available subcommands.
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Analyze images: generate descriptions and quality scores
    Score(ProcessArgs),
    /// Generate scene descriptions only (no scoring)
    Describe(ProcessArgs),
    /// Show a summary report of scored images
    Report(ReportArgs),
    /// Create default config files
    Init,
    /// Prepare immutable overview and native-pixel evidence without scoring or XMP writes.
    Prepare(PrepareArgs),
    /// Verify a prepared bundle before reuse.
    VerifyEvidence {
        /// Directory containing evidence.json.
        directory: PathBuf,
    },
    /// Freeze sources and existing XMP for a new resumable review job.
    ReviewInit {
        /// Photos or directories; same-stem companions are counted once.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// New task directory; existing jobs cannot be replaced.
        #[arg(long)]
        output: PathBuf,
        /// Native decoded area required by this job; lower explicitly for older cameras.
        #[arg(long, default_value_t = crate::imaging::DEFAULT_MIN_PIXELS)]
        min_pixels: u64,
    },
    /// Compact recorded/pending progress; does not certify current sources.
    ReviewStatus { directory: PathBuf },
    /// Read one latest draft without replaying its full history.
    ReviewShow { directory: PathBuf, photo_id: String },
    /// List a bounded page of logical photo IDs for scheduling.
    ReviewList {
        directory: PathBuf,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Save a draft observation; never apply its proposals to XMP.
    RecordReview {
        directory: PathBuf,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        photo_id: String,
        #[arg(long)]
        evidence: PathBuf,
        #[arg(long)]
        observation: PathBuf,
    },
    /// Check current sources, visual coverage and group constraints; no XMP writes.
    ReviewCheck { directory: PathBuf },
    /// Undo a metadata transaction, refusing to overwrite subsequent edits.
    Rollback {
        /// Path to .imgcull/transactions/change-*/transaction.json.
        transaction: PathBuf,
    },
}

/// Inputs to full-resolution evidence preparation.
#[derive(clap::Args, Debug)]
pub struct PrepareArgs {
    /// Photo identity (prefer RAW when a RAW/JPEG pair exists).
    pub photo: PathBuf,
    /// A new output directory; existing evidence is immutable.
    #[arg(long)]
    pub output: PathBuf,
    /// JSON array of detail requests in normalized native pixel coordinates.
    #[arg(long)]
    pub regions: Option<PathBuf>,
    /// Minimum decoded native area (45MP default for this user's 45MP camera).
    #[arg(long, default_value_t = crate::imaging::DEFAULT_MIN_PIXELS)]
    pub min_pixels: u64,
}

/// Arguments for the report subcommand.
#[derive(clap::Args, Debug)]
pub struct ReportArgs {
    /// Image files or directories to report on.
    #[arg(required = true)]
    pub paths: Vec<PathBuf>,

    /// Output format.
    #[arg(long, value_enum, default_value = "table")]
    pub format: ReportFormat,

    /// Sort order.
    #[arg(long, value_enum, default_value = "score")]
    pub sort: SortBy,

    /// Sort ascending instead of descending.
    #[arg(long)]
    pub asc: bool,
}

/// Output format for the report command.
#[derive(clap::ValueEnum, Debug, Clone)]
pub enum ReportFormat {
    /// Aligned terminal table.
    Table,
    /// Comma-separated values.
    Csv,
}

/// Sort order for the report command.
#[derive(clap::ValueEnum, Debug, Clone)]
pub enum SortBy {
    /// Sort by overall score.
    Score,
    /// Sort by filename.
    Filename,
    /// Sort by star rating.
    Rating,
}

/// Arguments shared by score and describe subcommands.
#[derive(clap::Args, Debug)]
pub struct ProcessArgs {
    /// Image files or directories to process.
    #[arg(required = true)]
    pub paths: Vec<PathBuf>,

    /// Override both description and scoring provider.
    #[arg(long)]
    pub provider: Option<String>,

    /// Override description provider only.
    #[arg(long)]
    pub description_provider: Option<String>,

    /// Override scoring provider only.
    #[arg(long)]
    pub scoring_provider: Option<String>,

    /// Max parallel LLM requests [default: from config or 4].
    #[arg(long)]
    pub concurrency: Option<usize>,

    /// Skip description generation.
    #[arg(long)]
    pub no_description: bool,

    /// Don't write star rating to xmp:Rating.
    #[arg(long)]
    pub no_rating: bool,

    /// Also keep the first legacy .xmp.bak (recovery transactions are always recorded).
    #[arg(long)]
    pub backup: bool,

    /// Re-score custom imgcull fields; preserve existing human metadata.
    #[arg(long)]
    pub force: bool,

    /// Show what would be processed without calling LLMs.
    #[arg(long)]
    pub dry_run: bool,

    /// Write detailed log to file.
    #[arg(long)]
    pub log: Option<PathBuf>,

    /// Use alternative prompts file.
    #[arg(long)]
    pub prompts: Option<PathBuf>,

    /// Verbose terminal output.
    #[arg(short, long)]
    pub verbose: bool,

    /// Only show errors.
    #[arg(short, long)]
    pub quiet: bool,
}
