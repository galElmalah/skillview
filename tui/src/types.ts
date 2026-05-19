// Mirror of schema/skillview.schema.json — keep in sync.

export type RootKind =
  | "claude-global"
  | "claude-project"
  | "codex"
  | "cursor"
  | "agents-generic"
  | "unknown";

export interface Root {
  id: string;
  kind: RootKind;
  path: string;
}

export type SkillTier = "primary" | "secondary";

export interface Frontmatter {
  name?: string;
  description?: string;
  [key: string]: unknown;
}

export interface Asset {
  path: string;
  size_bytes: number;
  referenced: boolean;
}

export type UsageConfidence = "high" | "low";

export interface Usage {
  mentions: number;
  sessions: number;
  last_seen_at?: string;
  by_source?: Record<string, number>;
  confidence: UsageConfidence;
}

export interface Tokens {
  description: number;
  body: number;
  total: number;
}

export interface Validation {
  ok: boolean;
  issues?: string[];
}

export interface Skill {
  id: string;
  tier: SkillTier;
  name: string;
  path: string;
  dir: string;
  agent: string;
  root_id: string;
  frontmatter?: Frontmatter;
  content_hash: string;
  minhash?: number[];
  assets: Asset[];
  cluster_id?: string;
  usage?: Usage;
  tokens?: Tokens;
  validation?: Validation;
}

export type ClusterKind = "exact" | "near";

export interface Cluster {
  id: string;
  kind: ClusterKind;
  similarity: number;
  members: string[];
}

export interface Stats {
  scanned_paths: number;
  elapsed_ms: number;
  primary_skills: number;
  secondary_skills: number;
  duplicate_clusters: number;
  usage_session_files?: number;
  usage_bytes_scanned?: number;
  usage_elapsed_ms?: number;
}

export interface Inventory {
  schema_version: number;
  generated_at: string;
  roots: Root[];
  skills: Skill[];
  clusters: Cluster[];
  stats: Stats;
}

// ----- streaming events (mirrors emit::Event in Rust) -----

export type StreamPhase = "walk" | "parse" | "cluster" | "usage";

export type StreamEvent =
  | {
      event: "start";
      root: string;
      started_at: string;
      schema_version: number;
    }
  | {
      event: "progress";
      phase: StreamPhase;
      paths_seen: number;
      skills_found: number;
      elapsed_ms: number;
    }
  | { event: "skill"; skill: Skill }
  | { event: "roots"; roots: Root[] }
  | {
      event: "clusters";
      clusters: Cluster[];
      assignments: Record<string, string>;
    }
  | {
      event: "usage";
      by_skill: Record<string, Usage>;
      session_files: number;
      bytes_scanned: number;
      elapsed_ms: number;
    }
  | { event: "done"; stats: Stats; generated_at: string };
