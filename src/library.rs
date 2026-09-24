//! The AI library: skills, prompts, agents and project rules kept as plain
//! Markdown files, so the library folder itself stays readable, diffable and
//! usable by other tools. No UI types in here.
//!
//! Layout under the app data folder:
//!
//! ```text
//! SideKit/library/
//!   index.json          favorites, timestamps, where an item was imported from
//!   skills/<name>.md
//!   prompts/<name>.md
//!   agents/<name>.md
//!   rules/<name>.md
//! ```

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use fancy_regex::Regex;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------- kinds

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Kind {
    Skill,
    Prompt,
    Agent,
    Rule,
}

pub const KINDS: [Kind; 4] = [Kind::Skill, Kind::Prompt, Kind::Agent, Kind::Rule];

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Skill => "Skill",
            Kind::Prompt => "Prompt",
            Kind::Agent => "Agent",
            Kind::Rule => "Rule",
        }
    }

    pub fn plural(self) -> &'static str {
        match self {
            Kind::Skill => "Skills",
            Kind::Prompt => "Prompts",
            Kind::Agent => "Agents",
            Kind::Rule => "Rules",
        }
    }

    pub fn dir(self) -> &'static str {
        match self {
            Kind::Skill => "skills",
            Kind::Prompt => "prompts",
            Kind::Agent => "agents",
            Kind::Rule => "rules",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Kind::Skill => "gen",
            Kind::Prompt => "lines",
            Kind::Agent => "bot",
            Kind::Rule => "shield",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Kind::Skill => "Reusable workflows an agent loads on demand (SKILL.md)",
            Kind::Prompt => "Task instructions and slash commands you reuse",
            Kind::Agent => "Subagent roles and their instructions",
            Kind::Rule => "Project conventions: AGENTS.md, CLAUDE.md, Cursor rules",
        }
    }

    fn parse(s: &str) -> Option<Kind> {
        Some(match s.trim().to_lowercase().trim_end_matches('s') {
            "skill" => Kind::Skill,
            "prompt" | "command" => Kind::Prompt,
            "agent" | "subagent" => Kind::Agent,
            "rule" | "project rule" | "instruction" => Kind::Rule,
            _ => return None,
        })
    }
}

// ---------------------------------------------------------------- frontmatter

/// The fields SideKit understands in YAML frontmatter.
#[derive(Default, Debug, PartialEq)]
pub struct Front {
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub kind: Option<Kind>,
}

/// Split `---` frontmatter from the body. Returns (yaml, body).
pub fn split_front(text: &str) -> (Option<&str>, &str) {
    let t = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let Some(rest) = t.strip_prefix("---\n").or_else(|| t.strip_prefix("---\r\n")) else {
        return (None, t);
    };
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == "---" {
            let body = &rest[offset + line.len()..];
            return (Some(&rest[..offset]), body);
        }
        offset += line.len();
    }
    (None, t)
}

pub fn parse_front(text: &str) -> Front {
    let (Some(yaml), _) = split_front(text) else { return Front::default() };
    let Ok(serde_yaml::Value::Mapping(m)) = serde_yaml::from_str::<serde_yaml::Value>(yaml) else {
        return Front::default();
    };
    let get = |keys: &[&str]| {
        keys.iter().find_map(|k| match m.get(*k) {
            Some(serde_yaml::Value::String(s)) if !s.trim().is_empty() => Some(s.trim().to_string()),
            _ => None,
        })
    };
    let tags = match m.get("tags").or_else(|| m.get("keywords")) {
        Some(serde_yaml::Value::Sequence(s)) => s.iter().filter_map(|v| v.as_str()).map(str::to_string).collect(),
        Some(serde_yaml::Value::String(s)) => s.split(',').map(str::to_string).collect(),
        _ => Vec::new(),
    };
    Front {
        name: get(&["name", "title"]),
        description: get(&["description", "summary"]),
        tags: clean_tags(tags),
        kind: get(&["type", "kind"]).and_then(|k| Kind::parse(&k)),
    }
}

fn clean_tags(tags: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    tags.into_iter()
        .map(|t| t.trim().trim_start_matches('#').to_lowercase())
        .filter(|t| !t.is_empty() && seen.insert(t.clone()))
        .collect()
}

/// First `# heading` of the body, if any.
fn first_heading(body: &str) -> Option<String> {
    body.lines().find_map(|l| l.strip_prefix("# ")).map(|h| h.trim().to_string()).filter(|h| !h.is_empty())
}

/// First line of prose in the body, for items without a description.
fn first_sentence(body: &str) -> Option<String> {
    let line = body
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("```") && !l.starts_with("---") && !l.starts_with('<'))?;
    // Plain text for the list: drop list markers and inline Markdown emphasis.
    let line = line.trim_start_matches(['-', '*', '>', ' ']).replace(['`', '*'], "");
    let line = line.as_str();
    let mut s: String = line.chars().take(160).collect();
    if line.chars().count() > 160 {
        s.push('…');
    }
    Some(s)
}

// ---------------------------------------------------------------- credentials

#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub what: &'static str,
    /// 1-based line in the stored file.
    pub line: usize,
}

static SECRETS: LazyLock<Vec<(&'static str, Regex)>> = LazyLock::new(|| {
    [
        ("Private key", r"-----BEGIN (?:[A-Z0-9]+ )*PRIVATE KEY-----"),
        ("Authorization header", r"(?i)authorization\s*:\s*(?:bearer|basic|token)\s+[A-Za-z0-9._~+/=-]{12,}"),
        ("AWS access key", r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b"),
        ("GitHub token", r"\b(?:gh[pousr]_[A-Za-z0-9]{36}|github_pat_[A-Za-z0-9_]{40,})"),
        ("Anthropic API key", r"\bsk-ant-[A-Za-z0-9_-]{20,}"),
        ("OpenAI API key", r"\bsk-(?:proj-)?[A-Za-z0-9_-]{20,}"),
        ("Slack token", r"\bxox[abprs]-[A-Za-z0-9-]{10,}"),
        ("Google API key", r"\bAIza[0-9A-Za-z_-]{35}"),
        ("Stripe key", r"\b[sr]k_live_[0-9A-Za-z]{16,}"),
        (
            "Secret value",
            r#"(?i)\b(?:api[_-]?key|secret(?:[_-]?key)?|access[_-]?token|auth[_-]?token|passw(?:or)?d|client[_-]?secret)\b["']?\s*[:=]\s*["']?(?!\$|\{|<|your|xxx|\*\*|example|changeme|placeholder)[A-Za-z0-9_\-/+=.]{12,}"#,
        ),
    ]
    .into_iter()
    .map(|(what, re)| (what, Regex::new(re).expect("valid secret pattern")))
    .collect()
});

/// Lines that look like they hold a credential. Only the kind and line are
/// kept, never the value.
pub fn scan_secrets(text: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if let Some((what, _)) = SECRETS.iter().find(|(_, re)| re.is_match(line).unwrap_or(false)) {
            out.push(Finding { what, line: i + 1 });
        }
    }
    out
}

// ---------------------------------------------------------------- prompt variables

static VAR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{\s*([A-Za-z_][A-Za-z0-9_.-]{0,40})\s*\}\}|\$(ARGUMENTS|[1-9])\b").unwrap());

/// Placeholders in a prompt, in order of first use: `{{name}}`, and Claude
/// Code's `$ARGUMENTS` / `$1`…`$9`.
pub fn variables(text: &str) -> Vec<String> {
    let (_, body) = split_front(text);
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for cap in VAR.captures_iter(body).flatten() {
        let name = cap.get(1).or_else(|| cap.get(2)).map(|m| m.as_str().to_string()).unwrap_or_default();
        let key = if cap.get(2).is_some() { format!("${name}") } else { name };
        if seen.insert(key.clone()) {
            out.push(key);
        }
    }
    out
}

/// The body with placeholders replaced; unfilled ones are left as written.
pub fn fill_variables(text: &str, values: &HashMap<String, String>) -> String {
    let (_, body) = split_front(text);
    VAR.replace_all(body, |c: &fancy_regex::Captures| {
        let key = match (c.get(1), c.get(2)) {
            (Some(n), _) => n.as_str().to_string(),
            (_, Some(n)) => format!("${}", n.as_str()),
            _ => String::new(),
        };
        match values.get(&key).filter(|v| !v.is_empty()) {
            Some(v) => v.clone(),
            None => c[0].to_string(),
        }
    })
    .trim()
    .to_string()
}

// ---------------------------------------------------------------- items

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Meta {
    pub favorite: bool,
    /// Unix seconds.
    pub created: i64,
    pub updated: i64,
    pub used: i64,
    /// Where the item was imported from.
    pub source: Option<String>,
    /// A name inferred at import (e.g. a skill's folder) for files without one.
    pub title: Option<String>,
}

#[derive(Clone)]
pub struct Item {
    pub id: u64,
    /// Path under the library root, with forward slashes: `skills/review.md`.
    pub rel: String,
    pub kind: Kind,
    pub title: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub text: String,
    pub meta: Meta,
    pub findings: Vec<Finding>,
    pub vars: Vec<String>,
    hash: u64,
}

impl Item {
    fn build(rel: String, kind: Kind, text: String, meta: Meta) -> Item {
        let front = parse_front(&text);
        let (_, body) = split_front(&text);
        let stem = rel.rsplit('/').next().unwrap_or(&rel).trim_end_matches(".md").to_string();
        let title = front.name.clone().or_else(|| meta.title.clone()).or_else(|| first_heading(body)).unwrap_or(stem);
        let summary = front.description.clone().or_else(|| first_sentence(body)).unwrap_or_default();
        Item {
            id: fnv(rel.as_bytes()),
            kind,
            title,
            summary,
            tags: front.tags,
            findings: scan_secrets(&text),
            vars: variables(&text),
            hash: content_hash(&text),
            rel,
            text,
            meta,
        }
    }

    /// The body without frontmatter, as pasted into a chat.
    pub fn body(&self) -> &str {
        split_front(&self.text).1.trim_start_matches(['\n', '\r'])
    }

    pub fn slug(&self) -> String {
        slug(&self.title)
    }
}

fn fnv(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, x| (h ^ *x as u64).wrapping_mul(0x100_0000_01b3))
}

/// Hash of the text with line endings, trailing spaces and outer blank lines
/// normalized, so the same file copied between repositories is one item.
pub fn content_hash(text: &str) -> u64 {
    let norm: Vec<&str> = text.lines().map(str::trim_end).collect();
    fnv(norm.join("\n").trim().as_bytes())
}

pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').chars().take(60).collect::<String>();
    if out.is_empty() { "untitled".into() } else { out }
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

pub fn template(kind: Kind) -> String {
    match kind {
        Kind::Skill => "---\nname: new-skill\ndescription: What this skill does and when an agent should use it.\ntags: []\n---\n\n# New skill\n\n## When to use\n\n- \n\n## Steps\n\n1. \n".into(),
        Kind::Prompt => "---\nname: new-prompt\ndescription: A reusable prompt. Placeholders like {{topic}} can be filled in before copying.\ntags: []\n---\n\nReview {{topic}} and list the three riskiest changes.\n".into(),
        Kind::Agent => "---\nname: new-agent\ndescription: What this agent is for, and when to delegate to it.\ntags: []\n---\n\nYou are a focused specialist. Your job is to …\n".into(),
        Kind::Rule => "---\nname: project-rules\ndescription: Conventions an agent should follow in this project.\ntags: []\n---\n\n# Project rules\n\n- Build with …\n- Test with …\n".into(),
    }
}

// ---------------------------------------------------------------- library

#[derive(Default, Serialize, Deserialize)]
struct Index {
    items: HashMap<String, Meta>,
}

pub struct Library {
    root: PathBuf,
    pub items: Vec<Item>,
}

#[derive(Default, Debug, PartialEq)]
pub struct ImportReport {
    pub added: usize,
    pub duplicates: usize,
    pub flagged: usize,
    pub unreadable: usize,
    /// The first added item, to select afterwards.
    pub first: Option<u64>,
}

impl ImportReport {
    pub fn summary(&self) -> String {
        if self.added + self.duplicates + self.unreadable == 0 {
            return "Nothing to import: no skills, prompts, agents or rules were found there".into();
        }
        let mut parts = vec![format!("Imported {}", crate::logic::plural(self.added, "item"))];
        if self.duplicates > 0 {
            parts.push(format!("{} already in the library", self.duplicates));
        }
        if self.flagged > 0 {
            parts.push(format!("{} may contain credentials", self.flagged));
        }
        if self.unreadable > 0 {
            parts.push(format!("{} unreadable", self.unreadable));
        }
        parts.join(" · ")
    }
}

/// Folders a scan never enters.
const SKIP_DIRS: &[&str] = &["node_modules", ".git", "target", "dist", "build", ".venv", "venv", "__pycache__", ".next", "vendor", "Pods"];
const MAX_FILE: u64 = 512 * 1024;

impl Library {
    /// `SIDEKIT_LIBRARY` points the app at another library folder.
    pub fn default_root() -> Option<PathBuf> {
        if let Some(dir) = std::env::var_os("SIDEKIT_LIBRARY") {
            return Some(PathBuf::from(dir));
        }
        crate::settings::config_dir().map(|d| d.join("SideKit").join("library"))
    }

    pub fn open(root: PathBuf) -> Library {
        let index: Index = std::fs::read(root.join("index.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let mut items = Vec::new();
        for kind in KINDS {
            let Ok(dir) = std::fs::read_dir(root.join(kind.dir())) else { continue };
            for entry in dir.flatten() {
                let path = entry.path();
                if path.extension().is_none_or(|e| e != "md") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else { continue };
                let rel = format!("{}/{}", kind.dir(), entry.file_name().to_string_lossy());
                let mut meta = index.items.get(&rel).cloned().unwrap_or_default();
                if meta.created == 0 {
                    let mtime = entry.metadata().ok().and_then(|m| m.modified().ok());
                    let secs = mtime.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(now(), |d| d.as_secs() as i64);
                    meta.created = secs;
                    meta.updated = secs;
                }
                items.push(Item::build(rel, kind, text, meta));
            }
        }
        Library { root, items }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn get(&self, id: u64) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }

    pub fn path_of(&self, item: &Item) -> PathBuf {
        self.root.join(&item.rel)
    }

    fn save_index(&self) {
        let index = Index { items: self.items.iter().map(|i| (i.rel.clone(), i.meta.clone())).collect() };
        let _ = std::fs::create_dir_all(&self.root);
        if let Ok(json) = serde_json::to_vec_pretty(&index) {
            let _ = std::fs::write(self.root.join("index.json"), json);
        }
    }

    /// A file name under `kind`'s folder that nothing uses yet.
    fn free_rel(&self, kind: Kind, name: &str) -> String {
        let base = slug(name);
        let taken = |rel: &str| self.items.iter().any(|i| i.rel == rel) || self.root.join(rel).exists();
        let mut rel = format!("{}/{base}.md", kind.dir());
        let mut n = 2;
        while taken(&rel) {
            rel = format!("{}/{base}-{n}.md", kind.dir());
            n += 1;
        }
        rel
    }

    fn insert(&mut self, kind: Kind, name: &str, text: String, meta: Meta) -> std::io::Result<u64> {
        let rel = self.free_rel(kind, name);
        let path = self.root.join(&rel);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, &text)?;
        let item = Item::build(rel, kind, text, meta);
        let id = item.id;
        self.items.push(item);
        self.save_index();
        Ok(id)
    }

    pub fn create(&mut self, kind: Kind) -> std::io::Result<u64> {
        let t = now();
        let name = format!("new {}", kind.label().to_lowercase());
        self.insert(kind, &name, template(kind), Meta { created: t, updated: t, ..Default::default() })
    }

    /// Replace an item's text and write it out.
    pub fn set_text(&mut self, id: u64, text: String) -> std::io::Result<()> {
        let Some(i) = self.items.iter().position(|i| i.id == id) else { return Ok(()) };
        let old = &self.items[i];
        if old.text == text {
            return Ok(());
        }
        std::fs::write(self.root.join(&old.rel), &text)?;
        let mut meta = old.meta.clone();
        meta.updated = now();
        self.items[i] = Item::build(old.rel.clone(), old.kind, text, meta);
        self.save_index();
        Ok(())
    }

    /// Move an item to another kind's folder. Returns its new id.
    pub fn set_kind(&mut self, id: u64, kind: Kind) -> std::io::Result<u64> {
        let Some(i) = self.items.iter().position(|i| i.id == id) else { return Ok(id) };
        if self.items[i].kind == kind {
            return Ok(id);
        }
        let old = self.items.remove(i);
        let rel = self.free_rel(kind, old.rel.rsplit('/').next().unwrap_or("item").trim_end_matches(".md"));
        let to = self.root.join(&rel);
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir)?;
        }
        if let Err(e) = std::fs::rename(self.root.join(&old.rel), &to) {
            self.items.insert(i, old);
            return Err(e);
        }
        let item = Item::build(rel, kind, old.text, old.meta);
        let new_id = item.id;
        self.items.insert(i, item);
        self.save_index();
        Ok(new_id)
    }

    pub fn delete(&mut self, id: u64) -> std::io::Result<()> {
        let Some(i) = self.items.iter().position(|i| i.id == id) else { return Ok(()) };
        let path = self.root.join(&self.items[i].rel);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        self.items.remove(i);
        self.save_index();
        Ok(())
    }

    pub fn toggle_favorite(&mut self, id: u64) {
        if let Some(i) = self.items.iter_mut().find(|i| i.id == id) {
            i.meta.favorite = !i.meta.favorite;
            self.save_index();
        }
    }

    /// Record that an item was copied or installed, for "recently used".
    pub fn touch(&mut self, id: u64) {
        if let Some(i) = self.items.iter_mut().find(|i| i.id == id) {
            i.meta.used = now();
            self.save_index();
        }
    }

    #[cfg(test)]
    pub fn tags(&self) -> Vec<(String, usize)> {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for i in &self.items {
            for t in &i.tags {
                *counts.entry(t).or_default() += 1;
            }
        }
        let mut v: Vec<(String, usize)> = counts.into_iter().map(|(t, n)| (t.to_string(), n)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        v
    }

    #[cfg(test)]
    pub fn import(&mut self, paths: &[PathBuf]) -> ImportReport {
        self.add_found(gather(paths))
    }

    /// Add what [`gather`] found, skipping content the library already has.
    pub fn add_found(&mut self, found: Vec<Found>) -> ImportReport {
        let mut report = ImportReport::default();
        let mut hashes: HashSet<u64> = self.items.iter().map(|i| i.hash).collect();
        for Found { path, kind: guessed, text } in found {
            let Some(text) = text else {
                report.unreadable += 1;
                continue;
            };
            if !hashes.insert(content_hash(&text)) {
                report.duplicates += 1;
                continue;
            }
            let front = parse_front(&text);
            let kind = front.kind.unwrap_or(guessed);
            let title = inferred_title(&path);
            let name = front.name.clone().or_else(|| title.clone()).unwrap_or_else(|| "imported".into());
            let t = now();
            let meta = Meta {
                created: t,
                updated: t,
                source: Some(path.to_string_lossy().into_owned()),
                title: if front.name.is_none() && first_heading(split_front(&text).1).is_none() { title } else { None },
                ..Default::default()
            };
            let flagged = !scan_secrets(&text).is_empty();
            match self.insert(kind, &name, text, meta) {
                Ok(id) => {
                    report.added += 1;
                    report.flagged += flagged as usize;
                    report.first.get_or_insert(id);
                }
                Err(_) => report.unreadable += 1,
            }
        }
        report
    }

    /// Write `item` where `target` expects it. Existing files are only
    /// replaced with `overwrite`.
    pub fn install(&mut self, id: u64, target: Target, project: Option<&Path>, overwrite: bool) -> Result<PathBuf, InstallError> {
        let item = self.get(id).ok_or(InstallError::Other("The item no longer exists".into()))?;
        let path = target.path(item, project).ok_or(InstallError::Other("No destination for this item".into()))?;
        let text = target.render(item);
        if let Ok(existing) = std::fs::read_to_string(&path) {
            if content_hash(&existing) == content_hash(&text) {
                self.touch(id);
                return Ok(path);
            }
            if !overwrite {
                return Err(InstallError::Exists(path));
            }
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| InstallError::Other(e.to_string()))?;
        }
        std::fs::write(&path, text).map_err(|e| InstallError::Other(e.to_string()))?;
        self.touch(id);
        Ok(path)
    }
}

/// A candidate file and its contents, read off the UI thread.
pub struct Found {
    pub path: PathBuf,
    pub kind: Kind,
    /// `None` when unreadable, empty or too large.
    pub text: Option<String>,
}

/// Collect importable files. Folders are scanned for the usual conventions;
/// Markdown files given directly are taken whatever their name.
pub fn gather(paths: &[PathBuf]) -> Vec<Found> {
    let mut found = Vec::new();
    for p in paths {
        if p.is_dir() {
            scan_dir(p, 0, &mut found);
        } else if let Some(kind) = recognize(p).or_else(|| is_markdown(p).then_some(Kind::Prompt)) {
            found.push((p.clone(), kind));
        }
    }
    found
        .into_iter()
        .map(|(path, kind)| {
            let text = match std::fs::metadata(&path) {
                Ok(m) if m.len() <= MAX_FILE => std::fs::read_to_string(&path).ok(),
                _ => None,
            };
            Found { path, kind, text: text.filter(|t| !t.trim().is_empty()) }
        })
        .collect()
}

fn is_markdown(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("mdc") || e.eq_ignore_ascii_case("markdown"))
}

/// What a file is, going by the conventions of Claude Code, Codex and Cursor.
pub fn recognize(path: &Path) -> Option<Kind> {
    let name = path.file_name()?.to_string_lossy().to_lowercase();
    match name.as_str() {
        "skill.md" => return Some(Kind::Skill),
        "agent.md" => return Some(Kind::Agent),
        "agents.md" | "claude.md" | "gemini.md" | ".cursorrules" | ".windsurfrules" | "copilot-instructions.md" => return Some(Kind::Rule),
        "readme.md" | "changelog.md" | "license.md" | "contributing.md" => return None,
        _ => {}
    }
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("mdc")) {
        return Some(Kind::Rule);
    }
    if !is_markdown(path) {
        return None;
    }
    let mut tool_dir = false;
    for comp in path.parent()?.components().rev() {
        let c = comp.as_os_str().to_string_lossy().to_lowercase();
        match c.as_str() {
            "skills" => return Some(Kind::Skill),
            "agents" => return Some(Kind::Agent),
            "prompts" | "commands" => return Some(Kind::Prompt),
            "instructions" => return Some(Kind::Rule),
            "rules" => return Some(Kind::Rule),
            ".claude" | ".codex" | ".cursor" | ".copilot" => tool_dir = true,
            _ => {}
        }
    }
    tool_dir.then_some(Kind::Prompt)
}

fn scan_dir(dir: &Path, depth: usize, out: &mut Vec<(PathBuf, Kind)>) {
    if depth > 10 || out.len() > 5000 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    for e in entries.flatten() {
        let p = e.path();
        match e.file_type() {
            Ok(t) if t.is_dir() => {
                let n = e.file_name().to_string_lossy().into_owned();
                if !SKIP_DIRS.contains(&n.as_str()) {
                    dirs.push(p);
                }
            }
            Ok(t) if t.is_file() => files.push(p),
            _ => {}
        }
    }
    files.sort();
    dirs.sort();
    // A skill folder is one SKILL.md plus its resources: take only the SKILL.md.
    if let Some(skill) = files.iter().find(|f| f.file_name().is_some_and(|n| n.eq_ignore_ascii_case("SKILL.md"))) {
        out.push((skill.clone(), Kind::Skill));
        return;
    }
    out.extend(files.into_iter().filter_map(|f| recognize(&f).map(|k| (f, k))));
    for d in dirs {
        scan_dir(&d, depth + 1, out);
    }
}

/// A name for a file with none of its own: a skill's folder, or the project
/// an AGENTS.md belongs to.
fn inferred_title(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_string_lossy().into_owned();
    let parent = path.parent().and_then(|p| p.file_name()).map(|p| p.to_string_lossy().into_owned());
    match name.to_lowercase().as_str() {
        "skill.md" | "agent.md" => parent,
        "agents.md" | "claude.md" | "gemini.md" | ".cursorrules" | "copilot-instructions.md" => {
            Some(match parent.filter(|p| !p.starts_with('.')) {
                Some(p) => format!("{p} {name}"),
                None => name,
            })
        }
        // Copilot's `review.agent.md`, `fix.prompt.md`, `rust.instructions.md`.
        _ => path.file_stem().map(|s| {
            let s = s.to_string_lossy();
            [".agent", ".prompt", ".instructions", ".chatmode"].iter().find_map(|x| s.strip_suffix(x)).unwrap_or(&s).to_string()
        }),
    }
}

/// Folders where AI tools keep user-level skills, prompts and agents.
pub fn known_locations() -> Vec<PathBuf> {
    let Some(home) = home_dir() else { return Vec::new() };
    [
        ".claude/skills",
        ".claude/agents",
        ".claude/commands",
        ".claude/CLAUDE.md",
        ".codex/prompts",
        ".codex/skills",
        ".codex/AGENTS.md",
        ".copilot/skills",
        ".copilot/agents",
        ".cursor/rules",
        ".cursor/commands",
    ]
    .iter()
    .map(|p| home.join(p))
    .filter(|p| p.exists())
    .collect()
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from)
}

// ---------------------------------------------------------------- install targets

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    /// `~/.claude/…`: skills, agents (subagents), prompts (slash commands).
    ClaudeUser,
    /// `<project>/.claude/…`, or `CLAUDE.md` for rules.
    ClaudeProject,
    /// `~/.codex/…`: skills, prompts, and the global `AGENTS.md` for rules.
    CodexUser,
    /// `<project>/.cursor/rules/*.mdc` or `.cursor/commands/*.md`.
    CursorProject,
    /// `<project>/AGENTS.md`.
    AgentsMd,
    /// `~/.copilot/…`: personal skills and Copilot CLI custom agents.
    CopilotUser,
    /// `<project>/.github/…`: skills, `*.agent.md`, `*.prompt.md`, and
    /// `copilot-instructions.md` for rules.
    CopilotProject,
}

#[derive(Debug)]
pub enum InstallError {
    /// A different file is already there.
    Exists(PathBuf),
    Other(String),
}

impl Target {
    pub fn for_kind(kind: Kind) -> Vec<Target> {
        match kind {
            Kind::Skill => vec![Target::ClaudeUser, Target::ClaudeProject, Target::CodexUser, Target::CopilotUser, Target::CopilotProject],
            Kind::Prompt => vec![Target::ClaudeUser, Target::ClaudeProject, Target::CodexUser, Target::CursorProject, Target::CopilotProject],
            Kind::Agent => vec![Target::ClaudeUser, Target::ClaudeProject, Target::CopilotUser, Target::CopilotProject],
            Kind::Rule => vec![Target::AgentsMd, Target::ClaudeProject, Target::CursorProject, Target::CopilotProject, Target::CodexUser],
        }
    }

    pub fn needs_project(self) -> bool {
        matches!(self, Target::ClaudeProject | Target::CursorProject | Target::AgentsMd | Target::CopilotProject)
    }

    pub fn label(self, kind: Kind) -> String {
        let what = match (self, kind) {
            (Target::AgentsMd, _) => return "Project AGENTS.md…".into(),
            (Target::ClaudeProject, Kind::Rule) => return "Project CLAUDE.md…".into(),
            (Target::CodexUser, Kind::Rule) => return "Codex · global AGENTS.md".into(),
            (Target::CursorProject, Kind::Rule) => return "Cursor · project rule…".into(),
            (Target::CursorProject, _) => return "Cursor · project command…".into(),
            (Target::CopilotProject, Kind::Rule) => return "GitHub Copilot · repository instructions…".into(),
            (Target::CopilotUser | Target::CopilotProject, Kind::Agent) => "custom agent",
            (Target::CopilotProject, Kind::Prompt) => "prompt file",
            (_, Kind::Skill) => "skill",
            (_, Kind::Agent) => "subagent",
            (_, Kind::Prompt) => if self == Target::CodexUser { "prompt" } else { "slash command" },
            (_, Kind::Rule) => "rule",
        };
        match self {
            Target::ClaudeUser => format!("Claude Code · user {what}"),
            Target::ClaudeProject => format!("Claude Code · project {what}…"),
            Target::CodexUser => format!("Codex · user {what}"),
            Target::CopilotUser => format!("GitHub Copilot · user {what}"),
            Target::CopilotProject => format!("GitHub Copilot · project {what}…"),
            _ => unreachable!(),
        }
    }

    /// Where the file lands; `None` when this target needs a project folder
    /// that was not given.
    pub fn path(self, item: &Item, project: Option<&Path>) -> Option<PathBuf> {
        let slug = item.slug();
        let base = match self {
            Target::ClaudeUser => home_dir()?.join(".claude"),
            Target::CodexUser => home_dir()?.join(".codex"),
            Target::ClaudeProject => project?.join(".claude"),
            Target::CursorProject => project?.join(".cursor"),
            Target::CopilotUser => home_dir()?.join(".copilot"),
            Target::CopilotProject => project?.join(".github"),
            Target::AgentsMd => return Some(project?.join("AGENTS.md")),
        };
        Some(match (self, item.kind) {
            (Target::ClaudeProject, Kind::Rule) => project?.join("CLAUDE.md"),
            (Target::CodexUser, Kind::Rule) => base.join("AGENTS.md"),
            (Target::CursorProject, Kind::Rule) => base.join("rules").join(format!("{slug}.mdc")),
            (Target::CursorProject, _) => base.join("commands").join(format!("{slug}.md")),
            (Target::CopilotProject, Kind::Rule) => base.join("copilot-instructions.md"),
            (Target::CopilotProject, Kind::Prompt) => base.join("prompts").join(format!("{slug}.prompt.md")),
            (Target::CopilotUser | Target::CopilotProject, Kind::Agent) => base.join("agents").join(format!("{slug}.agent.md")),
            (Target::CopilotUser, Kind::Prompt | Kind::Rule) => return None,
            (_, Kind::Skill) => base.join("skills").join(&slug).join("SKILL.md"),
            (_, Kind::Agent) => base.join("agents").join(format!("{slug}.md")),
            (Target::CodexUser, Kind::Prompt) => base.join("prompts").join(format!("{slug}.md")),
            (_, Kind::Prompt) => base.join("commands").join(format!("{slug}.md")),
            _ => return None,
        })
    }

    /// The file content each tool expects: skills and subagents need `name`
    /// and `description`, rule files are plain Markdown, Cursor rules carry
    /// their own frontmatter.
    pub fn render(self, item: &Item) -> String {
        let body = item.body().trim_end();
        let desc = item.summary.replace('\n', " ");
        let yaml_str = |s: &str| serde_json::to_string(s).unwrap_or_default();
        match (self, item.kind) {
            (_, Kind::Skill) | (_, Kind::Agent) => {
                let front = parse_front(&item.text);
                if front.name.is_some() && front.description.is_some() {
                    return item.text.trim_end().to_string() + "\n";
                }
                format!("---\nname: {}\ndescription: {}\n---\n\n{body}\n", item.slug(), yaml_str(&desc))
            }
            (Target::CursorProject, Kind::Rule) => {
                format!("---\ndescription: {}\nalwaysApply: true\n---\n\n{body}\n", yaml_str(&desc))
            }
            (_, Kind::Rule) => format!("{body}\n"),
            (_, Kind::Prompt) => {
                if desc.is_empty() {
                    format!("{body}\n")
                } else {
                    format!("---\ndescription: {}\n---\n\n{body}\n", yaml_str(&desc))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sidekit-lib-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn frontmatter() {
        let t = "---\nname: code-review\ndescription: Review a diff\ntags: [Rust, review, rust]\ntype: skill\n---\n\n# Hi\nBody";
        let f = parse_front(t);
        assert_eq!(f.name.as_deref(), Some("code-review"));
        assert_eq!(f.tags, ["rust", "review"]);
        assert_eq!(f.kind, Some(Kind::Skill));
        assert_eq!(split_front(t).1, "\n# Hi\nBody");
        assert_eq!(parse_front("tags: a, b").name, None);
        assert_eq!(parse_front("---\ntags: \"a, #B\"\n---\n").tags, ["a", "b"]);
        assert_eq!(split_front("---\nno end").0, None);
    }

    #[test]
    fn recognizes_conventions() {
        let r = |p: &str| recognize(Path::new(p));
        assert_eq!(r("repo/.claude/skills/review/SKILL.md"), Some(Kind::Skill));
        assert_eq!(r("repo/AGENTS.md"), Some(Kind::Rule));
        assert_eq!(r("repo/.cursor/rules/style.mdc"), Some(Kind::Rule));
        assert_eq!(r("home/.claude/commands/fix.md"), Some(Kind::Prompt));
        assert_eq!(r("home/.claude/agents/tester.md"), Some(Kind::Agent));
        assert_eq!(r("repo/docs/guide.md"), None);
        assert_eq!(r("repo/.github/copilot-instructions.md"), Some(Kind::Rule));
        assert_eq!(r("repo/.github/instructions/rust.instructions.md"), Some(Kind::Rule));
        assert_eq!(r("repo/.github/prompts/fix.prompt.md"), Some(Kind::Prompt));
        assert_eq!(r("repo/.github/agents/reviewer.agent.md"), Some(Kind::Agent));
        assert_eq!(inferred_title(Path::new("repo/.github/agents/reviewer.agent.md")).as_deref(), Some("reviewer"));
        assert_eq!(r("repo/prompts/README.md"), None);
        assert_eq!(r("repo/src/main.rs"), None);
    }

    #[test]
    fn secrets_are_flagged_without_values() {
        let t = "fine\nAuthorization: Bearer abcdefghijklmnop1234\napi_key = \"sk-ant-api03-aaaaaaaaaaaaaaaaaaaaaaaa\"\napi_key: ${API_KEY}\npassword: <your-password>\n-----BEGIN OPENSSH PRIVATE KEY-----";
        let f = scan_secrets(t);
        assert_eq!(f.iter().map(|f| (f.what, f.line)).collect::<Vec<_>>(), [
            ("Authorization header", 2),
            ("Anthropic API key", 3),
            ("Private key", 6)
        ]);
        assert!(scan_secrets("AKIAABCDEFGHIJKLMNOP").len() == 1);
        assert!(scan_secrets("Use the token from the vault").is_empty());
    }

    #[test]
    fn prompt_variables() {
        let t = "---\nname: x\n---\nReview {{ file }} for {{focus}}; then {{file}} again. Args: $ARGUMENTS, $1";
        assert_eq!(variables(t), ["file", "focus", "$ARGUMENTS", "$1"]);
        let mut v = HashMap::new();
        v.insert("file".to_string(), "main.rs".to_string());
        v.insert("$ARGUMENTS".to_string(), "--fast".to_string());
        assert_eq!(fill_variables(t, &v), "Review main.rs for {{focus}}; then main.rs again. Args: --fast, $1");
    }

    #[test]
    fn slugs_and_hashes() {
        assert_eq!(slug("Code Review: Rust!"), "code-review-rust");
        assert_eq!(slug("   "), "untitled");
        assert_eq!(content_hash("a  \r\nb\n\n"), content_hash("a\nb"));
        assert_ne!(content_hash("a\nb"), content_hash("a\nc"));
    }

    #[test]
    fn import_dedupes_and_round_trips() {
        let src = temp_root("src");
        let skill = src.join("repo-a/.claude/skills/review");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(skill.join("SKILL.md"), "# Review\nCheck the diff.\n").unwrap();
        std::fs::write(skill.join("notes.md"), "resource, not a skill").unwrap();
        let copy = src.join("repo-b/.claude/skills/review");
        std::fs::create_dir_all(&copy).unwrap();
        std::fs::write(copy.join("SKILL.md"), "# Review\r\nCheck the diff.  \r\n").unwrap();
        std::fs::write(src.join("repo-a/AGENTS.md"), "Use cargo.\ntoken = ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n").unwrap();
        std::fs::create_dir_all(src.join("repo-a/node_modules/x/prompts")).unwrap();
        std::fs::write(src.join("repo-a/node_modules/x/prompts/p.md"), "ignored").unwrap();

        let root = temp_root("lib");
        let mut lib = Library::open(root.clone());
        let r = lib.import(&[src.clone()]);
        assert_eq!((r.added, r.duplicates, r.flagged), (2, 1, 1));
        let again = lib.import(&[src.clone()]);
        assert_eq!((again.added, again.duplicates), (0, 3));

        let rule = lib.items.iter().find(|i| i.kind == Kind::Rule).unwrap();
        assert_eq!(rule.title, "repo-a AGENTS.md");
        assert_eq!(rule.findings[0].what, "GitHub token");
        let id = lib.items.iter().find(|i| i.kind == Kind::Skill).unwrap().id;
        lib.toggle_favorite(id);
        lib.set_text(id, "---\nname: review\ndescription: Review diffs\ntags: [git]\n---\nCheck it.".into()).unwrap();

        let reopened = Library::open(root.clone());
        let s = reopened.get(id).unwrap();
        assert!(s.meta.favorite);
        assert_eq!((s.title.as_str(), s.summary.as_str()), ("review", "Review diffs"));
        assert_eq!(reopened.tags(), [("git".to_string(), 1)]);

        let mut lib = reopened;
        let moved = lib.set_kind(id, Kind::Prompt).unwrap();
        assert!(lib.get(moved).unwrap().rel.starts_with("prompts/"));
        lib.delete(moved).unwrap();
        assert_eq!(Library::open(root.clone()).items.len(), 1);
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn install_targets() {
        let root = temp_root("install");
        let project = root.join("proj");
        let mut lib = Library::open(root.join("lib"));
        let id = lib.create(Kind::Rule).unwrap();
        let p = lib.install(id, Target::AgentsMd, Some(&project), false).unwrap();
        assert_eq!(p, project.join("AGENTS.md"));
        assert!(!std::fs::read_to_string(&p).unwrap().starts_with("---"));
        // Same content again is fine; different content needs confirmation.
        assert!(lib.install(id, Target::AgentsMd, Some(&project), false).is_ok());
        std::fs::write(&p, "someone else's rules").unwrap();
        assert!(matches!(lib.install(id, Target::AgentsMd, Some(&project), false), Err(InstallError::Exists(_))));
        assert!(lib.install(id, Target::AgentsMd, Some(&project), true).is_ok());

        let mdc = lib.install(id, Target::CursorProject, Some(&project), false).unwrap();
        assert!(mdc.ends_with("project-rules.mdc"));
        assert!(std::fs::read_to_string(mdc).unwrap().contains("alwaysApply: true"));

        let skill = lib.create(Kind::Skill).unwrap();
        lib.set_text(skill, "# Deploy\nShip it.".into()).unwrap();
        let item = lib.get(skill).unwrap();
        let path = Target::ClaudeProject.path(item, Some(&project)).unwrap();
        assert!(path.ends_with(".claude/skills/deploy/SKILL.md"));
        assert!(Target::ClaudeProject.render(item).starts_with("---\nname: deploy\ndescription: \"Ship it.\"\n---"));
        assert!(Target::ClaudeProject.path(item, None).is_none());
        assert!(Target::CopilotProject.path(item, Some(&project)).unwrap().ends_with(".github/skills/deploy/SKILL.md"));

        let p = lib.install(id, Target::CopilotProject, Some(&project), false).unwrap();
        assert_eq!(p, project.join(".github").join("copilot-instructions.md"));
        assert!(!std::fs::read_to_string(&p).unwrap().starts_with("---"));
        assert!(Target::CopilotUser.path(lib.get(id).unwrap(), None).is_none());
        let prompt = lib.create(Kind::Prompt).unwrap();
        let path = Target::CopilotProject.path(lib.get(prompt).unwrap(), Some(&project)).unwrap();
        assert!(path.ends_with(".github/prompts/new-prompt.prompt.md"));
        let agent = lib.create(Kind::Agent).unwrap();
        let path = Target::CopilotUser.path(lib.get(agent).unwrap(), None).unwrap();
        assert!(path.ends_with(".copilot/agents/new-agent.agent.md"));
        assert!(Target::CopilotUser.render(lib.get(agent).unwrap()).contains("description:"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
