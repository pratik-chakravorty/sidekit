//! Saved workflows: one small JSON file each under `SideKit/workflows/`, so
//! they can be shared, diffed or kept in a repo. No UI types in here.

use std::path::PathBuf;

use crate::logic::workflow::Workflow;

pub fn dir() -> Option<PathBuf> {
    Some(crate::settings::config_dir()?.join("SideKit").join("workflows"))
}

pub struct Saved {
    pub file: PathBuf,
    pub flow: Workflow,
}

/// Every readable workflow file, by name. Broken files are skipped, not deleted.
pub fn load_all() -> Vec<Saved> {
    let Some(entries) = dir().and_then(|d| std::fs::read_dir(d).ok()) else { return Vec::new() };
    let mut out: Vec<Saved> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter_map(|file| {
            let flow: Workflow = serde_json::from_slice(&std::fs::read(&file).ok()?).ok()?;
            Some(Saved { file, flow })
        })
        .collect();
    out.sort_by_key(|s| s.flow.name.to_lowercase());
    out
}

/// A file name from the workflow name: "Decode log token" → "decode-log-token.json".
fn file_for(name: &str) -> Option<PathBuf> {
    let mut slug = String::new();
    for c in name.trim().to_lowercase().chars() {
        if c.is_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "workflow" } else { slug };
    Some(dir()?.join(format!("{slug}.json")))
}

/// Write `flow`, replacing `old` (its previous file) when the name changed.
/// Returns the file it now lives in.
pub fn save(flow: &Workflow, old: Option<&PathBuf>) -> std::io::Result<PathBuf> {
    let file = file_for(&flow.name).ok_or_else(|| std::io::Error::other("No settings folder on this system"))?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_vec_pretty(flow).map_err(std::io::Error::other)?;
    std::fs::write(&file, json)?;
    if let Some(old) = old.filter(|o| **o != file) {
        let _ = std::fs::remove_file(old);
    }
    Ok(file)
}

/// Whether saving `name` would overwrite a different workflow's file.
pub fn taken(name: &str, mine: Option<&PathBuf>) -> bool {
    file_for(name).is_some_and(|f| f.exists() && Some(&f) != mine)
}

pub fn delete(file: &PathBuf) -> std::io::Result<()> {
    std::fs::remove_file(file)
}
