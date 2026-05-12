use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RootKind {
    ClaudeGlobal,
    ClaudeProject,
    Codex,
    Cursor,
    AgentsGeneric,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Root {
    pub id: String,
    pub kind: RootKind,
    pub path: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SkillTier {
    Primary,
    Secondary,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Frontmatter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub path: String,
    pub size_bytes: u64,
    pub referenced: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UsageConfidence {
    /// Name is distinctive enough (length >= 6 AND contains '-' or '_') that
    /// matches in session logs are reliable signals.
    High,
    /// Name is short or common (e.g. "auth", "browser"); we don't scan for it
    /// because matches would be dominated by false positives.
    #[default]
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Usage {
    /// Total occurrences across all scanned session logs (Claude + Codex).
    pub mentions: u64,
    /// Number of sessions that mention this skill at least once.
    pub sessions: u64,
    /// ISO-8601 timestamp of the latest session-file mtime that mentioned the skill.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen_at: Option<String>,
    /// Per-source breakdown: { "claude": 12, "codex": 3 }
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty", default)]
    pub by_source: std::collections::BTreeMap<String, u64>,
    /// Reliability of the count. `Low` means the skill name was too generic
    /// to scan for (we don't report numbers we don't trust).
    #[serde(default)]
    pub confidence: UsageConfidence,
}

impl Usage {
    pub fn is_name_reliable(name: &str) -> bool {
        name.len() >= 6 && (name.contains('-') || name.contains('_'))
    }
}

/// Approximate token counts (`chars / 3.7` heuristic, rounded up). Not exact
/// — within ~10% of cl100k_base for English markdown; we use it because the
/// real tokenizer adds a 1MB BPE blob for marginal accuracy on this use case.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Tokens {
    /// Approximate tokens in the frontmatter `description` field.
    pub description: u32,
    /// Approximate tokens in the skill body (after frontmatter).
    pub body: u32,
    /// description + body. Frontmatter envelope itself is negligible.
    pub total: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Validation {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub tier: SkillTier,
    pub name: String,
    pub path: String,
    pub dir: String,
    pub agent: String,
    pub root_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frontmatter: Option<Frontmatter>,
    pub content_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minhash: Option<Vec<u64>>,
    pub assets: Vec<Asset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<String>,
    #[serde(default, skip_serializing_if = "is_default_usage")]
    pub usage: Usage,
    #[serde(default)]
    pub tokens: Tokens,
    #[serde(default)]
    pub validation: Validation,
}

fn is_default_usage(u: &Usage) -> bool {
    u.mentions == 0
        && u.sessions == 0
        && u.last_seen_at.is_none()
        && u.by_source.is_empty()
        && u.confidence == UsageConfidence::Low
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ClusterKind {
    Exact,
    Near,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cluster {
    pub id: String,
    pub kind: ClusterKind,
    pub similarity: f64,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Stats {
    pub scanned_paths: usize,
    pub elapsed_ms: u128,
    pub primary_skills: usize,
    pub secondary_skills: usize,
    pub duplicate_clusters: usize,
    #[serde(default)]
    pub usage_session_files: usize,
    #[serde(default)]
    pub usage_bytes_scanned: u64,
    #[serde(default)]
    pub usage_elapsed_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inventory {
    pub schema_version: u32,
    pub generated_at: String,
    pub roots: Vec<Root>,
    pub skills: Vec<Skill>,
    pub clusters: Vec<Cluster>,
    pub stats: Stats,
}
