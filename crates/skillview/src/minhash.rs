use std::collections::HashMap;
use std::hash::Hasher;
use twox_hash::XxHash64;

pub const NUM_HASHES: usize = 128;
pub const SHINGLE: usize = 3;

pub fn signature(text: &str) -> Vec<u64> {
    let tokens = tokenize(text);
    let mut sig = vec![u64::MAX; NUM_HASHES];

    if tokens.is_empty() {
        return sig;
    }

    let window = SHINGLE.min(tokens.len());
    let upper = tokens.len().saturating_sub(window) + 1;
    for start in 0..upper {
        let shingle = &tokens[start..start + window];
        for (i, slot) in sig.iter_mut().enumerate() {
            let seed = SEEDS[i % SEEDS.len()].wrapping_mul(i as u64 + 1);
            let mut h = XxHash64::with_seed(seed);
            for tok in shingle {
                h.write(tok.as_bytes());
                h.write_u8(b'\x1f');
            }
            let v = h.finish();
            if v < *slot {
                *slot = v;
            }
        }
    }
    sig
}

pub fn jaccard(a: &[u64], b: &[u64]) -> f64 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let eq = a.iter().zip(b.iter()).filter(|(x, y)| x == y).count();
    eq as f64 / n as f64
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

pub struct Clustering {
    /// skill_id -> cluster_id
    pub assignments: HashMap<String, String>,
    /// cluster_id -> (kind: "exact"|"near", similarity, members)
    pub clusters: Vec<(String, &'static str, f64, Vec<String>)>,
}

/// Cluster by exact-hash first (kind="exact", similarity=1.0), then run
/// MinHash on whatever's left and produce kind="near" connected components.
pub fn cluster(
    items: &[(String, String, Vec<u64>)], // (skill_id, content_hash, minhash)
    threshold: f64,
) -> Clustering {
    let mut assignments: HashMap<String, String> = HashMap::new();
    let mut clusters: Vec<(String, &'static str, f64, Vec<String>)> = Vec::new();

    let mut by_hash: HashMap<&str, Vec<&str>> = HashMap::new();
    for (id, hash, _) in items {
        by_hash.entry(hash.as_str()).or_default().push(id.as_str());
    }
    for (_, ids) in by_hash.iter() {
        if ids.len() > 1 {
            let cid = format!("c_{}", clusters.len());
            for id in ids {
                assignments.insert((*id).to_string(), cid.clone());
            }
            clusters.push((
                cid,
                "exact",
                1.0,
                ids.iter().map(|s| (*s).to_string()).collect(),
            ));
        }
    }

    let remaining: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, (id, _, _))| !assignments.contains_key(id))
        .map(|(i, _)| i)
        .collect();

    let mut parent: Vec<usize> = (0..remaining.len()).collect();
    let mut edge_scores: HashMap<(usize, usize), f64> = HashMap::new();
    for ia in 0..remaining.len() {
        for ib in (ia + 1)..remaining.len() {
            let a = &items[remaining[ia]].2;
            let b = &items[remaining[ib]].2;
            let s = jaccard(a, b);
            if s >= threshold {
                edge_scores.insert((ia, ib), s);
                let ra = find(&mut parent, ia);
                let rb = find(&mut parent, ib);
                if ra != rb {
                    parent[ra] = rb;
                }
            }
        }
    }

    let mut by_root: HashMap<usize, Vec<usize>> = HashMap::new();
    for ix in 0..remaining.len() {
        let r = find(&mut parent, ix);
        by_root.entry(r).or_default().push(ix);
    }

    for (_, group) in by_root {
        if group.len() < 2 {
            continue;
        }
        let mut sum = 0.0;
        let mut count = 0;
        for i in 0..group.len() {
            for j in (i + 1)..group.len() {
                let key = (group[i].min(group[j]), group[i].max(group[j]));
                if let Some(s) = edge_scores.get(&key) {
                    sum += s;
                    count += 1;
                }
            }
        }
        let avg = if count > 0 { sum / count as f64 } else { 0.0 };
        let cid = format!("c_{}", clusters.len());
        let members: Vec<String> = group.iter().map(|i| items[remaining[*i]].0.clone()).collect();
        for m in &members {
            assignments.insert(m.clone(), cid.clone());
        }
        clusters.push((cid, "near", avg, members));
    }

    Clustering {
        assignments,
        clusters,
    }
}

fn find(parent: &mut [usize], i: usize) -> usize {
    if parent[i] == i {
        i
    } else {
        let r = find(parent, parent[i]);
        parent[i] = r;
        r
    }
}

const SEEDS: [u64; 16] = [
    0x9E37_79B9_7F4A_7C15,
    0xBF58_476D_1CE4_E5B9,
    0x94D0_49BB_1331_11EB,
    0xC2B2_AE3D_27D4_EB4F,
    0x165667B19E3779F9,
    0x85EBCA77C2B2AE63,
    0xCC9E2D51DEADBEEF,
    0x1B873593FACEFEED,
    0xA5A5A5A5A5A5A5A5,
    0x5A5A5A5A5A5A5A5A,
    0xD1B54A32D192ED03,
    0x8C2D8B7B96A28C5D,
    0x6364136223846793,
    0xDA942042E4DD58B5,
    0x3243F6A8885A308D,
    0x9D74D34F1C7C8DBB,
];
