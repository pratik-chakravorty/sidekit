//! The AI library page: a grouped list of skills, prompts, agents and rules
//! beside the selected one, shown as the document it is.
//!
//! Kept deliberately quiet: one search box instead of filter rows, one primary
//! action per item (Copy for prompts, Install for the rest) with everything
//! else in the ⋯ menu, and messages as a toast that goes away by itself.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use gpui_kit::component::input::{EditorState, InputState};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::text::TextView;
use gpui_kit::{
    AnyElement, App, AppContext, ClipboardItem, Context, Entity, EventEmitter, ExternalPaths, FontWeight,
    InteractiveElement, IntoElement, KeyDownEvent, ParentElement, PathPromptOptions, Render, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Task, Window, div, prelude::FluentBuilder, px,
    relative,
};

use crate::icons::icon;
use crate::id;
use crate::library::{self, ImportReport, InstallError, Item, KINDS, Kind, Library, Target};
use crate::theme::{MONO_FONT, Pal};
use crate::tools::{code_editor, code_editor_el, field_el, is_focused, line, line_text, set_line, set_text, text_of, watch};
use crate::ui::{self, BtnKind, MenuEntry, Tone};

const SAVE_DELAY: Duration = Duration::from_millis(400);
/// How long a message stays up; confirmations stay until answered.
const TOAST_FOR: Duration = Duration::from_secs(4);

/// What the palette needs to list library items.
pub struct PaletteItem {
    pub id: u64,
    pub title: String,
    pub summary: String,
    pub kind: Kind,
    pub tags: Vec<String>,
}

/// Everything the library can do, so the command palette can offer it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LibAction {
    New(Kind),
    ImportFolder,
    ImportFiles,
    Scan,
    OpenFolder,
    Copy,
    CopyRaw,
    Pin,
    Install(Target),
    SaveAs,
    Reveal,
    OpenExternal,
    Edit,
    Done,
    MoveTo(Kind),
    Delete,
}

pub struct PaletteCommand {
    pub action: LibAction,
    pub title: String,
    pub subtitle: String,
    pub icon: &'static str,
    pub keywords: String,
    /// About the selected item: listed first while the library is open.
    pub contextual: bool,
}

/// The selection moved from one item to another, for back navigation.
pub enum LibraryEvent {
    Moved { from: u64 },
}

enum NoticeAction {
    Reveal(PathBuf),
    Replace { id: u64, target: Target, project: Option<PathBuf> },
    ConfirmDelete(u64),
}

struct Notice {
    text: String,
    tone: Tone,
    action: Option<NoticeAction>,
}

/// Rows of the list in display order: Pinned first, then one group per kind.
struct Group {
    key: &'static str,
    label: &'static str,
    items: Vec<u64>,
}

pub struct LibraryView {
    lib: Library,
    search: Entity<InputState>,
    editor: Entity<EditorState>,
    selected: Option<u64>,
    collapsed: HashSet<&'static str>,
    editing: bool,
    /// Placeholder inputs for the selected prompt, in order of use.
    vars: Vec<(String, Entity<InputState>)>,
    notice: Option<Notice>,
    /// Bumped per message, so an older message's timer leaves a newer one alone.
    notice_gen: usize,
    dirty: bool,
    save_task: Option<Task<()>>,
    importing: bool,
    _subs: Vec<Subscription>,
    var_subs: Vec<Subscription>,
}

/// The file name a tool would know this item by.
fn file_label(item: &Item) -> String {
    match item.kind {
        Kind::Skill => "SKILL.md".into(),
        Kind::Agent => "agent.md".into(),
        Kind::Prompt => "prompt.md".into(),
        Kind::Rule => item
            .meta
            .source
            .as_deref()
            .and_then(|s| s.rsplit(['/', '\\']).next())
            .filter(|n| library::recognize(std::path::Path::new(n)).is_some())
            .unwrap_or("rules.md")
            .to_string(),
    }
}

fn kind_word(w: &str) -> Option<Kind> {
    match w {
        "skill" | "skills" => Some(Kind::Skill),
        "prompt" | "prompts" => Some(Kind::Prompt),
        "agent" | "agents" => Some(Kind::Agent),
        "rule" | "rules" => Some(Kind::Rule),
        _ => None,
    }
}

impl LibraryView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let root = Library::default_root().unwrap_or_else(|| std::env::temp_dir().join("SideKit").join("library"));
        let lib = Library::open(root);
        let search = line("", "Search, #tag, or a kind like \"skill\"", window, cx);
        let editor = code_editor("", "Write Markdown. Frontmatter sets the name, description and tags.", "markdown", window, cx);
        editor.update(cx, |s, cx| s.set_soft_wrap(true, window, cx));
        let subs = vec![
            watch(&search, window, cx, |_, _, cx| cx.notify()),
            watch(&editor, window, cx, Self::edited),
        ];
        let mut this = Self {
            lib,
            search,
            editor,
            selected: None,
            collapsed: HashSet::new(),
            editing: false,
            vars: Vec::new(),
            notice: None,
            notice_gen: 0,
            dirty: false,
            save_task: None,
            importing: false,
            _subs: subs,
            var_subs: Vec::new(),
        };
        if let Some(first) = this.ordered(cx).first().copied() {
            this.show(first, window, cx);
        }
        this
    }

    pub fn count(&self) -> usize {
        self.lib.items.len()
    }

    pub fn palette_items(&self) -> Vec<PaletteItem> {
        self.lib
            .items
            .iter()
            .map(|i| PaletteItem { id: i.id, title: i.title.clone(), summary: i.summary.clone(), kind: i.kind, tags: i.tags.clone() })
            .collect()
    }

    pub fn selected(&self) -> Option<u64> {
        self.selected
    }

    /// Go back to an item without recording history. False if it is gone.
    pub fn restore(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.lib.get(id).is_none() {
            return false;
        }
        self.reveal_in_list(id, window, cx);
        self.show(id, window, cx);
        true
    }

    /// Show one item, clearing a search that would hide it.
    pub fn open_item(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.reveal_in_list(id, window, cx);
        self.select(id, window, cx);
    }

    fn reveal_in_list(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ordered(cx).contains(&id) {
            set_line(&self.search, "", window, cx);
        }
        if let Some(item) = self.lib.get(id) {
            let key = if item.meta.favorite { "pinned" } else { item.kind.dir() };
            self.collapsed.remove(key);
        }
    }

    fn selected_item(&self) -> Option<&Item> {
        self.selected.and_then(|id| self.lib.get(id))
    }

    /// Groups of items passing the search. The query understands `#tag` and
    /// kind words ("skill", "prompts"); anything else matches the text.
    fn groups(&self, cx: &App) -> Vec<Group> {
        let q = line_text(&self.search, cx).to_lowercase();
        let mut tags = Vec::new();
        let mut kinds = Vec::new();
        let mut words = Vec::new();
        for w in q.split_whitespace() {
            if let Some(t) = w.strip_prefix('#').filter(|t| !t.is_empty()) {
                tags.push(t.to_string());
            } else if let Some(k) = kind_word(w) {
                kinds.push(k);
            } else {
                words.push(w);
            }
        }
        let text = words.join(" ");
        let mut items: Vec<&Item> = self
            .lib
            .items
            .iter()
            .filter(|i| kinds.is_empty() || kinds.contains(&i.kind))
            .filter(|i| tags.iter().all(|t| i.tags.iter().any(|x| x.starts_with(t.as_str()))))
            .filter(|i| {
                text.is_empty()
                    || i.title.to_lowercase().contains(&text)
                    || i.summary.to_lowercase().contains(&text)
                    || crate::palette::fuzzy(&text, &i.title).is_some_and(|s| s >= text.len() as i32 * 4)
                    || (text.len() >= 3 && i.text.to_lowercase().contains(&text))
            })
            .collect();
        items.sort_by(|a, b| b.meta.updated.cmp(&a.meta.updated));
        let mut groups = vec![Group { key: "pinned", label: "Pinned", items: items.iter().filter(|i| i.meta.favorite).map(|i| i.id).collect() }];
        groups.extend(KINDS.iter().map(|k| Group {
            key: k.dir(),
            label: k.plural(),
            items: items.iter().filter(|i| i.kind == *k && !i.meta.favorite).map(|i| i.id).collect(),
        }));
        groups.retain(|g| !g.items.is_empty());
        groups
    }

    /// Item ids in the order the list shows them, skipping collapsed groups.
    fn ordered(&self, cx: &App) -> Vec<u64> {
        self.groups(cx).into_iter().filter(|g| !self.collapsed.contains(g.key)).flat_map(|g| g.items).collect()
    }

    // ------------------------------------------------------------ selection and editing

    fn select(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(from) = self.selected.filter(|f| *f != id && self.lib.get(*f).is_some()) {
            cx.emit(LibraryEvent::Moved { from });
        }
        self.show(id, window, cx);
    }

    fn show(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.save_now(window, cx);
        if self.selected != Some(id) {
            self.editing = false;
        }
        self.selected = Some(id);
        if !matches!(self.notice.as_ref().and_then(|n| n.action.as_ref()), Some(NoticeAction::Reveal(_)) | None) {
            self.notice = None;
        }
        let text = self.lib.get(id).map(|i| i.text.clone()).unwrap_or_default();
        set_text(&self.editor, &text, window, cx);
        self.rebuild_vars(window, cx);
        cx.notify();
    }

    fn rebuild_vars(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let names = self.selected_item().map(|i| i.vars.clone()).unwrap_or_default();
        if names.iter().eq(self.vars.iter().map(|(n, _)| n)) {
            return;
        }
        let old: HashMap<String, Entity<InputState>> = self.vars.drain(..).collect();
        self.var_subs.clear();
        for name in names {
            let input = match old.get(&name) {
                Some(i) => i.clone(),
                None => line("", if name.starts_with('$') { "Arguments" } else { "" }, window, cx),
            };
            self.var_subs.push(watch(&input, window, cx, |_, _, cx| cx.notify()));
            self.vars.push((name, input));
        }
    }

    fn edited(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.is_none() {
            return;
        }
        self.dirty = true;
        self.save_task = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(SAVE_DELAY).await;
            let _ = this.update_in(cx, |v, window, cx| v.save_now(window, cx));
        }));
        cx.notify();
    }

    fn save_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.save_task = None;
        if !std::mem::take(&mut self.dirty) {
            return;
        }
        let Some(id) = self.selected else { return };
        let text = text_of(&self.editor, cx);
        if let Err(e) = self.lib.set_text(id, text) {
            self.say(format!("Could not save: {e}"), Tone::Err, None, cx);
        }
        self.rebuild_vars(window, cx);
        cx.notify();
    }

    /// Show a message. Plain messages fade after a few seconds; questions stay.
    fn say(&mut self, text: impl Into<String>, tone: Tone, action: Option<NoticeAction>, cx: &mut Context<Self>) {
        let stays = matches!(action, Some(NoticeAction::Replace { .. } | NoticeAction::ConfirmDelete(_)));
        self.notice_gen += 1;
        self.notice = Some(Notice { text: text.into(), tone, action });
        if !stays {
            let generation = self.notice_gen;
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(TOAST_FOR).await;
                let _ = this.update(cx, |v, cx| {
                    if v.notice_gen == generation {
                        v.notice = None;
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    // ------------------------------------------------------------ actions

    fn create(&mut self, kind: Kind, window: &mut Window, cx: &mut Context<Self>) {
        self.save_now(window, cx);
        match self.lib.create(kind) {
            Ok(id) => {
                self.open_item(id, window, cx);
                self.editing = true;
                self.editor.update(cx, |s, cx| s.focus(window, cx));
            }
            Err(e) => self.say(format!("Could not create the file: {e}"), Tone::Err, None, cx),
        }
        cx.notify();
    }

    fn import(&mut self, paths: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        if paths.is_empty() || self.importing {
            return;
        }
        self.save_now(window, cx);
        self.importing = true;
        self.say("Importing…", Tone::Info, None, cx);
        let gather = cx.background_spawn(async move { library::gather(&paths) });
        cx.spawn_in(window, async move |this, cx| {
            let found = gather.await;
            let _ = this.update_in(cx, |v, window, cx| {
                v.importing = false;
                let report: ImportReport = v.lib.add_found(found);
                if let Some(id) = report.first {
                    v.open_item(id, window, cx);
                }
                let tone = if report.flagged > 0 { Tone::Err } else if report.added > 0 { Tone::Ok } else { Tone::Neutral };
                v.say(report.summary(), tone, None, cx);
            });
        })
        .detach();
    }

    fn prompt_import(&mut self, folders: bool, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: !folders,
            directories: folders,
            multiple: true,
            prompt: Some("Import".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                let _ = this.update_in(cx, |v, window, cx| v.import(paths, window, cx));
            }
        })
        .detach();
    }

    fn scan_known(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let places = library::known_locations();
        if places.is_empty() {
            self.say("No Claude Code, Codex, Cursor or Copilot folders were found in your home folder", Tone::Neutral, None, cx);
            return;
        }
        self.import(places, window, cx);
    }

    fn copy(&mut self, with_front: bool, cx: &mut Context<Self>) {
        let Some(item) = self.selected_item() else { return };
        let values: HashMap<String, String> = self.vars.iter().map(|(n, i)| (n.clone(), line_text(i, cx))).collect();
        let text = if with_front { item.text.clone() } else { library::fill_variables(&item.text, &values) };
        let unfilled = !with_front && self.vars.iter().any(|(_, i)| line_text(i, cx).is_empty());
        let id = item.id;
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.lib.touch(id);
        let msg = if unfilled { "Copied · some placeholders are still empty" } else { "Copied to the clipboard" };
        self.say(msg, if unfilled { Tone::Neutral } else { Tone::Ok }, None, cx);
    }

    fn install(&mut self, target: Target, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected else { return };
        self.save_now(window, cx);
        if !target.needs_project() {
            self.do_install(id, target, None, false, cx);
            return;
        }
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose project".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(mut paths))) = rx.await {
                if let Some(project) = paths.pop() {
                    let _ = this.update(cx, |v, cx| v.do_install(id, target, Some(project), false, cx));
                }
            }
        })
        .detach();
    }

    fn do_install(&mut self, id: u64, target: Target, project: Option<PathBuf>, overwrite: bool, cx: &mut Context<Self>) {
        match self.lib.install(id, target, project.as_deref(), overwrite) {
            Ok(path) => self.say(format!("Installed to {}", path.display()), Tone::Ok, Some(NoticeAction::Reveal(path)), cx),
            Err(InstallError::Exists(path)) => self.say(
                format!("{} already exists and is different", path.display()),
                Tone::Err,
                Some(NoticeAction::Replace { id, target, project }),
                cx,
            ),
            Err(InstallError::Other(e)) => self.say(format!("Could not install: {e}"), Tone::Err, None, cx),
        }
    }

    fn save_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.selected_item() else { return };
        let dir = library::home_dir().unwrap_or_default();
        let name = format!("{}.md", item.slug());
        let (id, text) = (item.id, item.text.clone());
        let rx = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(path))) = rx.await {
                let result = std::fs::write(&path, text);
                let _ = this.update(cx, |v, cx| match result {
                    Ok(()) => {
                        v.lib.touch(id);
                        v.say(format!("Saved to {}", path.display()), Tone::Ok, Some(NoticeAction::Reveal(path)), cx);
                    }
                    Err(e) => v.say(format!("Could not save: {e}"), Tone::Err, None, cx),
                });
            }
        })
        .detach();
    }

    fn move_to(&mut self, kind: Kind, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected else { return };
        self.save_now(window, cx);
        match self.lib.set_kind(id, kind) {
            Ok(new_id) => {
                self.open_item(new_id, window, cx);
                self.say(format!("Moved to {}", kind.plural()), Tone::Ok, None, cx);
            }
            Err(e) => self.say(format!("Could not move the file: {e}"), Tone::Err, None, cx),
        }
    }

    fn delete(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.save_task = None;
        self.dirty = false;
        let title = self.lib.get(id).map(|i| i.title.clone()).unwrap_or_default();
        // The neighbour below (or above) takes its place, as in a file list.
        let order = self.ordered(cx);
        let next = order.iter().position(|i| *i == id).and_then(|p| order.get(p + 1).or(p.checked_sub(1).and_then(|q| order.get(q)))).copied();
        match self.lib.delete(id) {
            Ok(()) => {
                self.selected = None;
                match next {
                    Some(n) => self.show(n, window, cx),
                    None => {
                        set_text(&self.editor, "", window, cx);
                        self.vars.clear();
                    }
                }
                self.say(format!("Deleted \"{title}\""), Tone::Neutral, None, cx);
            }
            Err(e) => self.say(format!("Could not delete: {e}"), Tone::Err, None, cx),
        }
    }

    pub fn palette_commands(&self, in_library: bool) -> Vec<PaletteCommand> {
        let mut out = Vec::new();
        let mut add = |action, title: String, subtitle: &str, icon, keywords: &str, contextual| {
            out.push(PaletteCommand { action, title, subtitle: subtitle.into(), icon, keywords: keywords.into(), contextual })
        };
        for k in KINDS {
            add(LibAction::New(k), format!("New {}", k.label().to_lowercase()), k.blurb(), k.icon(), "create add library ai", false);
        }
        add(LibAction::Scan, "Scan Claude, Codex, Cursor & Copilot folders".into(), "Import skills, prompts, agents and rules from your home folder", "search", "import library find ai claude codex cursor copilot github", false);
        add(LibAction::ImportFolder, "Import folder into library…".into(), "Find skills, prompts, agents and rules in a folder", "folder", "import library scan ai", false);
        add(LibAction::ImportFiles, "Import files into library…".into(), "Add Markdown files to the AI library", "lines", "import library ai", false);
        add(LibAction::OpenFolder, "Open library folder".into(), "Where the library's Markdown files live", "folder", "library files explorer finder", false);
        let Some(item) = self.selected_item().filter(|_| in_library) else { return out };
        let t = &item.title;
        add(LibAction::Copy, format!("Copy \"{t}\""), "Put it on the clipboard, with placeholders filled in", "copy", "copy clipboard", true);
        add(LibAction::CopyRaw, format!("Copy \"{t}\" with frontmatter"), "The whole file, as stored", "copy", "copy raw file frontmatter", true);
        let pin = if item.meta.favorite { "Unpin" } else { "Pin" };
        add(LibAction::Pin, format!("{pin} \"{t}\""), "Pinned items stay at the top of the list", "star", "pin favorite star", true);
        for target in Target::for_kind(item.kind) {
            add(LibAction::Install(target), format!("Install to {}", target.label(item.kind)), &format!("Install \"{t}\""), "install", "install export claude codex cursor agents github copilot", true);
        }
        add(LibAction::SaveAs, "Save as file…".into(), &format!("Export \"{t}\" as Markdown"), "folder", "export save file", true);
        if self.editing {
            add(LibAction::Done, "Done editing".into(), "Back to the rendered document", "check", "done preview finish", true);
        } else {
            add(LibAction::Edit, format!("Edit \"{t}\""), "Edit the Markdown and frontmatter", "edit", "edit markdown", true);
        }
        let reveal = if cfg!(target_os = "macos") { "Show in Finder" } else { "Show in folder" };
        add(LibAction::Reveal, reveal.into(), &format!("Reveal \"{t}\"'s file"), "folder", "reveal explorer finder file", true);
        add(LibAction::OpenExternal, "Open in default editor".into(), &format!("Open \"{t}\" in another app"), "edit", "open external editor", true);
        for k in KINDS.into_iter().filter(|k| *k != item.kind) {
            add(LibAction::MoveTo(k), format!("Move to {}", k.plural()), &format!("Change \"{t}\" to a {}", k.label().to_lowercase()), k.icon(), "move kind type change", true);
        }
        add(LibAction::Delete, format!("Delete \"{t}\""), "Remove it from the library folder", "trash", "delete remove", true);
        out
    }

    pub fn run(&mut self, action: LibAction, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            LibAction::New(k) => self.create(k, window, cx),
            LibAction::ImportFolder => self.prompt_import(true, window, cx),
            LibAction::ImportFiles => self.prompt_import(false, window, cx),
            LibAction::Scan => self.scan_known(window, cx),
            LibAction::OpenFolder => {
                let _ = std::fs::create_dir_all(self.lib.root());
                cx.open_with_system(self.lib.root());
            }
            LibAction::Copy => self.copy(false, cx),
            LibAction::CopyRaw => self.copy(true, cx),
            LibAction::Pin => {
                if let Some(id) = self.selected {
                    self.lib.toggle_favorite(id);
                    self.reveal_in_list(id, window, cx);
                }
            }
            LibAction::Install(target) => self.install(target, window, cx),
            LibAction::SaveAs => self.save_as(window, cx),
            LibAction::Reveal | LibAction::OpenExternal => {
                if let Some(item) = self.selected_item() {
                    let path = self.lib.path_of(item);
                    if action == LibAction::Reveal { cx.reveal_path(&path) } else { cx.open_with_system(&path) }
                }
            }
            LibAction::Edit | LibAction::Done => {
                self.save_now(window, cx);
                self.editing = action == LibAction::Edit;
                if self.editing {
                    self.editor.update(cx, |s, cx| s.focus(window, cx));
                }
            }
            LibAction::MoveTo(k) => self.move_to(k, window, cx),
            LibAction::Delete => {
                if let Some(item) = self.selected_item() {
                    let (id, title) = (item.id, item.title.clone());
                    self.say(format!("Delete \"{title}\"?"), Tone::Err, Some(NoticeAction::ConfirmDelete(id)), cx);
                }
            }
        }
        cx.notify();
    }

    fn run_notice_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(action) = self.notice.take().and_then(|n| n.action) else { return };
        match action {
            NoticeAction::Reveal(path) => cx.reveal_path(&path),
            NoticeAction::Replace { id, target, project } => self.do_install(id, target, project, true, cx),
            NoticeAction::ConfirmDelete(id) => self.delete(id, window, cx),
        }
        cx.notify();
    }
}

// ---------------------------------------------------------------- rendering

fn ago(ts: i64) -> String {
    let d = chrono::Utc::now().timestamp() - ts;
    match d {
        _ if ts == 0 => "never".into(),
        ..60 => "just now".into(),
        60..3600 => format!("{} min ago", d / 60),
        3600..86_400 => format!("{} h ago", d / 3600),
        86_400..604_800 => crate::logic::plural((d / 86_400) as usize, "day") + " ago",
        _ => chrono::DateTime::from_timestamp(ts, 0).map(|t| t.format("%-d %b %Y").to_string()).unwrap_or_default(),
    }
}

/// `~` for the home folder, so source paths stay short.
fn tilde(path: &str) -> String {
    match library::home_dir().map(|h| h.to_string_lossy().into_owned()) {
        Some(h) if !h.is_empty() && path.starts_with(&h) => format!("~{}", &path[h.len()..]),
        _ => path.to_string(),
    }
}

impl LibraryView {
    fn render_list(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let p = Pal::get(cx);
        let selected = self.selected;
        let groups = self.groups(cx);
        let searching = !line_text(&self.search, cx).trim().is_empty();
        let mut list = div().id("lib-list").flex_1().min_h_0().overflow_y_scrollbar().flex().flex_col().pr(px(4.));

        for g in groups.iter() {
            let key = g.key;
            let open = !self.collapsed.contains(key);
            list = list.child(
                div()
                    .id(id!("lib-group-{key}"))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .h(px(28.))
                    .mt(px(10.))
                    .px(px(8.))
                    .rounded(px(5.))
                    .cursor_pointer()
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.text3)
                    .hover(move |s| s.text_color(p.text2))
                    .child(icon(if open { "chev-down" } else { "chev-right" }, 11., p.text3))
                    .child(div().flex_1().child(g.label))
                    .child(div().font_weight(FontWeight::NORMAL).child(g.items.len().to_string()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.collapsed.remove(key) {
                            this.collapsed.insert(key);
                        }
                        cx.notify();
                    })),
            );
            if !open {
                continue;
            }
            for &iid in &g.items {
                let Some(item) = self.lib.get(iid) else { continue };
                let sel = selected == Some(iid);
                let flagged = !item.findings.is_empty();
                list = list.child(
                    div()
                        .id(id!("lib-row-{iid}"))
                        .relative()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .py(px(7.))
                        .pl(px(24.))
                        .pr(px(10.))
                        .rounded(px(6.))
                        .cursor_pointer()
                        .when(sel, |d| {
                            d.bg(p.subtle2).child(div().absolute().left(px(8.)).top(px(10.)).bottom(px(10.)).w(px(3.)).rounded(px(3.)).bg(p.accent))
                        })
                        .when(!sel, |d| d.hover(move |s| s.bg(p.subtle)))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_size(px(13.5))
                                        .when(sel, |d| d.font_weight(FontWeight::SEMIBOLD))
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .child(item.title.clone()),
                                )
                                .when(flagged, |d| d.child(div().size(px(6.)).flex_none().rounded_full().bg(p.danger))),
                        )
                        .when(!item.summary.is_empty(), |d| {
                            d.child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(p.text3)
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(item.summary.clone()),
                            )
                        })
                        .on_click(cx.listener(move |this, _, window, cx| this.select(iid, window, cx))),
                );
            }
        }
        if groups.is_empty() {
            list = list.child(
                div()
                    .px(px(8.))
                    .py(px(16.))
                    .text_size(px(13.))
                    .text_color(p.text3)
                    .child(if searching { "Nothing matches." } else { "No items yet." }),
            );
        }

        div()
            .w(relative(0.34))
            .min_w(px(240.))
            .max_w(px(320.))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(4.))
            .min_h_0()
            .child(field_el(&self.search, false, 34., 13., window, cx).child(icon("search", 14., p.text3)))
            .child(list)
            .into_any_element()
    }

    /// A message pinned to the bottom of the document, over the content.
    fn render_toast(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = Pal::get(cx);
        let n = self.notice.as_ref()?;
        let action_label = match &n.action {
            Some(NoticeAction::Reveal(_)) => Some(if cfg!(target_os = "macos") { "Show in Finder" } else { "Show in folder" }),
            Some(NoticeAction::Replace { .. }) => Some("Replace"),
            Some(NoticeAction::ConfirmDelete(_)) => Some("Delete"),
            None => None,
        };
        let asks = matches!(n.action, Some(NoticeAction::Replace { .. } | NoticeAction::ConfirmDelete(_)));
        Some(
            div()
                .absolute()
                .bottom(px(16.))
                .left(px(16.))
                .right(px(16.))
                .flex()
                .justify_center()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .max_w(px(640.))
                        .min_h(px(40.))
                        .pl(px(14.))
                        .pr(px(6.))
                        .py(px(4.))
                        .rounded(px(8.))
                        .bg(p.card)
                        .border_1()
                        .border_color(p.stroke_strong)
                        .shadow(p.shadow())
                        .text_size(px(12.5))
                        .child(ui::dot(Some(n.tone), &p))
                        .child(div().flex_1().min_w_0().line_height(relative(1.4)).child(n.text.clone()))
                        .when_some(action_label, |d, label| {
                            let kind = if asks { BtnKind::Accent } else { BtnKind::Normal };
                            d.child(ui::btn("lib-toast-act", None, label, kind, &p, cx.listener(|this, _, w, cx| this.run_notice_action(w, cx))).h(px(28.)))
                        })
                        .child(ui::icon_btn("lib-toast-x", "x", &p, cx.listener(|this, _, _, cx| {
                            this.notice = None;
                            cx.notify();
                        }))),
                )
                .into_any_element(),
        )
    }

    fn render_detail(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let p = Pal::get(cx);
        let Some(item) = self.selected_item().cloned() else {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(p.text3)
                .text_size(px(13.))
                .child("Pick something from the list.")
                .into_any_element();
        };
        let id = item.id;
        let kind = item.kind;
        let targets = Target::for_kind(kind);
        // Prompts are pasted into a chat; everything else goes into a tool's folder.
        let copy_first = kind == Kind::Prompt;

        let primary = if copy_first {
            ui::btn("lib-copy", Some("copy"), "Copy", BtnKind::Accent, &p, cx.listener(|this, _, _, cx| this.copy(false, cx))).into_any_element()
        } else {
            let entries: Vec<MenuEntry> = targets.iter().map(|t| MenuEntry::new(t.label(kind), None)).collect();
            let t2 = targets.clone();
            let on_install = cx.listener(move |this: &mut Self, i: &usize, window, cx| this.install(t2[*i], window, cx));
            let on_install = move |i: usize, w: &mut Window, cx: &mut App| on_install(&i, w, cx);
            ui::menu_btn("lib-install", Some("install"), Some("Install".into()), BtnKind::Accent, entries, &p, window, cx, on_install).into_any_element()
        };

        // Everything that is not the primary action lives here.
        let mut more: Vec<(MenuEntry, LibAction)> = Vec::new();
        if copy_first {
            more.extend(targets.iter().map(|t| (MenuEntry::new(format!("Install to {}", t.label(kind)), Some("install")), LibAction::Install(*t))));
        } else {
            more.push((MenuEntry::new("Copy", Some("copy")), LibAction::Copy));
        }
        more.push((MenuEntry::new("Copy with frontmatter", Some("copy")), LibAction::CopyRaw));
        more.push((MenuEntry::new(if item.meta.favorite { "Unpin" } else { "Pin to top" }, Some("star")), LibAction::Pin));
        more.push((MenuEntry::new("Save as file…", Some("folder")), LibAction::SaveAs));
        more.push((MenuEntry::new(if cfg!(target_os = "macos") { "Show in Finder" } else { "Show in folder" }, Some("folder")), LibAction::Reveal));
        more.push((MenuEntry::new("Open in default editor", Some("edit")), LibAction::OpenExternal));
        more.extend(KINDS.into_iter().filter(|k| *k != kind).map(|k| (MenuEntry::new(format!("Move to {}", k.plural()), Some(k.icon())), LibAction::MoveTo(k))));
        more.push((MenuEntry::new("Delete", Some("trash")).danger(), LibAction::Delete));
        let (entries, actions): (Vec<MenuEntry>, Vec<LibAction>) = more.into_iter().unzip();
        let on_more = cx.listener(move |this: &mut Self, i: &usize, window, cx| this.run(actions[*i], window, cx));
        let on_more = move |i: usize, w: &mut Window, cx: &mut App| on_more(&i, w, cx);

        let edit_toggle = if self.editing {
            ui::btn("lib-done", None, "Done", BtnKind::Normal, &p, cx.listener(|this, _, w, cx| this.run(LibAction::Done, w, cx)))
        } else {
            ui::btn("lib-edit", Some("edit"), "Edit", BtnKind::Normal, &p, cx.listener(|this, _, w, cx| this.run(LibAction::Edit, w, cx)))
        };

        let header = div()
            .flex()
            .flex_wrap()
            .items_start()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .flex_1()
                    .min_w(px(220.))
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap(px(10.))
                            .child(div().text_size(px(20.)).font_weight(FontWeight::SEMIBOLD).child(item.title.clone()))
                            .child(div().flex_none().font_family(MONO_FONT).text_size(px(12.)).text_color(p.text3).child(file_label(&item))),
                    )
                    .when(!item.summary.is_empty(), |d| {
                        d.child(div().text_size(px(13.)).text_color(p.text2).line_height(relative(1.45)).child(item.summary.clone()))
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .flex_none()
                    .ml_auto()
                    .when(self.editing, |d| {
                        d.child(div().text_size(px(12.)).text_color(p.text3).mr(px(4.)).child(if self.dirty { "Saving…" } else { "Saved" }))
                    })
                    .child(edit_toggle)
                    .child(primary)
                    .child(ui::menu_btn("lib-more", None, None, BtnKind::Normal, entries, &p, window, cx, on_more)),
            );

        // One quiet line of facts; tags search the library when clicked.
        let mut facts: Vec<String> = vec![kind.label().to_string(), format!("edited {}", ago(item.meta.updated))];
        if item.meta.used > 0 {
            facts.push(format!("used {}", ago(item.meta.used)));
        }
        if let Some(src) = &item.meta.source {
            facts.push(format!("from {}", tilde(src)));
        }
        let meta = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x(px(10.))
            .gap_y(px(2.))
            .text_size(px(12.))
            .text_color(p.text3)
            .child(div().min_w_0().whitespace_nowrap().overflow_hidden().text_ellipsis().child(facts.join(" · ")))
            .children(item.tags.iter().map(|t| {
                let tag = t.clone();
                div()
                    .id(id!("lib-tag-{t}"))
                    .cursor_pointer()
                    .hover(move |s| s.text_color(p.accent))
                    .child(format!("#{t}"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        set_line(&this.search, &format!("#{tag}"), window, cx);
                        cx.notify();
                    }))
            }));

        let findings = (!item.findings.is_empty()).then(|| {
            let lines = item.findings.iter().map(|f| format!("{} on line {}", f.what.to_lowercase(), f.line)).collect::<Vec<_>>().join(", ");
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(12.5))
                .text_color(p.danger)
                .child(icon("warn", 14., p.danger))
                .child(div().flex_1().min_w_0().child(format!("Possible credential: {lines}")))
        });

        let vars = (!self.vars.is_empty()).then(|| {
            div().flex().flex_col().gap(px(6.)).max_w(px(560.)).children(self.vars.iter().enumerate().map(|(i, (name, input))| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .w(px(120.))
                            .flex_none()
                            .font_family(MONO_FONT)
                            .text_size(px(12.))
                            .text_color(p.text2)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(name.trim_start_matches('$').to_string()),
                    )
                    .child(field_el(input, false, 30., 13., window, cx).flex_1().id(id!("lib-var-{i}")))
            }))
        });

        let body: AnyElement = if self.editing {
            ui::pane(is_focused(&self.editor, window, cx), &p)
                .flex_1()
                .min_h_0()
                .child(code_editor_el(&self.editor, false, cx))
                .on_key_down(cx.listener(|this, ev: &KeyDownEvent, w, cx| {
                    if ev.keystroke.key == "escape" {
                        this.run(LibAction::Done, w, cx);
                        cx.stop_propagation();
                    }
                }))
                .into_any_element()
        } else {
            div()
                .id(id!("lib-doc-{id}"))
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .pt(px(4.))
                .pb(px(64.))
                .child(
                    div()
                        .max_w(px(760.))
                        .child(TextView::markdown(id!("lib-md-{id}-{}", item.meta.updated), item.body().to_string()).selectable(true)),
                )
                .into_any_element()
        };

        div()
            .relative()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .pb(px(16.))
                    .child(header)
                    .child(meta)
                    .when_some(findings, |d, f| d.child(f))
                    .when_some(vars, |d, v| d.child(div().pt(px(6.)).child(v))),
            )
            .child(div().h(px(1.)).flex_none().bg(p.stroke).mb(px(16.)))
            .child(body)
            .into_any_element()
    }

    fn render_empty(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let p = Pal::get(cx);
        let starters = KINDS.iter().map(|k| {
            let k = *k;
            div()
                .id(id!("lib-empty-{}", k.dir()))
                .cursor_pointer()
                .text_color(p.accent)
                .hover(|s| s.underline())
                .child(k.label().to_lowercase())
                .on_click(cx.listener(move |this, _, w, cx| this.create(k, w, cx)))
        });
        let mut new_line = div().flex().flex_wrap().justify_center().gap(px(6.)).text_size(px(13.)).text_color(p.text3).child("or start a new");
        for (i, s) in starters.enumerate() {
            if i > 0 {
                new_line = new_line.child("·");
            }
            new_line = new_line.child(s);
        }
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w_full()
                    .max_w(px(560.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(12.))
                    .py(px(40.))
                    .px(px(28.))
                    .rounded(px(10.))
                    .border_1()
                    .border_dashed()
                    .border_color(p.stroke_strong)
                    .child(icon("library", 28., p.text3))
                    .child(div().text_size(px(16.)).font_weight(FontWeight::SEMIBOLD).child("Nothing here yet"))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(p.text2)
                            .text_center()
                            .line_height(relative(1.5))
                            .child("Drop SKILL.md, AGENTS.md, .cursorrules or whole project folders here, or scan the folders your AI tools already use."),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .mt(px(4.))
                            .child(ui::btn("lib-e-scan", Some("search"), "Scan my AI tool folders", BtnKind::Accent, &p, cx.listener(|this, _, w, cx| this.scan_known(w, cx))))
                            .child(ui::btn("lib-e-folder", Some("folder"), "Import folder…", BtnKind::Normal, &p, cx.listener(|this, _, w, cx| this.prompt_import(true, w, cx)))),
                    )
                    .child(new_line),
            )
            .into_any_element()
    }
}

impl EventEmitter<LibraryEvent> for LibraryView {}

impl Render for LibraryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Pal::get(cx);
        let empty = self.lib.items.is_empty();

        let mut add: Vec<(MenuEntry, LibAction)> =
            KINDS.iter().map(|k| (MenuEntry::new(format!("New {}", k.label().to_lowercase()), Some(k.icon())), LibAction::New(*k))).collect();
        add.push((MenuEntry::new("Import folder…", Some("folder")), LibAction::ImportFolder));
        add.push((MenuEntry::new("Import files…", Some("lines")), LibAction::ImportFiles));
        add.push((MenuEntry::new("Scan Claude, Codex, Cursor & Copilot folders", Some("search")), LibAction::Scan));
        add.push((MenuEntry::new("Open library folder", Some("folder")), LibAction::OpenFolder));
        let (entries, actions): (Vec<MenuEntry>, Vec<LibAction>) = add.into_iter().unzip();
        let on_add = cx.listener(move |this: &mut Self, i: &usize, w, cx| this.run(actions[*i], w, cx));
        let on_add = move |i: usize, w: &mut Window, cx: &mut App| on_add(&i, w, cx);

        let header = div()
            .flex()
            .items_center()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .flex_1()
                    .min_w_0()
                    .child(div().text_size(px(26.)).font_weight(FontWeight::SEMIBOLD).child("AI Library"))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(p.text2)
                            .child(SharedString::from("Skills, prompts, agents and rules for your AI tools, kept as Markdown on this computer.")),
                    ),
            )
            .when(!empty, |d| {
                d.child(ui::menu_btn("lib-add", Some("plus"), Some("New".into()), BtnKind::Normal, entries, &p, window, cx, on_add))
            });

        let body: AnyElement = if empty {
            self.render_empty(cx)
        } else {
            div()
                .flex()
                .gap(px(28.))
                .flex_1()
                .min_h_0()
                .child(self.render_list(window, cx))
                .child(div().w(px(1.)).flex_none().bg(p.stroke))
                .child(self.render_detail(window, cx))
                .into_any_element()
        };

        div()
            .id("library")
            .relative()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .gap(px(20.))
            .pt(px(26.))
            .px(px(36.))
            .pb(px(20.))
            .drag_over::<ExternalPaths>(move |s, _, _, _| s.bg(p.accent_soft))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                this.import(paths.paths().to_vec(), window, cx);
            }))
            .child(header)
            .child(body)
            .when_some(self.render_toast(cx), |d, t| d.child(t))
    }
}
