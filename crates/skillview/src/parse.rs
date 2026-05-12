use crate::model::Frontmatter;
use anyhow::Result;
use regex::Regex;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

pub struct ParsedSkill {
    pub frontmatter: Option<Frontmatter>,
    pub body: String,
    pub references: BTreeSet<String>,
}

pub fn parse(path: &Path) -> Result<ParsedSkill> {
    let raw = fs::read_to_string(path)?;
    let (fm_raw, body) = split_frontmatter(&raw);
    let frontmatter = fm_raw.and_then(|y| serde_yaml::from_str::<Frontmatter>(y).ok());
    let references = extract_references(body);
    Ok(ParsedSkill {
        frontmatter,
        body: body.to_string(),
        references,
    })
}

fn split_frontmatter(raw: &str) -> (Option<&str>, &str) {
    let trimmed = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let Some(after_open) = trimmed.strip_prefix("---") else {
        return (None, raw);
    };
    let after_open = after_open.strip_prefix('\r').unwrap_or(after_open);
    let after_open = after_open.strip_prefix('\n').unwrap_or(after_open);
    let Some(end_idx) = find_close(after_open) else {
        return (None, raw);
    };
    let fm = &after_open[..end_idx];
    let rest = &after_open[end_idx..];
    let rest = rest.trim_start_matches(|c: char| c == '\n' || c == '\r' || c == '-');
    (Some(fm), rest)
}

fn find_close(after_open: &str) -> Option<usize> {
    let mut search_from = 0;
    while let Some(idx) = after_open[search_from..].find("---") {
        let abs = search_from + idx;
        let at_line_start = abs == 0 || matches!(after_open.as_bytes().get(abs - 1), Some(b'\n'));
        let after = abs + 3;
        let line_ends = matches!(
            after_open.as_bytes().get(after),
            None | Some(b'\n') | Some(b'\r')
        );
        if at_line_start && line_ends {
            return Some(abs);
        }
        search_from = abs + 3;
    }
    None
}

fn extract_references(body: &str) -> BTreeSet<String> {
    static LINK_RE: OnceLock<Regex> = OnceLock::new();
    static INLINE_RE: OnceLock<Regex> = OnceLock::new();
    let link_re = LINK_RE.get_or_init(|| Regex::new(r"\[[^\]]*\]\(([^)\s]+)\)").unwrap());
    let inline_re = INLINE_RE
        .get_or_init(|| Regex::new(r"`((?:\./|\.\./)?[\w./\-]+\.[A-Za-z0-9]+)`").unwrap());

    let mut out = BTreeSet::new();
    for cap in link_re.captures_iter(body) {
        let url = &cap[1];
        if !is_external(url) {
            out.insert(strip_anchor(url).to_string());
        }
    }
    for cap in inline_re.captures_iter(body) {
        let token = &cap[1];
        if looks_like_path(token) {
            out.insert(token.to_string());
        }
    }
    out
}

fn is_external(s: &str) -> bool {
    s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with("mailto:")
        || s.starts_with('#')
}

fn strip_anchor(s: &str) -> &str {
    s.split('#').next().unwrap_or(s)
}

fn looks_like_path(s: &str) -> bool {
    s.contains('/') || s.starts_with("./") || s.starts_with("../")
}

/// Normalize body for hashing/MinHash. Strips inline code fencing and link
/// targets so that visually identical skills hash the same even if a path
/// changed.
pub fn normalize_for_signature(body: &str) -> String {
    static LINK_RE: OnceLock<Regex> = OnceLock::new();
    let link_re = LINK_RE.get_or_init(|| Regex::new(r"\[([^\]]*)\]\([^)]*\)").unwrap());
    let replaced = link_re.replace_all(body, "$1");
    replaced
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
