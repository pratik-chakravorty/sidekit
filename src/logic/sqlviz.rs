//! SQL / CQL schemas → entity-relationship diagrams.
//!
//! The parser is deliberately forgiving: it understands the common shapes of
//! MySQL, PostgreSQL, SQLite and Cassandra (CQL) DDL, ignores queries, skips
//! anything else it does not recognise, and never fails outright.

// ---------------------------------------------------------------- tokens

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum K {
    Word,
    Quoted,
    Str,
    Num,
    Punct,
}

#[derive(Clone, Debug)]
struct Tok {
    k: K,
    t: String,
}

impl Tok {
    fn is(&self, kw: &str) -> bool {
        self.k == K::Word && self.t.eq_ignore_ascii_case(kw)
    }
    fn p(&self, c: &str) -> bool {
        self.k == K::Punct && self.t == c
    }
    fn ident(&self) -> bool {
        matches!(self.k, K::Word | K::Quoted)
    }
}

fn tokenize(src: &str) -> Vec<Tok> {
    let s: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        let next = s.get(i + 1).copied();
        if c.is_whitespace() {
            i += 1;
        } else if (c == '-' && next == Some('-')) || (c == '/' && next == Some('/')) || c == '#' {
            while i < s.len() && s[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            i += 2;
            while i < s.len() && !(s[i] == '*' && s.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i += 2;
        } else if c == '\'' || c == '"' || c == '`' || c == '[' {
            let close = if c == '[' { ']' } else { c };
            // `[` is only a quote when it opens an identifier (SQL Server);
            // after a word or `]` it is an array suffix such as `int[]`.
            if c == '[' && (next == Some(']') || next.is_some_and(|n| n.is_ascii_digit())) {
                out.push(Tok { k: K::Punct, t: c.to_string() });
                i += 1;
                continue;
            }
            let mut j = i + 1;
            let mut text = String::new();
            while j < s.len() {
                if s[j] == close {
                    if close != ']' && s.get(j + 1) == Some(&close) {
                        text.push(close);
                        j += 2;
                        continue;
                    }
                    break;
                }
                if s[j] == '\\' && c == '\'' && j + 1 < s.len() {
                    text.push(s[j]);
                    text.push(s[j + 1]);
                    j += 2;
                    continue;
                }
                text.push(s[j]);
                j += 1;
            }
            let k = if c == '\'' { K::Str } else { K::Quoted };
            out.push(Tok { k, t: text });
            i = j + 1;
        } else if c == '$' && next == Some('$') {
            // PostgreSQL dollar quoting: $$ ... $$
            let mut j = i + 2;
            while j + 1 < s.len() && !(s[j] == '$' && s[j + 1] == '$') {
                j += 1;
            }
            out.push(Tok { k: K::Str, t: s[i + 2..j.min(s.len())].iter().collect() });
            i = j + 2;
        } else if c.is_ascii_digit() || (c == '.' && next.is_some_and(|n| n.is_ascii_digit())) {
            let mut j = i;
            while j < s.len() && (s[j].is_ascii_alphanumeric() || s[j] == '.' || s[j] == '-' && matches!(s[j - 1], 'e' | 'E')) {
                // Stop before `-` unless it is an exponent sign, and keep UUID-like literals whole.
                j += 1;
            }
            // Cassandra UUID / timeuuid literals: 8-4-4-4-12 hex groups.
            while j < s.len() && s[j] == '-' && s.get(j + 1).is_some_and(|n| n.is_ascii_hexdigit()) {
                let start = j;
                j += 1;
                while j < s.len() && s[j].is_ascii_hexdigit() {
                    j += 1;
                }
                if j - start < 5 {
                    j = start;
                    break;
                }
            }
            out.push(Tok { k: K::Num, t: s[i..j].iter().collect() });
            i = j;
        } else if c.is_alphanumeric() || c == '_' || c == '@' || c == '$' || c == ':' && next.is_some_and(|n| n.is_alphabetic()) && out.last().is_none_or(|t| !t.p(":")) {
            let mut j = i + 1;
            while j < s.len() && (s[j].is_alphanumeric() || s[j] == '_' || s[j] == '$') {
                j += 1;
            }
            out.push(Tok { k: K::Word, t: s[i..j].iter().collect() });
            i = j;
        } else {
            let two: String = s[i..(i + 2).min(s.len())].iter().collect();
            let t = if ["<=", ">=", "<>", "!=", "||", "::", "->", ":="].contains(&two.as_str()) { two } else { c.to_string() };
            i += t.chars().count();
            out.push(Tok { k: K::Punct, t });
        }
    }
    out
}

/// Tokens back to readable text, with SQL-ish spacing.
fn render(toks: &[Tok]) -> String {
    let mut out = String::new();
    let mut prev: Option<&Tok> = None;
    for t in toks {
        let text = match t.k {
            K::Str => format!("'{}'", t.t),
            K::Quoted => t.t.clone(),
            _ => t.t.clone(),
        };
        let glue = match prev {
            None => true,
            Some(p) => {
                p.p("(") || p.p(".") || p.p("[") || p.p("::") || t.p(")") || t.p(",") || t.p(".") || t.p("::") || t.p("]") || t.p(";")
                    || (t.p("(") && p.k == K::Word && !is_kw(&p.t))
                    || (t.p("[") && p.k != K::Punct)
                    || (t.p("<") && p.k == K::Word && is_collection(&p.t))
                    || (p.p("<") && out.ends_with('<') && type_angle(&out))
                    || (t.p(">") && type_angle(&out))
            }
        };
        if !glue {
            out.push(' ');
        }
        out.push_str(&text);
        prev = Some(t);
    }
    out
}

fn is_collection(w: &str) -> bool {
    ["map", "set", "list", "frozen", "tuple", "vector"].iter().any(|c| c.eq_ignore_ascii_case(w))
}

/// Whether `s` ends inside an open CQL collection type (`map<text`).
fn type_angle(s: &str) -> bool {
    s.matches('<').count() > s.matches('>').count() && s.contains('<') && {
        let open = s.rfind('<').unwrap();
        let word: String = s[..open].chars().rev().take_while(|c| c.is_alphanumeric() || *c == '_').collect::<String>().chars().rev().collect();
        is_collection(&word) || s.matches('<').count() > 1
    }
}

const KEYWORDS: &[&str] = &[
    "select", "from", "where", "and", "or", "not", "null", "is", "in", "like", "ilike", "between", "case", "when", "then", "else",
    "end", "true", "false", "as", "asc", "desc", "distinct", "on", "join", "left", "right", "inner", "outer", "full", "cross",
    "group", "by", "order", "having", "limit", "offset", "union", "all", "exists", "any", "some", "interval", "using", "with",
    "insert", "into", "values", "update", "set", "delete", "create", "table", "primary", "key", "foreign", "references",
    "default", "unique", "check", "constraint", "if", "natural", "over", "partition", "rows", "range", "preceding",
    "following", "current", "row", "unbounded", "filtering", "allow", "escape", "collate", "similar", "regexp", "rlike",
    "div", "mod", "xor", "cast", "convert", "extract", "date", "time", "timestamp", "within", "filter", "lateral",
    "returning", "per", "ttl", "writetime", "token", "contains", "nulls", "first", "last", "fetch", "next", "only",
    "recursive", "window", "except", "intersect", "minus", "duplicate", "conflict", "do", "nothing", "straight_join",
];

fn is_kw(w: &str) -> bool {
    KEYWORDS.iter().any(|k| k.eq_ignore_ascii_case(w))
}

/// Split at top-level (depth 0) occurrences of `sep`.
fn split_top<'a>(toks: &'a [Tok], sep: &str) -> Vec<&'a [Tok]> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, t) in toks.iter().enumerate() {
        if t.p("(") || t.p("[") {
            depth += 1;
        } else if t.p(")") || t.p("]") {
            depth -= 1;
        } else if depth == 0 && t.p(sep) {
            out.push(&toks[start..i]);
            start = i + 1;
        }
    }
    out.push(&toks[start..]);
    out.into_iter().filter(|s| !s.is_empty()).collect()
}

/// Like [`split_top`], but CQL collection types (`map<text, int>`) stay whole.
fn split_defs(toks: &[Tok]) -> Vec<&[Tok]> {
    let mut out = Vec::new();
    let (mut depth, mut angle) = (0i32, 0i32);
    let mut start = 0;
    for (i, t) in toks.iter().enumerate() {
        if t.p("(") || t.p("[") {
            depth += 1;
        } else if t.p(")") || t.p("]") {
            depth -= 1;
        } else if t.p("<") && (angle > 0 || i > 0 && toks[i - 1].k == K::Word && is_collection(&toks[i - 1].t)) {
            angle += 1;
        } else if t.p(">") && angle > 0 {
            angle -= 1;
        } else if depth == 0 && angle == 0 && t.p(",") {
            out.push(&toks[start..i]);
            start = i + 1;
        }
    }
    out.push(&toks[start..]);
    out.into_iter().filter(|s| !s.is_empty()).collect()
}

/// Index of the token closing the bracket opened at `open`.
fn close_of(toks: &[Tok], open: usize) -> usize {
    let mut depth = 0;
    for (i, t) in toks.iter().enumerate().skip(open) {
        if t.p("(") || t.p("[") {
            depth += 1;
        } else if t.p(")") || t.p("]") {
            depth -= 1;
            if depth == 0 {
                return i;
            }
        }
    }
    // Unclosed: treat the last token as the close so callers can step past it.
    toks.len().saturating_sub(1).max(open)
}

/// The contents of the parenthesised group starting at `open`.
fn group(toks: &[Tok], open: usize) -> &[Tok] {
    let end = close_of(toks, open).min(toks.len());
    &toks[(open + 1).min(end)..end]
}

/// Names inside a `(a, b, c)` list, ignoring sort orders and lengths.
fn name_list(toks: &[Tok]) -> Vec<String> {
    split_top(toks, ",")
        .into_iter()
        .filter_map(|part| part.iter().find(|t| t.ident()).map(|t| t.t.clone()))
        .collect()
}

/// A possibly qualified name (`db.schema.table`) starting at `i`: the last
/// part and the index after it.
fn qualified(toks: &[Tok], mut i: usize) -> Option<(String, usize)> {
    let mut name = toks.get(i).filter(|t| t.ident())?.t.clone();
    i += 1;
    while toks.get(i).is_some_and(|t| t.p(".")) && toks.get(i + 1).is_some_and(|t| t.ident()) {
        name = toks[i + 1].t.clone();
        i += 2;
    }
    Some((name, i))
}

fn skip_if_not_exists(toks: &[Tok], mut i: usize) -> usize {
    if toks.get(i).is_some_and(|t| t.is("if")) {
        i += 1;
        if toks.get(i).is_some_and(|t| t.is("not")) {
            i += 1;
        }
        if toks.get(i).is_some_and(|t| t.is("exists")) {
            i += 1;
        }
    }
    i
}

fn find_kw(toks: &[Tok], kw: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, t) in toks.iter().enumerate() {
        if t.p("(") {
            depth += 1;
        } else if t.p(")") {
            depth -= 1;
        } else if depth == 0 && t.is(kw) {
            return Some(i);
        }
    }
    None
}

// ---------------------------------------------------------------- model

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    Generic,
    MySql,
    Postgres,
    Sqlite,
    Cassandra,
}

impl Dialect {
    pub fn label(self) -> &'static str {
        match self {
            Dialect::Generic => "SQL",
            Dialect::MySql => "MySQL",
            Dialect::Postgres => "PostgreSQL",
            Dialect::Sqlite => "SQLite",
            Dialect::Cassandra => "Cassandra CQL",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    None,
    Primary,
    /// Cassandra partition key column.
    Partition,
    /// Cassandra clustering column.
    Clustering,
}

#[derive(Clone, Debug)]
pub struct Column {
    pub name: String,
    pub ty: String,
    pub key: Key,
    pub fk: bool,
    pub not_null: bool,
    pub unique: bool,
    pub indexed: bool,
    /// Short extra such as a clustering order ("desc") or "static".
    pub note: Option<String>,
}

impl Column {
    fn new(name: impl Into<String>, ty: impl Into<String>) -> Self {
        Self { name: name.into(), ty: ty.into(), key: Key::None, fk: false, not_null: false, unique: false, indexed: false, note: None }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityKind {
    Table,
    View,
    /// A user-defined type (CQL `CREATE TYPE`, PostgreSQL composite types).
    Type,
}

#[derive(Clone, Debug)]
pub struct Entity {
    pub name: String,
    pub kind: EntityKind,
    /// Alias, statement type or other short caption.
    pub caption: Option<String>,
    pub columns: Vec<Column>,
}

impl Entity {
    fn new(name: impl Into<String>, kind: EntityKind) -> Self {
        Self { name: name.into(), kind, caption: None, columns: Vec::new() }
    }

    pub fn col(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.name.eq_ignore_ascii_case(name))
    }

    /// The key a reference to this table points at when no column is named.
    fn key_col(&self) -> Option<usize> {
        self.columns.iter().position(|c| c.key != Key::None)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum LinkKind {
    /// Declared foreign key: many (from) to one (to).
    ForeignKey,
    /// Guessed from column names, e.g. `orders.user_id` → `users.id`.
    Inferred,
    /// A view reading from a table.
    Flow,
    /// A column whose type is a user-defined type.
    Uses,
}

#[derive(Clone, Debug)]
pub struct Link {
    pub from: usize,
    pub from_col: Option<usize>,
    pub to: usize,
    pub to_col: Option<usize>,
    pub kind: LinkKind,
}

/// One entity-relationship picture.
#[derive(Clone, Debug, Default)]
pub struct Diagram {
    pub entities: Vec<Entity>,
    pub links: Vec<Link>,
}

impl Diagram {
    fn find(&self, name: &str) -> Option<usize> {
        self.entities.iter().position(|e| e.name.eq_ignore_ascii_case(name))
    }

    fn add_link(&mut self, l: Link) {
        let dup = self.links.iter().any(|o| o.from == l.from && o.to == l.to && o.from_col == l.from_col && o.to_col == l.to_col && o.kind == l.kind);
        if !dup {
            self.links.push(l);
        }
    }
}

#[derive(Clone, Debug)]
pub struct Analysis {
    pub dialect: Dialect,
    pub schema: Diagram,
    /// Plain-English lines describing each relationship in the schema.
    pub relations: Vec<String>,
    /// Statements that were skipped, with a reason.
    pub skipped: Vec<String>,
}

impl Analysis {
    pub fn table_count(&self) -> usize {
        self.schema.entities.iter().filter(|e| e.kind == EntityKind::Table).count()
    }

    pub fn link_count(&self) -> usize {
        self.schema.links.iter().filter(|l| matches!(l.kind, LinkKind::ForeignKey | LinkKind::Inferred)).count()
    }
}

// ---------------------------------------------------------------- dialect

pub fn detect_dialect(src: &str) -> Dialect {
    let toks = tokenize(src);
    let lower = src.to_ascii_lowercase();
    let mut score = [0i32; 5]; // generic, mysql, postgres, sqlite, cassandra
    let has = |w: &str| toks.iter().any(|t| t.is(w));
    let seq = |a: &str, b: &str| toks.windows(2).any(|w| w[0].is(a) && w[1].is(b));
    if has("keyspace") || seq("clustering", "order") || seq("allow", "filtering") || seq("apply", "batch") || has("frozen") || has("timeuuid") || seq("using", "ttl") || seq("per", "partition") {
        score[4] += 5;
    }
    if toks.windows(4).any(|w| w[0].is("primary") && w[1].is("key") && w[2].p("(") && w[3].p("(")) {
        score[4] += 3;
    }
    if toks.windows(2).any(|w| (w[0].is("map") || w[0].is("set") || w[0].is("list")) && w[1].p("<")) {
        score[4] += 3;
    }
    if lower.contains('`') {
        score[1] += 3;
    }
    for w in ["auto_increment", "engine", "unsigned", "tinyint", "mediumtext", "longtext", "zerofill"] {
        if has(w) {
            score[1] += 2;
        }
    }
    if seq("duplicate", "key") {
        score[1] += 3;
    }
    for w in ["serial", "bigserial", "jsonb", "returning", "ilike", "timestamptz", "uuid_generate_v4", "gen_random_uuid"] {
        if has(w) {
            score[2] += 2;
        }
    }
    if toks.iter().any(|t| t.p("::")) || seq("on", "conflict") || seq("as", "identity") {
        score[2] += 2;
    }
    if has("autoincrement") || seq("without", "rowid") || has("pragma") {
        score[3] += 4;
    }
    let (best, &max) = score.iter().enumerate().max_by_key(|(_, s)| **s).unwrap();
    if max <= 0 {
        return Dialect::Generic;
    }
    [Dialect::Generic, Dialect::MySql, Dialect::Postgres, Dialect::Sqlite, Dialect::Cassandra][best]
}

// ---------------------------------------------------------------- DDL

/// Words that end a column's type and start its constraints.
const COLUMN_STOPS: &[&str] = &[
    "not", "null", "primary", "references", "unique", "default", "auto_increment", "autoincrement", "check", "constraint",
    "collate", "generated", "comment", "static", "identity", "on", "key", "stored", "virtual", "invisible", "visible", "as",
    "encode", "masked",
];

struct Pending {
    from_table: String,
    from_cols: Vec<String>,
    to_table: String,
    to_cols: Vec<String>,
}

fn type_text(toks: &[Tok]) -> String {
    let t = render(toks).to_ascii_lowercase();
    t.replace(" (", "(").replace("< ", "<").replace(" >", ">")
}

fn parse_references(toks: &[Tok], i: usize) -> Option<(String, Vec<String>)> {
    let (table, j) = qualified(toks, i)?;
    let cols = if toks.get(j).is_some_and(|t| t.p("(")) { name_list(group(toks, j)) } else { Vec::new() };
    Some((table, cols))
}

fn parse_column(part: &[Tok], table: &str, dialect: Dialect, pending: &mut Vec<Pending>) -> Option<Column> {
    let name = part.first().filter(|t| t.ident())?.t.clone();
    let mut i = 1;
    let mut depth = 0i32;
    let start = i;
    while i < part.len() {
        let t = &part[i];
        if t.p("(") || t.p("<") {
            depth += 1;
        } else if t.p(")") || t.p(">") {
            depth -= 1;
        } else if depth == 0 && t.k == K::Word && COLUMN_STOPS.iter().any(|s| t.is(s)) {
            // `character set` belongs to MySQL string columns; `character varying` is a type.
            break;
        } else if depth == 0 && t.is("character") && part.get(i + 1).is_some_and(|n| n.is("set")) {
            break;
        }
        i += 1;
    }
    let mut col = Column::new(name.clone(), type_text(&part[start..i]));
    let rest = &part[i..];
    let mut j = 0;
    while j < rest.len() {
        let t = &rest[j];
        if t.is("primary") || (t.is("key") && j == 0) {
            col.key = if dialect == Dialect::Cassandra { Key::Partition } else { Key::Primary };
            col.not_null = true;
        } else if t.is("not") && rest.get(j + 1).is_some_and(|n| n.is("null")) {
            col.not_null = true;
            j += 1;
        } else if t.is("unique") {
            col.unique = true;
        } else if t.is("static") {
            col.note = Some("static".into());
        } else if t.is("references") {
            if let Some((to_table, to_cols)) = parse_references(rest, j + 1) {
                col.fk = true;
                pending.push(Pending { from_table: table.into(), from_cols: vec![name.clone()], to_table, to_cols });
            }
        } else if (t.is("default") || t.is("check") || t.is("comment") || t.is("as")) && rest.get(j + 1).is_some_and(|n| n.p("(")) {
            j = close_of(rest, j + 1);
        } else if t.is("identity") || t.is("auto_increment") || t.is("autoincrement") {
            col.note.get_or_insert_with(|| "auto".into());
        }
        j += 1;
    }
    if col.ty.contains("serial") {
        col.note.get_or_insert_with(|| "auto".into());
    }
    Some(col)
}

fn apply_primary(e: &mut Entity, inner: &[Tok], dialect: Dialect) {
    let parts = split_top(inner, ",");
    for (n, part) in parts.iter().enumerate() {
        let names = if part.first().is_some_and(|t| t.p("(")) { name_list(group(part, 0)) } else { name_list(part) };
        for name in names {
            if let Some(c) = e.col(&name) {
                let col = &mut e.columns[c];
                col.not_null = true;
                col.key = match (dialect, n) {
                    (Dialect::Cassandra, 0) => Key::Partition,
                    (Dialect::Cassandra, _) => Key::Clustering,
                    _ => Key::Primary,
                };
            }
        }
    }
}

/// A table constraint (`PRIMARY KEY (...)`, `FOREIGN KEY ...`, `UNIQUE (...)`,
/// MySQL `KEY idx (...)`). Returns false when `part` is not a constraint.
fn parse_constraint(part: &[Tok], e: &mut Entity, dialect: Dialect, pending: &mut Vec<Pending>) -> bool {
    let mut i = 0;
    if part.first().is_some_and(|t| t.is("constraint")) {
        i = 2;
        // `CONSTRAINT name` may be followed by nothing we understand.
        if part.get(1).is_some_and(|t| t.is("primary") || t.is("foreign") || t.is("unique") || t.is("check")) {
            i = 1;
        }
    }
    let Some(head) = part.get(i) else { return true };
    let open = |from: usize| part.iter().skip(from).position(|t| t.p("(")).map(|p| p + from);
    if head.is("primary") {
        if let Some(o) = open(i) {
            apply_primary(e, group(part, o), dialect);
        }
        return true;
    }
    if head.is("foreign") {
        if let Some(o) = open(i) {
            let from_cols = name_list(group(part, o));
            let after = close_of(part, o) + 1;
            if let Some(r) = part.iter().skip(after).position(|t| t.is("references")).map(|p| p + after) {
                if let Some((to_table, to_cols)) = parse_references(part, r + 1) {
                    for c in &from_cols {
                        if let Some(ci) = e.col(c) {
                            e.columns[ci].fk = true;
                        }
                    }
                    pending.push(Pending { from_table: e.name.clone(), from_cols, to_table, to_cols });
                }
            }
        }
        return true;
    }
    if head.is("unique") || head.is("key") || head.is("index") || head.is("fulltext") || head.is("spatial") {
        if let Some(o) = open(i) {
            let cols = name_list(group(part, o));
            for c in &cols {
                if let Some(ci) = e.col(c) {
                    if head.is("unique") && cols.len() == 1 {
                        e.columns[ci].unique = true;
                    }
                    e.columns[ci].indexed = true;
                }
            }
        }
        return true;
    }
    head.is("check") || head.is("exclude") || head.is("period") || head.is("like") || i > 0
}

fn parse_create_table(toks: &[Tok], dialect: Dialect, d: &mut Diagram, pending: &mut Vec<Pending>) -> Option<()> {
    let t = find_kw(toks, "table")?;
    let i = skip_if_not_exists(toks, t + 1);
    let (name, i) = qualified(toks, i)?;
    let mut e = Entity::new(name.clone(), EntityKind::Table);
    if toks.get(i).is_some_and(|t| t.p("(")) {
        let inner = group(toks, i);
        let mut constraints = Vec::new();
        for part in split_defs(inner) {
            let first = &part[0];
            let looks_constraint = ["constraint", "primary", "foreign", "unique", "key", "index", "fulltext", "spatial", "check", "exclude", "period", "like"]
                .iter()
                .any(|k| first.is(k))
                && first.k == K::Word
                && !part.get(1).is_some_and(|n| n.ident() && !n.is("key") && !first.is("constraint") && !first.is("unique") && !first.is("key") && !first.is("index"));
            if looks_constraint {
                constraints.push(part);
            } else if let Some(col) = parse_column(part, &name, dialect, pending) {
                e.columns.push(col);
            }
        }
        for part in constraints {
            parse_constraint(part, &mut e, dialect, pending);
        }
        // CQL: WITH CLUSTERING ORDER BY (col DESC, ...)
        let rest = &toks[close_of(toks, i) + 1..];
        if let Some(c) = rest.windows(2).position(|w| w[0].is("clustering") && w[1].is("order")) {
            if let Some(o) = rest.iter().skip(c).position(|t| t.p("(")).map(|p| p + c) {
                for part in split_top(group(rest, o), ",") {
                    if let (Some(n), Some(dir)) = (part.first(), part.get(1)) {
                        if let Some(ci) = e.col(&n.t) {
                            e.columns[ci].note = Some(dir.t.to_ascii_lowercase());
                        }
                    }
                }
            }
        }
    } else if let Some(a) = find_kw(&toks[i..], "as") {
        // CREATE TABLE x AS SELECT ...: columns come from the select list.
        if let Some(q) = parse_select(&toks[i + a + 1..]) {
            for item in &q.items {
                e.columns.push(Column::new(item.name.clone(), ""));
            }
        }
        e.caption = Some("from query".into());
    }
    if let Some(old) = d.find(&name) {
        d.entities[old] = e;
    } else {
        d.entities.push(e);
    }
    Some(())
}

fn parse_create_type(toks: &[Tok], d: &mut Diagram) -> Option<()> {
    let t = find_kw(toks, "type")?;
    let i = skip_if_not_exists(toks, t + 1);
    let (name, mut i) = qualified(toks, i)?;
    let mut e = Entity::new(name, EntityKind::Type);
    if toks.get(i).is_some_and(|t| t.is("as")) {
        i += 1;
        if toks.get(i).is_some_and(|t| t.is("enum")) {
            if let Some(o) = toks.iter().skip(i).position(|t| t.p("(")).map(|p| p + i) {
                for v in split_top(group(toks, o), ",") {
                    e.columns.push(Column::new(render(v).trim_matches('\'').to_string(), ""));
                }
            }
            e.caption = Some("enum".into());
            d.entities.push(e);
            return Some(());
        }
    }
    if toks.get(i).is_some_and(|t| t.p("(")) {
        let mut ignored = Vec::new();
        for part in split_defs(group(toks, i)) {
            if let Some(c) = parse_column(part, "", Dialect::Generic, &mut ignored) {
                e.columns.push(c);
            }
        }
    }
    e.caption = Some("type".into());
    d.entities.push(e);
    Some(())
}

fn parse_create_index(toks: &[Tok], d: &mut Diagram) -> Option<()> {
    let on = find_kw(toks, "on")?;
    let (table, j) = qualified(toks, on + 1)?;
    let o = toks.iter().skip(j).position(|t| t.p("(")).map(|p| p + j)?;
    let unique = toks.iter().take(on).any(|t| t.is("unique"));
    let e = d.find(&table)?;
    let mut cols = Vec::new();
    for part in split_top(group(toks, o), ",") {
        // CQL `KEYS(col)`, `VALUES(col)`, `ENTRIES(col)` and expressions: take the innermost name.
        if let Some(n) = part.iter().rev().find(|t| t.ident() && !is_kw(&t.t) && !["keys", "values", "entries", "full", "lower", "upper"].iter().any(|k| t.is(k))) {
            cols.push(n.t.clone());
        }
    }
    let single = cols.len() == 1;
    for c in cols {
        if let Some(ci) = d.entities[e].col(&c) {
            d.entities[e].columns[ci].indexed = true;
            if unique && single {
                d.entities[e].columns[ci].unique = true;
            }
        }
    }
    Some(())
}

fn parse_alter(toks: &[Tok], dialect: Dialect, d: &mut Diagram, pending: &mut Vec<Pending>) -> Option<()> {
    let t = find_kw(toks, "table")?;
    let mut i = skip_if_not_exists(toks, t + 1);
    if toks.get(i).is_some_and(|t| t.is("only")) {
        i += 1;
    }
    let (name, i) = qualified(toks, i)?;
    let e = d.find(&name)?;
    for action in split_top(&toks[i..], ",") {
        let mut a = action;
        if !a.first().is_some_and(|t| t.is("add")) {
            continue;
        }
        a = &a[1..];
        if a.first().is_some_and(|t| t.is("column")) {
            a = &a[1..];
        }
        let a = &a[skip_if_not_exists(a, 0)..];
        let mut ent = std::mem::replace(&mut d.entities[e], Entity::new("", EntityKind::Table));
        if !parse_constraint(a, &mut ent, dialect, pending) {
            // CQL: ALTER TABLE t ADD (a int, b text)
            let parts: Vec<&[Tok]> = if a.first().is_some_and(|t| t.p("(")) { split_defs(group(a, 0)) } else { vec![a] };
            for p in parts {
                if let Some(c) = parse_column(p, &name, dialect, pending) {
                    ent.columns.push(c);
                }
            }
        }
        d.entities[e] = ent;
    }
    Some(())
}

fn parse_create_view(toks: &[Tok], d: &mut Diagram) -> Option<()> {
    let v = find_kw(toks, "view")?;
    let i = skip_if_not_exists(toks, v + 1);
    let (name, i) = qualified(toks, i)?;
    let a = find_kw(&toks[i..], "as")? + i;
    let body = &toks[a + 1..];
    // CQL materialized views end with PRIMARY KEY (...) WITH ...
    let end = find_kw(body, "primary").unwrap_or(body.len());
    let q = parse_select(&body[..end])?;
    let materialized = toks.iter().take(v).any(|t| t.is("materialized"));
    let mut e = Entity::new(name, EntityKind::View);
    e.caption = Some(if materialized { "materialized view" } else { "view" }.into());
    let sources: Vec<String> = q.sources.iter().map(|s| s.0.clone()).collect();
    for item in &q.items {
        if item.star {
            let src = item.star_of.clone().or_else(|| sources.first().cloned());
            if let Some(s) = src.and_then(|s| q.resolve(&s)).and_then(|si| d.find(&q.sources[si].0)) {
                let cols = d.entities[s].columns.iter().map(|c| Column { key: Key::None, fk: false, indexed: false, ..c.clone() }).collect::<Vec<_>>();
                e.columns.extend(cols);
            }
        } else {
            let ty = item.src.as_ref().and_then(|(qual, col)| {
                let t = q.table_of(qual.as_deref(), col, d)?;
                let c = d.entities[t].col(col)?;
                Some(d.entities[t].columns[c].ty.clone())
            });
            e.columns.push(Column::new(item.name.clone(), ty.unwrap_or_default()));
        }
    }
    if end < body.len() {
        let pk = &body[end..];
        if let Some(o) = pk.iter().position(|t| t.p("(")) {
            apply_primary(&mut e, group(pk, o), Dialect::Cassandra);
        }
    }
    let view_idx = d.entities.len();
    d.entities.push(e);
    for s in sources {
        if let Some(si) = d.find(&s) {
            d.add_link(Link { from: si, from_col: None, to: view_idx, to_col: None, kind: LinkKind::Flow });
        }
    }
    Some(())
}

fn resolve_pending(d: &mut Diagram, pending: Vec<Pending>) {
    for p in pending {
        let (Some(from), Some(to)) = (d.find(&p.from_table), d.find(&p.to_table)) else { continue };
        for (n, fc) in p.from_cols.iter().enumerate() {
            let from_col = d.entities[from].col(fc);
            if let Some(c) = from_col {
                d.entities[from].columns[c].fk = true;
            }
            let to_col = p.to_cols.get(n).and_then(|c| d.entities[to].col(c)).or_else(|| d.entities[to].key_col());
            d.add_link(Link { from, from_col, to, to_col, kind: LinkKind::ForeignKey });
        }
    }
}

/// Link columns whose type names a user-defined type (`frozen<address>`).
fn link_types(d: &mut Diagram) {
    let types: Vec<(usize, String)> = d.entities.iter().enumerate().filter(|(_, e)| e.kind == EntityKind::Type).map(|(i, e)| (i, e.name.to_ascii_lowercase())).collect();
    if types.is_empty() {
        return;
    }
    let mut links = Vec::new();
    for (ei, e) in d.entities.iter().enumerate() {
        for (ci, c) in e.columns.iter().enumerate() {
            let words: Vec<&str> = c.ty.split(|ch: char| !(ch.is_alphanumeric() || ch == '_')).collect();
            for (ti, tn) in &types {
                if *ti != ei && words.iter().any(|w| w == tn) {
                    links.push(Link { from: ei, from_col: Some(ci), to: *ti, to_col: None, kind: LinkKind::Uses });
                }
            }
        }
    }
    for l in links {
        d.add_link(l);
    }
}

fn singular_forms(stem: &str) -> Vec<String> {
    let s = stem.to_ascii_lowercase();
    let mut v = vec![s.clone(), format!("{s}s"), format!("{s}es")];
    if let Some(b) = s.strip_suffix('y') {
        v.push(format!("{b}ies"));
    }
    v
}

/// Guess links from naming conventions: `order_items.product_id` →
/// `products.id`, and Cassandra-style `orders_by_user.user_id` → `users.user_id`.
pub fn infer_links(d: &mut Diagram) {
    let mut found = Vec::new();
    for (ei, e) in d.entities.iter().enumerate() {
        if e.kind != EntityKind::Table && e.kind != EntityKind::View {
            continue;
        }
        for (ci, c) in e.columns.iter().enumerate() {
            if c.fk || d.links.iter().any(|l| l.from == ei && l.from_col == Some(ci)) {
                continue;
            }
            let lower = c.name.to_ascii_lowercase();
            let stem = lower.strip_suffix("_id").or_else(|| lower.strip_suffix("id").filter(|s| s.len() > 1 && c.name.ends_with("Id")));
            let Some(stem) = stem.map(|s| s.trim_end_matches('_')) else { continue };
            if stem.is_empty() {
                continue;
            }
            let names = singular_forms(stem);
            let target = d.entities.iter().enumerate().find(|(ti, t)| {
                *ti != ei && t.kind == EntityKind::Table && names.iter().any(|n| t.name.eq_ignore_ascii_case(n))
            });
            if let Some((ti, t)) = target {
                let tc = t.col("id").or_else(|| t.col(&c.name)).or_else(|| t.key_col());
                // A table that is itself keyed by this column is a lookup/denormalised copy,
                // but it still points at the owning table.
                found.push(Link { from: ei, from_col: Some(ci), to: ti, to_col: tc, kind: LinkKind::Inferred });
            }
        }
    }
    for l in found {
        d.add_link(l);
    }
}

fn describe_relations(d: &Diagram) -> Vec<String> {
    let mut out = Vec::new();
    for l in &d.links {
        let (f, t) = (&d.entities[l.from], &d.entities[l.to]);
        let col = |e: &Entity, c: Option<usize>| c.map(|c| format!("{}.{}", e.name, e.columns[c].name)).unwrap_or_else(|| e.name.clone());
        let line = match l.kind {
            LinkKind::ForeignKey | LinkKind::Inferred => {
                let unique = l.from_col.is_some_and(|c| f.columns[c].unique || (f.columns[c].key != Key::None && f.columns.iter().filter(|x| x.key != Key::None).count() == 1));
                let card = if unique { "one-to-one" } else { "many-to-one" };
                let many = if unique { "Each" } else { "Many" };
                let guess = if l.kind == LinkKind::Inferred { " (guessed from the column name)" } else { "" };
                format!("{} → {} · {card}: {many} {} row{} can point at one {} row{guess}", col(f, l.from_col), col(t, l.to_col), f.name, if unique { "" } else { "s" }, t.name)
            }
            LinkKind::Flow => format!("{} reads from {}", t.name, f.name),
            LinkKind::Uses => format!("{} → {} · uses the type: each value is a {} record", col(f, l.from_col), t.name, t.name),
        };
        out.push(line);
    }
    out
}

// ---------------------------------------------------------------- view bodies

/// One output column of a view's SELECT.
struct SelItem {
    name: String,
    star: bool,
    /// `t.*`: the qualifier.
    star_of: Option<String>,
    /// The first column the expression reads, as (qualifier, column).
    src: Option<(Option<String>, String)>,
}

/// Just enough of a SELECT to give a view its columns and sources.
struct Select {
    items: Vec<SelItem>,
    /// Named tables in FROM / JOIN, with their aliases.
    sources: Vec<(String, Option<String>)>,
}

impl Select {
    /// The source an alias or table name refers to.
    fn resolve(&self, qual: &str) -> Option<usize> {
        self.sources
            .iter()
            .position(|s| s.1.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(qual)))
            .or_else(|| self.sources.iter().position(|s| s.0.eq_ignore_ascii_case(qual)))
    }

    /// The schema entity a column reference belongs to, using the schema for bare names.
    fn table_of(&self, qual: Option<&str>, col: &str, schema: &Diagram) -> Option<usize> {
        if let Some(q) = qual {
            return schema.find(&self.sources[self.resolve(q)?].0);
        }
        let hits: Vec<usize> = self.sources.iter().filter_map(|s| schema.find(&s.0)).filter(|&e| schema.entities[e].col(col).is_some()).collect();
        (hits.len() == 1).then(|| hits[0])
    }
}

/// Words that end the select list or the FROM clause at depth 0.
const SELECT_ENDS: &[&str] = &["where", "group", "having", "order", "limit", "offset", "fetch", "window", "qualify", "allow", "per", "for", "lock", "union", "intersect", "except", "minus"];

fn find_any(toks: &[Tok], kws: &[&str]) -> Option<usize> {
    let mut depth = 0;
    for (i, t) in toks.iter().enumerate() {
        if t.p("(") {
            depth += 1;
        } else if t.p(")") {
            depth -= 1;
        } else if depth == 0 && t.k == K::Word && kws.iter().any(|k| t.is(k)) {
            return Some(i);
        }
    }
    None
}

fn item_name(toks: &[Tok]) -> (String, &[Tok]) {
    let n = toks.len();
    if n >= 2 && toks[n - 2].is("as") && toks[n - 1].ident() {
        return (toks[n - 1].t.clone(), &toks[..n - 2]);
    }
    // `expr alias` without AS: an identifier after something that is not an operator.
    if n >= 2 && toks[n - 1].ident() && !is_kw(&toks[n - 1].t) && !toks[n - 2].p(".") && (toks[n - 2].p(")") || toks[n - 2].ident() || toks[n - 2].k == K::Str || toks[n - 2].k == K::Num) && !toks[n - 2].is("distinct") {
        return (toks[n - 1].t.clone(), &toks[..n - 1]);
    }
    // Bare or qualified column: its own name.
    if let Some(last) = toks.last() {
        if last.ident() && (n == 1 || toks[n - 2].p(".")) {
            return (last.t.clone(), toks);
        }
    }
    let text = render(toks);
    let short: String = text.chars().take(28).collect();
    (if text.chars().count() > 28 { format!("{short}…") } else { short }, toks)
}

/// The first column an expression reads: `a.b` or a bare name that is not a
/// keyword or a function call.
fn first_ref(toks: &[Tok]) -> Option<(Option<String>, String)> {
    let mut i = 0;
    while i < toks.len() {
        let t = &toks[i];
        let plain = t.ident() && !(t.k == K::Word && is_kw(&t.t)) && !toks.get(i + 1).is_some_and(|n| n.p("(")) && !(i > 0 && (toks[i - 1].p("::") || toks[i - 1].is("as")));
        if plain {
            if toks.get(i + 1).is_some_and(|n| n.p(".")) {
                // a.b.c → qualifier b, column c
                let mut j = i;
                while toks.get(j + 1).is_some_and(|x| x.p(".")) && toks.get(j + 2).is_some_and(|x| x.ident()) {
                    j += 2;
                }
                if j > i && !toks.get(j + 1).is_some_and(|x| x.p("(")) {
                    return Some((Some(toks[j - 2].t.clone()), toks[j].t.clone()));
                }
                i = j + 1;
                continue;
            }
            return Some((None, t.t.clone()));
        }
        i += 1;
    }
    None
}

fn join_words(toks: &[Tok], i: usize) -> Option<usize> {
    let mut j = i;
    while let Some(t) = toks.get(j) {
        if ["natural", "left", "right", "full", "inner", "outer", "cross", "join", "straight_join", "lateral", "semi", "anti"].iter().any(|k| t.is(k)) {
            j += 1;
            if t.is("join") || t.is("straight_join") {
                return Some(j);
            }
        } else {
            break;
        }
    }
    None
}

/// Named tables in a FROM clause; sub-selects and ON conditions are skipped.
fn from_sources(toks: &[Tok]) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut expect = true;
    while i < toks.len() {
        let t = &toks[i];
        if t.p("(") {
            expect = false;
            i = close_of(toks, i) + 1;
        } else if t.p(",") {
            expect = true;
            i += 1;
        } else if let Some(next) = join_words(toks, i) {
            expect = true;
            i = next;
        } else if expect && !t.is("lateral") && let Some((name, mut j)) = qualified(toks, i) {
            if toks.get(j).is_some_and(|t| t.is("as")) {
                j += 1;
            }
            let alias = toks.get(j).filter(|t| t.ident() && !is_kw(&t.t)).map(|t| t.t.clone());
            if alias.is_some() {
                j += 1;
            }
            out.push((name, alias));
            expect = false;
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

/// Read the first SELECT of a statement (after any CTEs).
fn parse_select(toks: &[Tok]) -> Option<Select> {
    let mut toks = toks;
    while toks.first().is_some_and(|t| t.p("(")) && close_of(toks, 0) + 1 == toks.len() {
        toks = group(toks, 0);
    }
    if toks.first()?.is("with") {
        let mut i = 1;
        if toks.get(i).is_some_and(|t| t.is("recursive")) {
            i += 1;
        }
        loop {
            let (_, mut j) = qualified(toks, i)?;
            if toks.get(j).is_some_and(|t| t.p("(")) {
                j = close_of(toks, j) + 1;
            }
            if !toks.get(j).is_some_and(|t| t.is("as")) {
                return None;
            }
            j += 1;
            while toks.get(j).is_some_and(|t| t.is("not") || t.is("materialized")) {
                j += 1;
            }
            if !toks.get(j).is_some_and(|t| t.p("(")) {
                return None;
            }
            i = close_of(toks, j) + 1;
            if toks.get(i).is_some_and(|t| t.p(",")) {
                i += 1;
            } else {
                break;
            }
        }
        return parse_select(toks.get(i..)?);
    }
    if !toks.first()?.is("select") {
        return None;
    }
    let from = find_kw(toks, "from");
    let list_end = from.or_else(|| find_any(toks, SELECT_ENDS)).unwrap_or(toks.len());
    let mut sel = &toks[1.min(list_end)..list_end];
    while let Some(t) = sel.first() {
        if t.is("distinct") || t.is("distinctrow") {
            sel = &sel[1..];
            if sel.first().is_some_and(|t| t.is("on")) && sel.get(1).is_some_and(|t| t.p("(")) {
                sel = &sel[(close_of(sel, 1) + 1).min(sel.len())..];
            }
        } else if t.is("all") || t.is("sql_calc_found_rows") || t.is("high_priority") || t.is("straight_join") {
            sel = &sel[1..];
        } else if t.is("top") {
            sel = &sel[2.min(sel.len())..];
        } else {
            break;
        }
    }
    let items = split_top(sel, ",")
        .into_iter()
        .map(|part| {
            let (name, expr) = item_name(part);
            let star = expr.last().is_some_and(|t| t.p("*")) && (expr.len() == 1 || expr.len() == 3 && expr[1].p("."));
            let star_of = (star && expr.len() == 3).then(|| expr[0].t.clone());
            SelItem { name: if star { render(expr) } else { name }, star, star_of, src: if star { None } else { first_ref(expr) } }
        })
        .collect();
    let sources = match from {
        Some(f) => {
            let body = &toks[f + 1..];
            from_sources(&body[..find_any(body, SELECT_ENDS).unwrap_or(body.len())])
        }
        None => Vec::new(),
    };
    Some(Select { items, sources })
}

fn shorten(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n - 1).collect::<String>())
    }
}

// ---------------------------------------------------------------- entry point

/// Split into statements, handling CQL batches.
fn statements(toks: &[Tok]) -> Vec<&[Tok]> {
    let mut out = Vec::new();
    for mut s in split_top(toks, ";") {
        while s.first().is_some_and(|t| t.is("begin")) {
            let b = s.iter().position(|t| t.is("batch")).map(|p| p + 1).unwrap_or(1);
            s = &s[b..];
        }
        if s.first().is_some_and(|t| t.is("apply")) || s.is_empty() {
            continue;
        }
        out.push(s);
    }
    out
}

pub fn analyze(src: &str, infer: bool) -> Analysis {
    let dialect = detect_dialect(src);
    let toks = tokenize(src);
    let stmts = statements(&toks);
    let mut schema = Diagram::default();
    let mut pending = Vec::new();
    let mut views = Vec::new();
    let mut skipped = Vec::new();
    for s in &stmts {
        let head: Vec<String> = s.iter().take(6).filter(|t| t.k == K::Word).map(|t| t.t.to_ascii_lowercase()).collect();
        let has = |w: &str| head.iter().any(|h| h == w);
        let first = head.first().map(|s| s.as_str()).unwrap_or("");
        match first {
            "create" if has("table") => {
                parse_create_table(s, dialect, &mut schema, &mut pending);
            }
            "create" if has("view") => views.push(*s),
            "create" if has("type") => {
                parse_create_type(s, &mut schema);
            }
            "create" if has("index") => {}
            "alter" if has("table") => {}
            // Queries and other statements that don't change the schema.
            "select" | "with" | "insert" | "update" | "delete" | "replace" | "upsert" | "merge" | "explain" => {}
            "create" | "alter" | "drop" | "use" | "set" | "pragma" | "grant" | "revoke" | "truncate" | "begin" | "commit" | "rollback" | "start" | "lock" | "unlock" | "comment" | "delimiter" => {}
            _ => skipped.push(shorten(&render(&s[..s.len().min(12)]), 60)),
        }
    }
    // Indexes and ALTERs refer to tables that may be declared anywhere above.
    for s in &stmts {
        let head: Vec<String> = s.iter().take(6).filter(|t| t.k == K::Word).map(|t| t.t.to_ascii_lowercase()).collect();
        if head.first().is_some_and(|h| h == "alter") && head.iter().any(|h| h == "table") {
            parse_alter(s, dialect, &mut schema, &mut pending);
        } else if head.first().is_some_and(|h| h == "create") && head.iter().any(|h| h == "index") {
            parse_create_index(s, &mut schema);
        }
    }
    resolve_pending(&mut schema, pending);
    for v in views {
        parse_create_view(v, &mut schema);
    }
    link_types(&mut schema);
    if infer {
        infer_links(&mut schema);
    }
    let relations = describe_relations(&schema);
    Analysis { dialect, schema, relations, skipped }
}

// ---------------------------------------------------------------- export

/// The schema as a Mermaid `erDiagram`.
pub fn to_mermaid(d: &Diagram) -> String {
    let clean = |s: &str| s.chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' }).collect::<String>();
    let mut out = String::from("erDiagram\n");
    for e in d.entities.iter().filter(|e| matches!(e.kind, EntityKind::Table | EntityKind::View | EntityKind::Type)) {
        out.push_str(&format!("    {} {{\n", clean(&e.name)));
        for c in &e.columns {
            let ty = if c.ty.is_empty() { "unknown".to_string() } else { clean(&c.ty.replace(' ', "_")) };
            let mut keys = Vec::new();
            if c.key != Key::None {
                keys.push("PK");
            }
            if c.fk {
                keys.push("FK");
            }
            if c.unique && c.key == Key::None {
                keys.push("UK");
            }
            let keys = if keys.is_empty() { String::new() } else { format!(" {}", keys.join(",")) };
            let note = match c.key {
                Key::Partition => " \"partition key\"",
                Key::Clustering => " \"clustering key\"",
                _ => "",
            };
            out.push_str(&format!("        {ty} {}{keys}{note}\n", clean(&c.name)));
        }
        out.push_str("    }\n");
    }
    for l in &d.links {
        let (f, t) = (&d.entities[l.from], &d.entities[l.to]);
        let (rel, label) = match &l.kind {
            LinkKind::ForeignKey => {
                let unique = l.from_col.is_some_and(|c| f.columns[c].unique);
                (if unique { "|o--||" } else { "}o--||" }, l.from_col.map(|c| f.columns[c].name.clone()).unwrap_or_else(|| "references".into()))
            }
            LinkKind::Inferred => ("}o..||", l.from_col.map(|c| f.columns[c].name.clone()).unwrap_or_else(|| "references".into())),
            LinkKind::Uses => ("}o..||", "uses".into()),
            LinkKind::Flow => ("}o..o{", "feeds".into()),
        };
        out.push_str(&format!("    {} {rel} {} : \"{}\"\n", clean(&f.name), clean(&t.name), label.replace('"', "'")));
    }
    out
}

// ---------------------------------------------------------------- layout

/// Positions for boxes of the given sizes, in columns so that links run left
/// to right (`from` left of `to`). Unlinked boxes go in a grid underneath.
pub fn layout(sizes: &[(f32, f32)], links: &[(usize, usize)], gap_x: f32, gap_y: f32) -> Vec<(f32, f32)> {
    let n = sizes.len();
    let mut out = vec![(0f32, 0f32); n];
    if n == 0 {
        return out;
    }
    let mut succ = vec![Vec::new(); n];
    let mut connected = vec![false; n];
    for &(a, b) in links {
        if a != b && a < n && b < n && !succ[a].contains(&b) {
            succ[a].push(b);
            connected[a] = true;
            connected[b] = true;
        }
    }
    // Drop back edges so cycles don't stretch the layering.
    let mut state = vec![0u8; n];
    let mut dag = vec![Vec::new(); n];
    fn dfs(v: usize, succ: &[Vec<usize>], state: &mut [u8], dag: &mut [Vec<usize>]) {
        state[v] = 1;
        for &w in &succ[v] {
            if state[w] == 1 {
                continue;
            }
            dag[v].push(w);
            if state[w] == 0 {
                dfs(w, succ, state, dag);
            }
        }
        state[v] = 2;
    }
    for v in 0..n {
        if state[v] == 0 && connected[v] {
            dfs(v, &succ, &mut state, &mut dag);
        }
    }
    // Longest path from the sources.
    let mut layer = vec![0usize; n];
    for _ in 0..n {
        let mut changed = false;
        for v in 0..n {
            for &w in &dag[v] {
                if layer[w] < layer[v] + 1 {
                    layer[w] = layer[v] + 1;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    // Pull sources toward their targets so short chains don't hug the left edge.
    for v in 0..n {
        if connected[v] && !dag[v].is_empty() {
            let min_next = dag[v].iter().map(|&w| layer[w]).min().unwrap();
            if min_next > layer[v] + 1 && !dag.iter().any(|s| s.contains(&v)) {
                layer[v] = min_next - 1;
            }
        }
    }
    let layers = connected.iter().zip(&layer).filter(|(c, _)| **c).map(|(_, l)| *l + 1).max().unwrap_or(0);
    let mut cols: Vec<Vec<usize>> = vec![Vec::new(); layers];
    for v in 0..n {
        if connected[v] {
            cols[layer[v]].push(v);
        }
    }
    // Barycentre ordering to reduce crossings.
    let mut undirected = vec![Vec::new(); n];
    for (a, s) in dag.iter().enumerate() {
        for &b in s {
            undirected[a].push(b);
            undirected[b].push(a);
        }
    }
    let mut pos = vec![0f32; n];
    for col in &cols {
        for (i, &v) in col.iter().enumerate() {
            pos[v] = i as f32;
        }
    }
    for sweep in 0..8 {
        let order: Vec<usize> = if sweep % 2 == 0 { (0..layers).collect() } else { (0..layers).rev().collect() };
        for l in order {
            let col = &mut cols[l];
            let mut keyed: Vec<(f32, usize)> = col
                .iter()
                .map(|&v| {
                    let nb: Vec<f32> = undirected[v].iter().filter(|&&w| layer[w] != l).map(|&w| pos[w]).collect();
                    (if nb.is_empty() { pos[v] } else { nb.iter().sum::<f32>() / nb.len() as f32 }, v)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            *col = keyed.into_iter().map(|(_, v)| v).collect();
            for (i, &v) in col.iter().enumerate() {
                pos[v] = i as f32;
            }
        }
    }
    let heights: Vec<f32> = cols.iter().map(|c| c.iter().map(|&v| sizes[v].1).sum::<f32>() + gap_y * c.len().saturating_sub(1) as f32).collect();
    let tallest = heights.iter().cloned().fold(0f32, f32::max);
    let mut x = 0f32;
    for (l, col) in cols.iter().enumerate() {
        let w = col.iter().map(|&v| sizes[v].0).fold(0f32, f32::max);
        let mut y = (tallest - heights[l]) / 2.;
        for &v in col {
            out[v] = (x, y);
            y += sizes[v].1 + gap_y;
        }
        x += w + gap_x;
    }
    // Unlinked boxes: rows under the linked part.
    let total_w = (x - gap_x).max(900.);
    let (mut gx, mut gy, mut row_h) = (0f32, if layers > 0 { tallest + gap_y * 2. } else { 0. }, 0f32);
    for v in 0..n {
        if connected[v] {
            continue;
        }
        if gx > 0. && gx + sizes[v].0 > total_w {
            gx = 0.;
            gy += row_h + gap_y;
            row_h = 0.;
        }
        out[v] = (gx, gy);
        gx += sizes[v].0 + gap_x / 2.;
        row_h = row_h.max(sizes[v].1);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHOP: &str = "
CREATE TABLE users (
  id BIGINT PRIMARY KEY,
  email VARCHAR(255) NOT NULL UNIQUE,
  created_at TIMESTAMP NOT NULL
);
CREATE TABLE orders (
  id BIGINT PRIMARY KEY,
  user_id BIGINT NOT NULL,
  total_cents INT NOT NULL,
  FOREIGN KEY (user_id) REFERENCES users(id)
);
CREATE TABLE order_items (
  id BIGINT PRIMARY KEY,
  order_id BIGINT NOT NULL,
  product_id BIGINT NOT NULL REFERENCES products(id),
  CONSTRAINT fk_oi_order FOREIGN KEY (order_id) REFERENCES orders(id)
);
CREATE TABLE products (id BIGINT PRIMARY KEY, sku VARCHAR(64) NOT NULL UNIQUE);
";

    fn ent<'a>(d: &'a Diagram, n: &str) -> &'a Entity {
        &d.entities[d.find(n).unwrap()]
    }

    #[test]
    fn schema_tables_and_foreign_keys() {
        let a = analyze(SHOP, false);
        assert_eq!(a.table_count(), 4);
        let users = ent(&a.schema, "users");
        assert_eq!(users.columns.len(), 3);
        assert_eq!(users.columns[0].key, Key::Primary);
        assert_eq!(users.columns[1].ty, "varchar(255)");
        assert!(users.columns[1].unique && users.columns[1].not_null);
        // orders.user_id, order_items.product_id (inline, forward reference), order_items.order_id
        assert_eq!(a.link_count(), 3);
        let oi = ent(&a.schema, "order_items");
        assert!(oi.columns.iter().filter(|c| c.fk).count() == 2);
        assert!(a.relations.iter().any(|r| r.starts_with("orders.user_id → users.id · many-to-one")));
    }

    #[test]
    fn mysql_details() {
        let src = "CREATE TABLE `shop`.`items` (
  `id` int unsigned NOT NULL AUTO_INCREMENT,
  `name` varchar(100) CHARACTER SET utf8mb4 NOT NULL DEFAULT '',
  `kind` enum('a','b') DEFAULT NULL,
  `owner_id` int unsigned,
  PRIMARY KEY (`id`),
  UNIQUE KEY `uq_name` (`name`),
  KEY `idx_owner` (`owner_id`),
  CONSTRAINT `fk_owner` FOREIGN KEY (`owner_id`) REFERENCES `owners` (`id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
CREATE TABLE owners (id int unsigned auto_increment primary key, name text);";
        let a = analyze(src, false);
        assert_eq!(a.dialect, Dialect::MySql);
        let t = ent(&a.schema, "items");
        assert_eq!(t.columns.len(), 4, "{:?}", t.columns);
        assert_eq!(t.columns[0].ty, "int unsigned");
        assert_eq!(t.columns[0].key, Key::Primary);
        assert_eq!(t.columns[1].ty, "varchar(100)");
        assert!(t.columns[1].unique);
        assert_eq!(t.columns[2].ty, "enum('a', 'b')");
        assert!(t.columns[3].indexed && t.columns[3].fk);
        assert_eq!(a.link_count(), 1);
    }

    #[test]
    fn alter_table_adds_links() {
        let src = "CREATE TABLE a (id int primary key); CREATE TABLE b (id int primary key, a_id int);
                   ALTER TABLE b ADD CONSTRAINT fk FOREIGN KEY (a_id) REFERENCES a (id);";
        let a = analyze(src, false);
        assert_eq!(a.link_count(), 1);
        assert_eq!(a.schema.links[0].kind, LinkKind::ForeignKey);
    }

    const CQL: &str = "
CREATE KEYSPACE shop WITH replication = {'class': 'SimpleStrategy', 'replication_factor': 1};
CREATE TYPE shop.address (street text, city text, zip text);
CREATE TABLE shop.users (user_id uuid PRIMARY KEY, email text, home frozen<address>);
CREATE TABLE shop.orders_by_user (
  user_id uuid,
  order_date date,
  order_id timeuuid,
  total decimal,
  items map<text, int>,
  PRIMARY KEY ((user_id), order_date, order_id)
) WITH CLUSTERING ORDER BY (order_date DESC, order_id ASC);
CREATE INDEX ON shop.orders_by_user (total);
";

    #[test]
    fn cassandra_schema() {
        let a = analyze(CQL, true);
        assert_eq!(a.dialect, Dialect::Cassandra);
        let o = ent(&a.schema, "orders_by_user");
        assert_eq!(o.columns[0].key, Key::Partition);
        assert_eq!(o.columns[1].key, Key::Clustering);
        assert_eq!(o.columns[1].note.as_deref(), Some("desc"));
        assert_eq!(o.columns[4].ty, "map<text, int>");
        assert!(o.columns[3].indexed);
        let u = ent(&a.schema, "users");
        assert_eq!(u.columns[0].key, Key::Partition);
        assert_eq!(u.columns[2].ty, "frozen<address>");
        // users.home uses the address type; orders_by_user.user_id is inferred to users.
        assert!(a.schema.links.iter().any(|l| l.kind == LinkKind::Uses));
        assert!(a.schema.links.iter().any(|l| l.kind == LinkKind::Inferred && a.schema.entities[l.to].name == "users"));
    }

    #[test]
    fn inferred_links_by_name() {
        let a = analyze("CREATE TABLE customers (id int primary key); CREATE TABLE invoices (id int primary key, customer_id int, categoryId int); CREATE TABLE categories (id int primary key);", true);
        assert_eq!(a.link_count(), 2);
        assert!(a.schema.links.iter().all(|l| l.kind == LinkKind::Inferred));
    }

    #[test]
    fn views_and_mermaid() {
        let src = format!("{SHOP} CREATE VIEW big_orders AS SELECT o.id, o.total_cents FROM orders o WHERE o.total_cents > 1000;");
        let a = analyze(&src, false);
        let v = ent(&a.schema, "big_orders");
        assert_eq!(v.kind, EntityKind::View);
        assert_eq!(v.columns[1].ty, "int");
        assert!(a.schema.links.iter().any(|l| l.kind == LinkKind::Flow && a.schema.entities[l.from].name == "orders"));
        let m = to_mermaid(&a.schema);
        assert!(m.starts_with("erDiagram\n"));
        assert!(m.contains("orders }o--|| users : \"user_id\""));
        assert!(m.contains("bigint id PK"));
    }

    #[test]
    fn view_columns_from_the_select() {
        let src = format!("{SHOP}
CREATE VIEW user_orders AS
  WITH x AS (SELECT 1) SELECT u.email, SUM(o.total_cents) AS spent, o.* FROM users u LEFT JOIN orders o ON o.user_id = u.id GROUP BY u.email;
CREATE TABLE emails AS SELECT email FROM users;");
        let a = analyze(&src, false);
        let v = ent(&a.schema, "user_orders");
        let cols: Vec<(&str, &str)> = v.columns.iter().map(|c| (c.name.as_str(), c.ty.as_str())).collect();
        assert_eq!(cols, [("email", "varchar(255)"), ("spent", "int"), ("id", "bigint"), ("user_id", "bigint"), ("total_cents", "int")]);
        assert_eq!(a.schema.links.iter().filter(|l| l.kind == LinkKind::Flow).count(), 2);
        let t = ent(&a.schema, "emails");
        assert_eq!(t.columns.len(), 1);
        assert_eq!(t.caption.as_deref(), Some("from query"));
    }

    #[test]
    fn queries_are_ignored() {
        let a = analyze(&format!("{SHOP} SELECT * FROM users; UPDATE orders SET total_cents = 0; DELETE FROM users;"), false);
        assert_eq!(a.table_count(), 4);
        assert!(a.skipped.is_empty(), "{:?}", a.skipped);
    }

    #[test]
    fn tolerant_of_junk() {
        let a = analyze("this is not sql ((( ; CREATE TABLE ; SELECT FROM ; )))", true);
        assert_eq!(a.table_count(), 0);
        let _ = analyze("", false);
        let _ = analyze("CREATE TABLE x (", false);
        let _ = analyze("SELECT a FROM (SELECT", false);
        let _ = analyze("WITH x AS (SELECT 1) SELECT * FROM x", false);
    }

    #[test]
    fn layout_puts_referenced_tables_right() {
        let sizes = vec![(100., 50.); 4];
        let pos = layout(&sizes, &[(0, 1), (1, 2)], 60., 30.);
        assert!(pos[0].0 < pos[1].0 && pos[1].0 < pos[2].0);
        // node 3 is unlinked and sits below.
        assert!(pos[3].1 > pos[0].1);
        // cycles terminate
        let pos = layout(&sizes, &[(0, 1), (1, 0), (2, 2)], 60., 30.);
        assert_eq!(pos.len(), 4);
    }

    #[test]
    fn dialects() {
        assert_eq!(detect_dialect("CREATE TABLE t (id SERIAL PRIMARY KEY, d JSONB)"), Dialect::Postgres);
        assert_eq!(detect_dialect("CREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT)"), Dialect::Sqlite);
        assert_eq!(detect_dialect("select * from t allow filtering"), Dialect::Cassandra);
        assert_eq!(detect_dialect("select 1"), Dialect::Generic);
    }
}
