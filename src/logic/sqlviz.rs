//! SQL / CQL → diagrams: schemas become entity-relationship diagrams, queries
//! become data-flow diagrams with a plain-English walk-through.
//!
//! The parser is deliberately forgiving: it understands the common shapes of
//! MySQL, PostgreSQL, SQLite and Cassandra (CQL) DDL and DML, skips anything
//! it does not recognise, and never fails outright.

use std::collections::{HashMap, HashSet};

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

const AGGREGATES: &[&str] = &[
    "count", "sum", "avg", "min", "max", "group_concat", "string_agg", "array_agg", "json_agg", "jsonb_agg", "bool_and",
    "bool_or", "stddev", "variance", "listagg", "json_arrayagg", "json_objectagg", "any_value", "median",
];

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
    /// Query diagrams: the query reads or writes this column.
    pub used: bool,
    /// Query diagrams: the column appears in a filter (WHERE / ON / HAVING).
    pub filtered: bool,
}

impl Column {
    fn new(name: impl Into<String>, ty: impl Into<String>) -> Self {
        Self { name: name.into(), ty: ty.into(), key: Key::None, fk: false, not_null: false, unique: false, indexed: false, note: None, used: false, filtered: false }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityKind {
    Table,
    View,
    /// A user-defined type (CQL `CREATE TYPE`, PostgreSQL composite types).
    Type,
    /// A common table expression (`WITH x AS (...)`).
    Cte,
    /// A sub-select used as a table.
    Derived,
    /// What a query returns or writes.
    Result,
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
    /// A join between two tables in a query, labelled with its type.
    Join(String),
    /// Data flowing from a source into a result, view or CTE.
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

/// One entity-relationship or data-flow picture.
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

/// One line of a query walk-through, in logical execution order.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    /// The clause, e.g. "FROM", "LEFT JOIN", "WHERE".
    pub clause: String,
    pub text: String,
    /// Nesting depth: steps of CTEs and sub-queries are indented.
    pub depth: usize,
}

#[derive(Clone, Debug)]
pub struct QueryView {
    /// A one-line preview of the statement.
    pub preview: String,
    pub summary: String,
    pub steps: Vec<Step>,
    pub warnings: Vec<String>,
    pub diagram: Diagram,
}

#[derive(Clone, Debug)]
pub struct Analysis {
    pub dialect: Dialect,
    pub schema: Diagram,
    /// Plain-English lines describing each relationship in the schema.
    pub relations: Vec<String>,
    pub queries: Vec<QueryView>,
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
        if let Some(q) = parse_query(&toks[i + a + 1..]) {
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
    let q = parse_query(&body[..end])?;
    let materialized = toks.iter().take(v).any(|t| t.is("materialized"));
    let mut e = Entity::new(name, EntityKind::View);
    e.caption = Some(if materialized { "materialized view" } else { "view" }.into());
    let sources: Vec<String> = q.sources.iter().filter(|s| s.sub.is_none()).map(|s| s.name.clone()).collect();
    for item in &q.items {
        if item.star {
            let src = item.star_of.clone().or_else(|| sources.first().cloned());
            if let Some(s) = src.and_then(|s| q.resolve(&s)).and_then(|si| d.find(&q.sources[si].name)) {
                let cols = d.entities[s].columns.iter().map(|c| Column { key: Key::None, fk: false, indexed: false, ..c.clone() }).collect::<Vec<_>>();
                e.columns.extend(cols);
            }
        } else {
            let ty = item.refs.first().and_then(|r| {
                let si = q.source_of(r, d)?;
                let t = d.find(&q.sources[si].name)?;
                let c = d.entities[t].col(&r.col)?;
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
            LinkKind::Join(_) => continue,
        };
        out.push(line);
    }
    out
}

// ---------------------------------------------------------------- queries

#[derive(Clone, Debug, PartialEq)]
pub struct ColRef {
    pub qual: Option<String>,
    pub col: String,
}

#[derive(Clone, Debug)]
struct SelItem {
    expr: String,
    name: String,
    refs: Vec<ColRef>,
    agg: bool,
    window: bool,
    star: bool,
    star_of: Option<String>,
}

#[derive(Clone, Debug)]
struct Source {
    name: String,
    alias: Option<String>,
    sub: Option<Box<Query>>,
    /// None for the first source; otherwise the join type ("LEFT JOIN", ",").
    join: Option<String>,
    on: Option<String>,
    on_refs: Vec<ColRef>,
    pairs: Vec<(ColRef, ColRef)>,
    using: Vec<String>,
}

#[derive(Clone, Debug, Default)]
struct Query {
    verb: String,
    ctes: Vec<(String, Query, bool)>,
    distinct: bool,
    items: Vec<SelItem>,
    sources: Vec<Source>,
    where_: Option<String>,
    where_refs: Vec<ColRef>,
    where_pairs: Vec<(ColRef, ColRef)>,
    where_eq: Vec<ColRef>,
    group_by: Vec<String>,
    group_refs: Vec<ColRef>,
    having: Option<String>,
    having_refs: Vec<ColRef>,
    order_by: Vec<String>,
    limit: Option<String>,
    offset: Option<String>,
    set_ops: Vec<(String, Query)>,
    subqueries: Vec<(String, Query)>,
    // DML
    target: Option<String>,
    assignments: Vec<(String, String, Vec<ColRef>)>,
    insert_cols: Vec<String>,
    value_rows: usize,
    insert_query: Option<Box<Query>>,
    delete_cols: Vec<String>,
    extras: Vec<String>,
    upsert: Option<String>,
    returning: Option<String>,
}

impl Query {
    /// The source an alias or table name refers to.
    fn resolve(&self, qual: &str) -> Option<usize> {
        self.sources
            .iter()
            .position(|s| s.alias.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(qual)))
            .or_else(|| self.sources.iter().position(|s| s.name.eq_ignore_ascii_case(qual)))
    }

    /// The source a column reference belongs to, using the schema for bare names.
    fn source_of(&self, r: &ColRef, schema: &Diagram) -> Option<usize> {
        if let Some(q) = &r.qual {
            return self.resolve(q);
        }
        if self.sources.len() == 1 {
            return Some(0);
        }
        let hits: Vec<usize> = self
            .sources
            .iter()
            .enumerate()
            .filter(|(_, s)| match &s.sub {
                Some(q) => q.items.iter().any(|i| i.name.eq_ignore_ascii_case(&r.col)),
                None => schema.find(&s.name).is_some_and(|e| schema.entities[e].col(&r.col).is_some()),
            })
            .map(|(i, _)| i)
            .collect();
        (hits.len() == 1).then(|| hits[0])
    }
}

const CLAUSES: &[&str] = &["from", "where", "group", "having", "order", "limit", "offset", "fetch", "union", "intersect", "except", "minus", "window", "allow", "per", "for", "into", "qualify", "returning", "lock"];

/// Column references in an expression, plus sub-queries found inside it.
fn refs_in(toks: &[Tok], subs: &mut Vec<(String, Query)>, ctx: &str) -> Vec<ColRef> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        let t = &toks[i];
        if t.p("(") && toks.get(i + 1).is_some_and(|n| n.is("select") || n.is("with")) {
            let end = close_of(toks, i);
            if let Some(q) = parse_query(&toks[i + 1..end.min(toks.len())]) {
                subs.push((ctx.to_string(), q));
            }
            i = end + 1;
            continue;
        }
        if t.ident() && !(t.k == K::Word && is_kw(&t.t)) && !toks.get(i + 1).is_some_and(|n| n.p("(")) && !(i > 0 && (toks[i - 1].p(".") || toks[i - 1].p("::") || toks[i - 1].is("as") || toks[i - 1].is("interval"))) && !t.t.starts_with(':') && !t.t.starts_with('@') {
            if toks.get(i + 1).is_some_and(|n| n.p(".")) {
                if let Some(n) = toks.get(i + 2) {
                    if n.ident() {
                        // a.b.c → qualifier b, column c
                        let (mut q, mut c, mut j) = (t.t.clone(), n.t.clone(), i + 3);
                        while toks.get(j).is_some_and(|x| x.p(".")) && toks.get(j + 1).is_some_and(|x| x.ident()) {
                            q = c;
                            c = toks[j + 1].t.clone();
                            j += 2;
                        }
                        if !toks.get(j).is_some_and(|x| x.p("(")) {
                            out.push(ColRef { qual: Some(q), col: c });
                        }
                        i = j;
                        continue;
                    } else if n.p("*") {
                        i += 3;
                        continue;
                    }
                }
            }
            let is_type_word = i > 0 && toks[i - 1].p("::");
            let upper_const = t.k == K::Word && ["current_date", "current_timestamp", "current_time", "now", "localtime", "localtimestamp", "sysdate", "current_user"].iter().any(|k| t.is(k));
            if !is_type_word && !upper_const {
                out.push(ColRef { qual: None, col: t.t.clone() });
            }
        }
        i += 1;
    }
    out
}

/// `a.x = b.y` pairs in a condition (only across AND, not OR).
fn eq_pairs(toks: &[Tok]) -> Vec<(ColRef, ColRef)> {
    let mut out = Vec::new();
    let col_at = |i: usize| -> Option<(ColRef, usize)> {
        let t = toks.get(i)?;
        if !t.ident() || is_kw(&t.t) {
            return None;
        }
        if toks.get(i + 1).is_some_and(|n| n.p(".")) && toks.get(i + 2).is_some_and(|n| n.ident()) {
            let mut j = i + 2;
            while toks.get(j + 1).is_some_and(|x| x.p(".")) && toks.get(j + 2).is_some_and(|x| x.ident()) {
                j += 2;
            }
            return Some((ColRef { qual: Some(toks[j - 2].t.clone()), col: toks[j].t.clone() }, j + 1));
        }
        (!toks.get(i + 1).is_some_and(|n| n.p("("))).then(|| (ColRef { qual: None, col: t.t.clone() }, i + 1))
    };
    let mut i = 0;
    while i < toks.len() {
        if i == 0 || !(toks[i - 1].p(".")) {
            if let Some((a, j)) = col_at(i) {
                if toks.get(j).is_some_and(|t| t.p("=")) {
                    if let Some((b, k)) = col_at(j + 1) {
                        out.push((a, b));
                        i = k;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    out
}

/// Columns compared with `=` or `IN` to a value (for partition-key checks).
fn eq_value_cols(toks: &[Tok]) -> Vec<ColRef> {
    let mut out = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        if !t.ident() || is_kw(&t.t) || toks.get(i + 1).is_some_and(|n| n.p(".")) {
            continue;
        }
        let op = toks.get(i + 1);
        if op.is_some_and(|o| o.p("=") || o.is("in")) {
            let qual = (i >= 2 && toks[i - 1].p(".")).then(|| toks[i - 2].t.clone());
            out.push(ColRef { qual, col: t.t.clone() });
        }
    }
    // (a, b) IN ((...), (...)) — CQL multi-column restrictions
    for (i, t) in toks.iter().enumerate() {
        if t.p("(") {
            let end = close_of(toks, i);
            if toks.get(end + 1).is_some_and(|n| n.is("in") || n.p("=")) {
                for n in name_list(&toks[i + 1..end.min(toks.len())]) {
                    out.push(ColRef { qual: None, col: n });
                }
            }
        }
    }
    out
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

fn parse_items(toks: &[Tok], subs: &mut Vec<(String, Query)>) -> Vec<SelItem> {
    split_top(toks, ",")
        .into_iter()
        .map(|part| {
            let (name, expr) = item_name(part);
            let star = expr.last().is_some_and(|t| t.p("*")) && (expr.len() == 1 || expr.len() == 3 && expr[1].p("."));
            let star_of = (star && expr.len() == 3).then(|| expr[0].t.clone());
            let agg = expr.windows(2).any(|w| w[0].k == K::Word && w[1].p("(") && AGGREGATES.iter().any(|a| w[0].is(a)));
            let window = expr.iter().any(|t| t.is("over"));
            SelItem { expr: render(expr), name: if star { render(expr) } else { name }, refs: if star { Vec::new() } else { refs_in(expr, subs, "SELECT") }, agg, window, star, star_of }
        })
        .collect()
}

fn join_words(toks: &[Tok], i: usize) -> Option<(String, usize)> {
    let mut j = i;
    let mut words = Vec::new();
    while let Some(t) = toks.get(j) {
        if ["natural", "left", "right", "full", "inner", "outer", "cross", "join", "straight_join", "lateral", "semi", "anti"].iter().any(|k| t.is(k)) {
            words.push(t.t.to_ascii_uppercase());
            j += 1;
            if t.is("join") || t.is("straight_join") {
                return Some((words.join(" ").replace(" OUTER", ""), j));
            }
        } else {
            break;
        }
    }
    None
}

fn parse_from(toks: &[Tok], q: &mut Query, subs: &mut Vec<(String, Query)>) {
    let mut i = 0;
    let mut join: Option<String> = None;
    while i < toks.len() {
        if toks[i].p(",") {
            join = Some(",".into());
            i += 1;
            continue;
        }
        if let Some((j, next)) = join_words(toks, i) {
            join = Some(if j == "JOIN" { "INNER JOIN".into() } else { j });
            i = next;
            continue;
        }
        if toks[i].is("lateral") {
            i += 1;
            continue;
        }
        // One table reference: name or (subquery), optional alias, then ON / USING.
        let mut src = Source { name: String::new(), alias: None, sub: None, join: join.take(), on: None, on_refs: Vec::new(), pairs: Vec::new(), using: Vec::new() };
        if toks[i].p("(") {
            let end = close_of(toks, i);
            let inner = &toks[i + 1..end.min(toks.len())];
            if let Some(sq) = parse_query(inner) {
                src.sub = Some(Box::new(sq));
            }
            i = end + 1;
        } else if let Some((name, j)) = qualified(toks, i) {
            src.name = name;
            i = j;
            // Table functions: generate_series(...), unnest(...)
            if toks.get(i).is_some_and(|t| t.p("(")) {
                i = close_of(toks, i) + 1;
            }
        } else {
            i += 1;
            continue;
        }
        if toks.get(i).is_some_and(|t| t.is("as")) {
            i += 1;
        }
        if let Some(t) = toks.get(i) {
            if t.ident() && !is_kw(&t.t) && !["use", "force", "ignore", "tablesample"].iter().any(|k| t.is(k)) {
                src.alias = Some(t.t.clone());
                i += 1;
                // column alias list: AS t(a, b)
                if toks.get(i).is_some_and(|t| t.p("(")) {
                    i = close_of(toks, i) + 1;
                }
            }
        }
        // MySQL index hints: USE INDEX (...)
        while toks.get(i).is_some_and(|t| t.is("use") || t.is("force") || t.is("ignore")) {
            if let Some(o) = toks.iter().skip(i).position(|t| t.p("(")) {
                i = close_of(toks, i + o) + 1;
            } else {
                break;
            }
        }
        if src.sub.is_some() && src.alias.is_none() {
            src.alias = Some(format!("subquery {}", q.sources.len() + 1));
        }
        if src.sub.is_some() && src.name.is_empty() {
            src.name = src.alias.clone().unwrap_or_default();
        }
        if toks.get(i).is_some_and(|t| t.is("on")) {
            let start = i + 1;
            let mut j = start;
            let mut depth = 0;
            while j < toks.len() {
                let t = &toks[j];
                if t.p("(") {
                    depth += 1;
                } else if t.p(")") {
                    depth -= 1;
                } else if depth == 0 && (t.p(",") || join_words(toks, j).is_some()) {
                    break;
                }
                j += 1;
            }
            let cond = &toks[start..j];
            src.on = Some(render(cond));
            src.on_refs = refs_in(cond, subs, "ON");
            src.pairs = eq_pairs(cond);
            i = j;
        } else if toks.get(i).is_some_and(|t| t.is("using")) && toks.get(i + 1).is_some_and(|t| t.p("(")) {
            src.using = name_list(group(toks, i + 1));
            i = close_of(toks, i + 1) + 1;
        }
        q.sources.push(src);
    }
}

/// Clause boundaries at depth 0: (keyword index, clause name).
fn clause_marks(toks: &[Tok]) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut depth = 0;
    for (i, t) in toks.iter().enumerate() {
        if t.p("(") {
            depth += 1;
        } else if t.p(")") {
            depth -= 1;
        } else if depth == 0 && t.k == K::Word && CLAUSES.iter().any(|c| t.is(c)) {
            let name = t.t.to_ascii_lowercase();
            // `FOR UPDATE`, `ALLOW FILTERING`, `PER PARTITION LIMIT`: only as whole phrases.
            let ok = match name.as_str() {
                "allow" => toks.get(i + 1).is_some_and(|n| n.is("filtering")),
                "per" => toks.get(i + 1).is_some_and(|n| n.is("partition")),
                "for" => toks.get(i + 1).is_some_and(|n| n.is("update") || n.is("share")),
                "lock" => toks.get(i + 1).is_some_and(|n| n.is("in")),
                "group" | "order" => toks.get(i + 1).is_some_and(|n| n.is("by")),
                "limit" => !out.last().is_some_and(|(_, c): &(usize, String)| c == "per"),
                _ => true,
            };
            if ok {
                out.push((i, name));
            }
        }
    }
    out
}

fn parse_select(toks: &[Tok], q: &mut Query) {
    let mut subs = Vec::new();
    let marks = clause_marks(toks);
    let body_end = marks.first().map(|m| m.0).unwrap_or(toks.len());
    let mut sel = &toks[1.min(toks.len())..body_end];
    while let Some(t) = sel.first() {
        if t.is("distinct") || t.is("distinctrow") {
            q.distinct = true;
            sel = &sel[1..];
            if sel.first().is_some_and(|t| t.is("on")) && sel.get(1).is_some_and(|t| t.p("(")) {
                sel = &sel[close_of(sel, 1) + 1..];
            }
        } else if t.is("all") || t.is("sql_calc_found_rows") || t.is("high_priority") || t.is("straight_join") || t.is("json") {
            sel = &sel[1..];
        } else if t.is("top") {
            q.limit = sel.get(1).map(|t| t.t.clone());
            sel = &sel[2.min(sel.len())..];
        } else {
            break;
        }
    }
    q.items = parse_items(sel, &mut subs);
    for (n, (start, name)) in marks.iter().enumerate() {
        let end = marks.get(n + 1).map(|m| m.0).unwrap_or(toks.len());
        let skip = match name.as_str() {
            "group" | "order" => 2,
            "allow" | "per" | "for" => 2,
            _ => 1,
        };
        let body = &toks[(start + skip).min(end)..end];
        match name.as_str() {
            "from" => parse_from(body, q, &mut subs),
            "where" => {
                q.where_ = Some(render(body));
                q.where_refs = refs_in(body, &mut subs, "WHERE");
                q.where_pairs = eq_pairs(body);
                q.where_eq = eq_value_cols(body);
            }
            "group" => {
                q.group_by = split_top(body, ",").into_iter().map(render).collect();
                q.group_refs = refs_in(body, &mut subs, "GROUP BY");
            }
            "having" => {
                q.having = Some(render(body));
                q.having_refs = refs_in(body, &mut subs, "HAVING");
            }
            "order" => q.order_by = split_top(body, ",").into_iter().map(render).collect(),
            "limit" => {
                let parts = split_top(body, ",");
                if parts.len() == 2 {
                    q.offset = Some(render(parts[0]));
                    q.limit = Some(render(parts[1]));
                } else if let Some(o) = find_kw(body, "offset") {
                    q.limit = Some(render(&body[..o]));
                    q.offset = Some(render(&body[o + 1..]));
                } else {
                    q.limit = Some(render(body));
                }
            }
            "offset" => q.offset = Some(render(body).trim_end_matches(" ROWS").trim_end_matches(" rows").to_string()),
            "fetch" => q.limit = body.iter().find(|t| t.k == K::Num).map(|t| t.t.clone()).or(q.limit.take()),
            "allow" => q.extras.push("ALLOW FILTERING".into()),
            "per" => q.extras.push(format!("PER PARTITION LIMIT {}", render(body).trim_start_matches("LIMIT ").trim_start_matches("limit "))),
            "for" | "lock" => q.extras.push("locks the rows it reads".into()),
            "union" | "intersect" | "except" | "minus" => {}
            _ => {}
        }
    }
    q.subqueries = subs;
}

/// Parse SELECT (with CTEs and set operations), INSERT, UPDATE or DELETE.
fn parse_query(toks: &[Tok]) -> Option<Query> {
    let mut toks = toks;
    while toks.first().is_some_and(|t| t.p("(")) && close_of(toks, 0) + 1 == toks.len() {
        toks = group(toks, 0);
    }
    let first = toks.first()?;
    let mut q = Query::default();
    if first.is("with") {
        let mut i = 1;
        let recursive = toks.get(i).is_some_and(|t| t.is("recursive"));
        if recursive {
            i += 1;
        }
        loop {
            let (name, j) = qualified(toks, i)?;
            let mut j = j;
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
            let end = close_of(toks, j);
            let sub = parse_query(&toks[j + 1..end.min(toks.len())])?;
            q.ctes.push((name, sub, recursive));
            i = end + 1;
            if toks.get(i).is_some_and(|t| t.p(",")) {
                i += 1;
            } else {
                break;
            }
        }
        let mut main = parse_query(&toks[i..])?;
        main.ctes.splice(0..0, q.ctes);
        return Some(main);
    }
    if first.is("select") {
        q.verb = "SELECT".into();
        // Set operations split the statement into several selects.
        let mut depth = 0;
        let mut cuts = Vec::new();
        for (i, t) in toks.iter().enumerate() {
            if t.p("(") {
                depth += 1;
            } else if t.p(")") {
                depth -= 1;
            } else if depth == 0 && ["union", "intersect", "except", "minus"].iter().any(|k| t.is(k)) {
                cuts.push(i);
            }
        }
        let end = cuts.first().copied().unwrap_or(toks.len());
        parse_select(&toks[..end], &mut q);
        for (n, &c) in cuts.iter().enumerate() {
            let mut s = c + 1;
            let mut op = toks[c].t.to_ascii_uppercase();
            if toks.get(s).is_some_and(|t| t.is("all") || t.is("distinct")) {
                op = format!("{op} {}", toks[s].t.to_ascii_uppercase());
                s += 1;
            }
            let e = cuts.get(n + 1).copied().unwrap_or(toks.len());
            if let Some(part) = parse_query(&toks[s..e]) {
                q.set_ops.push((op, part));
            }
        }
        return Some(q);
    }
    if first.is("insert") || first.is("replace") || first.is("upsert") {
        q.verb = if first.is("replace") { "REPLACE".into() } else { "INSERT".into() };
        let into = toks.iter().position(|t| t.is("into")).map(|p| p + 1).unwrap_or(1);
        let (name, mut i) = qualified(toks, into)?;
        q.target = Some(name);
        if toks.get(i).is_some_and(|t| t.is("as")) {
            i += 2;
        }
        if toks.get(i).is_some_and(|t| t.p("(")) && !toks.get(i + 1).is_some_and(|t| t.is("select")) {
            q.insert_cols = name_list(group(toks, i));
            i = close_of(toks, i) + 1;
        }
        let rest = &toks[i.min(toks.len())..];
        if rest.first().is_some_and(|t| t.is("values") || t.is("value")) {
            let mut j = 1;
            while rest.get(j).is_some_and(|t| t.p("(")) {
                q.value_rows += 1;
                j = close_of(rest, j) + 1;
                if rest.get(j).is_some_and(|t| t.p(",")) {
                    j += 1;
                }
            }
            dml_tail(&rest[j..], &mut q);
        } else if rest.first().is_some_and(|t| t.is("set")) {
            // MySQL INSERT ... SET a = 1, b = 2
            let end = find_kw(rest, "on").unwrap_or(rest.len());
            parse_assignments(&rest[1..end], &mut q);
            q.insert_cols = q.assignments.iter().map(|a| a.0.clone()).collect();
            q.value_rows = 1;
            dml_tail(&rest[end..], &mut q);
        } else if rest.first().is_some_and(|t| t.is("json")) {
            q.value_rows = 1;
            q.extras.push("values come from a JSON document".into());
            dml_tail(&rest[2.min(rest.len())..], &mut q);
        } else if rest.first().is_some_and(|t| t.is("select") || t.is("with") || t.p("(")) {
            let end = find_kw(rest, "on").or_else(|| find_kw(rest, "returning")).unwrap_or(rest.len());
            q.insert_query = parse_query(&rest[..end]).map(Box::new);
            dml_tail(&rest[end..], &mut q);
        } else if rest.first().is_some_and(|t| t.is("default")) {
            q.value_rows = 1;
        }
        return Some(q);
    }
    if first.is("update") {
        q.verb = "UPDATE".into();
        let set = find_kw(toks, "set")?;
        let mut head = &toks[1..set];
        while head.first().is_some_and(|t| t.is("low_priority") || t.is("ignore") || t.is("only")) {
            head = &head[1..];
        }
        // CQL: UPDATE t USING TTL 10 SET ...
        let using = find_kw(head, "using");
        if let Some(u) = using {
            q.extras.push(render(&head[u..]));
            head = &head[..u];
        }
        let mut subs = Vec::new();
        parse_from(head, &mut q, &mut subs);
        q.target = q.sources.first().map(|s| s.name.clone());
        let rest = &toks[set + 1..];
        let marks = clause_marks(rest);
        let end = marks.first().map(|m| m.0).unwrap_or(rest.len());
        // CQL conditional updates: ... WHERE ... IF a = 1
        parse_assignments(&rest[..end], &mut q);
        for (n, (start, name)) in marks.iter().enumerate() {
            let stop = marks.get(n + 1).map(|m| m.0).unwrap_or(rest.len());
            let body = &rest[start + 1..stop];
            match name.as_str() {
                "from" => parse_from(body, &mut q, &mut subs),
                "where" => where_with_if(body, &mut q, &mut subs),
                "order" => q.order_by = split_top(&body[1.min(body.len())..], ",").into_iter().map(render).collect(),
                "limit" => q.limit = Some(render(body)),
                "returning" => q.returning = Some(render(body)),
                _ => {}
            }
        }
        q.subqueries.extend(subs);
        return Some(q);
    }
    if first.is("delete") {
        q.verb = "DELETE".into();
        let from = find_kw(toks, "from")?;
        // CQL: DELETE col1, col2 FROM t; MySQL: DELETE t1 FROM t1 JOIN ...
        let between = &toks[1..from];
        let between: Vec<Tok> = between.iter().filter(|t| !(t.is("low_priority") || t.is("quick") || t.is("ignore"))).cloned().collect();
        let mut subs = Vec::new();
        let rest = &toks[from + 1..];
        let marks = clause_marks(rest);
        let mut head_end = marks.first().map(|m| m.0).unwrap_or(rest.len());
        let using_at = find_kw(&rest[..head_end], "using");
        if let Some(u) = using_at {
            // PostgreSQL: DELETE FROM t USING other ...; CQL: USING TIMESTAMP
            if rest.get(u + 1).is_some_and(|t| t.is("timestamp")) {
                q.extras.push(render(&rest[u..head_end]));
            }
            head_end = u;
        }
        parse_from(&rest[..head_end], &mut q, &mut subs);
        if let Some(u) = using_at {
            if !rest.get(u + 1).is_some_and(|t| t.is("timestamp")) {
                let stop = marks.iter().find(|m| m.0 > u).map(|m| m.0).unwrap_or(rest.len());
                let mut extra = Query::default();
                parse_from(&rest[u + 1..stop], &mut extra, &mut subs);
                for mut s in extra.sources {
                    s.join.get_or_insert_with(|| ",".into());
                    q.sources.push(s);
                }
            }
        }
        q.target = q.sources.first().map(|s| s.name.clone());
        if !between.is_empty() {
            let names = name_list(&between);
            if names.len() == 1 && q.resolve(&names[0]).is_some() {
                q.target = q.resolve(&names[0]).map(|i| q.sources[i].name.clone());
            } else {
                q.delete_cols = split_top(&between, ",").into_iter().map(render).collect();
            }
        }
        for (n, (start, name)) in marks.iter().enumerate() {
            let stop = marks.get(n + 1).map(|m| m.0).unwrap_or(rest.len());
            let body = &rest[start + 1..stop];
            match name.as_str() {
                "where" => where_with_if(body, &mut q, &mut subs),
                "order" => q.order_by = split_top(&body[1.min(body.len())..], ",").into_iter().map(render).collect(),
                "limit" => q.limit = Some(render(body)),
                "returning" => q.returning = Some(render(body)),
                _ => {}
            }
        }
        q.subqueries.extend(subs);
        return Some(q);
    }
    None
}

/// WHERE, with a trailing CQL `IF ...` condition split off.
fn where_with_if(body: &[Tok], q: &mut Query, subs: &mut Vec<(String, Query)>) {
    let (cond, iff) = match find_kw(body, "if") {
        Some(i) => (&body[..i], Some(&body[i + 1..])),
        None => (body, None),
    };
    q.where_ = Some(render(cond));
    q.where_refs = refs_in(cond, subs, "WHERE");
    q.where_pairs = eq_pairs(cond);
    q.where_eq = eq_value_cols(cond);
    if let Some(c) = iff {
        let text = render(c);
        q.extras.push(if text.eq_ignore_ascii_case("exists") { "IF EXISTS".into() } else { format!("IF {text}") });
    }
}

fn parse_assignments(toks: &[Tok], q: &mut Query) {
    let mut subs = Vec::new();
    for part in split_top(toks, ",") {
        if let Some(eq) = part.iter().position(|t| t.p("=")) {
            let target = &part[..eq];
            let name = target.iter().rev().find(|t| t.ident()).map(|t| t.t.clone()).unwrap_or_default();
            let value = &part[eq + 1..];
            q.assignments.push((name, render(value), refs_in(value, &mut subs, "SET")));
        }
    }
    q.subqueries.extend(subs);
}

fn dml_tail(toks: &[Tok], q: &mut Query) {
    let mut i = 0;
    while i < toks.len() {
        let t = &toks[i];
        if t.is("on") && toks.get(i + 1).is_some_and(|n| n.is("duplicate")) {
            let end = find_kw(&toks[i..], "returning").map(|e| e + i).unwrap_or(toks.len());
            let mut tmp = Query::default();
            parse_assignments(&toks[(i + 4).min(end)..end], &mut tmp);
            q.upsert = Some(format!("if a row with the same key already exists, it updates {} instead", list_names(tmp.assignments.iter().map(|a| a.0.as_str()))));
            i = end;
            continue;
        }
        if t.is("on") && toks.get(i + 1).is_some_and(|n| n.is("conflict")) {
            let end = find_kw(&toks[i..], "returning").map(|e| e + i).unwrap_or(toks.len());
            let part = &toks[i..end];
            let target = part.iter().position(|t| t.p("(")).map(|o| name_list(group(part, o))).unwrap_or_default();
            let on = if target.is_empty() { String::new() } else { format!(" on {}", target.join(", ")) };
            q.upsert = Some(if part.iter().any(|t| t.is("nothing")) {
                format!("rows that clash with an existing one{on} are skipped")
            } else {
                let mut tmp = Query::default();
                if let Some(s) = find_kw(part, "set") {
                    let stop = find_kw(part, "where").unwrap_or(part.len());
                    parse_assignments(&part[s + 1..stop.max(s + 1)], &mut tmp);
                }
                format!("rows that clash with an existing one{on} update {} instead", list_names(tmp.assignments.iter().map(|a| a.0.as_str())))
            });
            i = end;
            continue;
        }
        if t.is("if") && toks.get(i + 1).is_some_and(|n| n.is("not")) {
            q.extras.push("IF NOT EXISTS".into());
            i += 3;
            continue;
        }
        if t.is("using") {
            let end = toks.iter().skip(i).position(|t| t.is("returning")).map(|p| p + i).unwrap_or(toks.len());
            q.extras.push(render(&toks[i..end]));
            i = end;
            continue;
        }
        if t.is("returning") {
            q.returning = Some(render(&toks[i + 1..]));
            break;
        }
        i += 1;
    }
}

fn list_names<'a>(names: impl Iterator<Item = &'a str>) -> String {
    let v: Vec<&str> = names.collect();
    match v.len() {
        0 => "the row".into(),
        1 => v[0].to_string(),
        n => format!("{} and {}", v[..n - 1].join(", "), v[n - 1]),
    }
}

fn human_seconds(s: &str) -> Option<String> {
    let n: u64 = s.parse().ok()?;
    let (v, unit) = match n {
        0 => return Some("never (0 disables expiry)".into()),
        n if n % 86400 == 0 => (n / 86400, "day"),
        n if n % 3600 == 0 => (n / 3600, "hour"),
        n if n % 60 == 0 => (n / 60, "minute"),
        n => (n, "second"),
    };
    Some(format!("{v} {unit}{}", if v == 1 { "" } else { "s" }))
}

// ---------------------------------------------------------------- walk-through

fn quote_list(v: &[String]) -> String {
    list_names(v.iter().map(|s| s.as_str()))
}

fn source_label(s: &Source) -> String {
    match (&s.sub, &s.alias) {
        (Some(_), Some(a)) => format!("the sub-query {a}"),
        (_, Some(a)) if !a.eq_ignore_ascii_case(&s.name) => format!("{} (as {a})", s.name),
        _ => s.name.clone(),
    }
}

fn explain(q: &Query, dialect: Dialect, schema: &Diagram, depth: usize, steps: &mut Vec<Step>, warnings: &mut Vec<String>) {
    let push = |steps: &mut Vec<Step>, clause: &str, text: String| steps.push(Step { clause: clause.into(), text, depth });
    for (name, cte, recursive) in &q.ctes {
        let how = if *recursive { "recursively (it keeps feeding its own rows back in until no new rows appear)" } else { "first, as a named temporary result" };
        push(steps, "WITH", format!("Builds {name} {how}:"));
        explain(cte, dialect, schema, depth + 1, steps, warnings);
    }
    match q.verb.as_str() {
        "SELECT" => explain_select(q, dialect, schema, depth, steps, warnings),
        "INSERT" | "REPLACE" => {
            let target = q.target.clone().unwrap_or_default();
            let cols = if q.insert_cols.is_empty() { "every column, in table order".to_string() } else { quote_list(&q.insert_cols) };
            if let Some(src) = &q.insert_query {
                push(steps, "INSERT", format!("Adds the rows produced by the query below to {target}, filling {cols}."));
                explain(src, dialect, schema, depth + 1, steps, warnings);
            } else {
                let rows = if q.value_rows == 1 { "one row".to_string() } else { format!("{} rows", q.value_rows) };
                push(steps, &q.verb, format!("Adds {rows} to {target}, setting {cols}."));
            }
            if q.verb == "REPLACE" {
                push(steps, "REPLACE", "A row with the same primary or unique key is deleted first, then the new row is inserted.".into());
            }
            if dialect == Dialect::Cassandra && !q.extras.iter().any(|e| e == "IF NOT EXISTS") {
                push(steps, "UPSERT", format!("In Cassandra an INSERT is an upsert: if {target} already has a row with the same primary key, its columns are overwritten."));
            }
            if let Some(u) = &q.upsert {
                push(steps, "ON CONFLICT", format!("On a duplicate key, {u}."));
            }
        }
        "UPDATE" => {
            let target = q.target.clone().unwrap_or_default();
            if q.sources.len() > 1 {
                describe_sources(q, schema, depth, steps);
            }
            let sets: Vec<String> = q.assignments.iter().map(|(c, v, _)| format!("{c} to {v}")).collect();
            let scope = match &q.where_ {
                Some(w) => format!("for rows where {w}"),
                None => "for every row".into(),
            };
            push(steps, "UPDATE", format!("In {target}, sets {} {scope}.", quote_list(&sets)));
            if q.where_.is_none() {
                warnings.push(format!("UPDATE without WHERE changes every row in {target}."));
            }
            if dialect == Dialect::Cassandra {
                push(steps, "UPSERT", "In Cassandra an UPDATE is an upsert: if no row matches the key, one is created.".into());
            }
        }
        "DELETE" => {
            let target = q.target.clone().unwrap_or_default();
            if q.sources.len() > 1 {
                describe_sources(q, schema, depth, steps);
            }
            let what = if q.delete_cols.is_empty() { "Removes rows".to_string() } else { format!("Clears {} (the rows stay)", quote_list(&q.delete_cols)) };
            match &q.where_ {
                Some(w) => push(steps, "DELETE", format!("{what} from {target} where {w}.")),
                None => {
                    push(steps, "DELETE", format!("{what} from every row of {target}."));
                    warnings.push(format!("DELETE without WHERE removes everything in {target}."));
                }
            }
        }
        _ => {}
    }
    if q.verb != "SELECT" {
        if let Some(l) = &q.limit {
            push(steps, "LIMIT", format!("Stops after {l} rows."));
        }
        for (ctx, sub) in &q.subqueries {
            push(steps, "SUB-QUERY", format!("Runs a sub-query used in {ctx}:"));
            explain(sub, dialect, schema, depth + 1, steps, warnings);
        }
        if q.where_.is_some() && dialect == Dialect::Cassandra {
            if let Some(t) = &q.target {
                check_partition_key(q, t, schema, warnings);
            }
        }
    }
    for e in &q.extras {
        let upper = e.to_ascii_uppercase();
        let clause = if upper.starts_with("USING TTL") {
            "USING TTL"
        } else if upper.starts_with("USING TIMESTAMP") {
            "USING TIMESTAMP"
        } else if upper == "IF NOT EXISTS" {
            "IF NOT EXISTS"
        } else if upper.starts_with("IF ") {
            "IF"
        } else if upper.starts_with("PER PARTITION") {
            "PER PARTITION"
        } else {
            "OPTION"
        };
        let text = if upper.starts_with("USING TTL") {
            let secs = e.split_whitespace().nth(2).unwrap_or("");
            format!("Written values expire after {}.", human_seconds(secs).unwrap_or_else(|| format!("{secs} seconds")))
        } else if upper.starts_with("USING TIMESTAMP") {
            "Uses the given write timestamp instead of the current time; the newest timestamp wins on conflict.".into()
        } else if upper == "IF NOT EXISTS" {
            "Only writes if the row does not exist yet. This is a lightweight transaction (Paxos): about four times slower than a plain write.".into()
        } else if upper.starts_with("IF ") {
            format!("Only applies when {} holds, checked atomically with a lightweight transaction (slower than a plain write).", &e[3..])
        } else if upper == "ALLOW FILTERING" {
            continue;
        } else if upper.starts_with("PER PARTITION LIMIT") {
            format!("Takes at most {} rows from each partition.", e.split_whitespace().last().unwrap_or(""))
        } else {
            let mut c = e.clone();
            if let Some(f) = c.get_mut(0..1) {
                f.make_ascii_uppercase();
            }
            format!("{c}.")
        };
        push(steps, clause, text);
    }
    if let Some(r) = &q.returning {
        push(steps, "RETURNING", format!("Returns {r} from the affected rows."));
    }
}

fn describe_sources(q: &Query, schema: &Diagram, depth: usize, steps: &mut Vec<Step>) {
    for (n, s) in q.sources.iter().enumerate() {
        let label = source_label(s);
        let (clause, text) = match s.join.as_deref() {
            None => ("FROM".to_string(), format!("Reads the rows of {label}.")),
            Some(",") => {
                let linked = q.where_pairs.iter().any(|(a, b)| {
                    let (sa, sb) = (q.source_of(a, schema), q.source_of(b, schema));
                    (sa == Some(n) && sb.is_some_and(|x| x < n)) || (sb == Some(n) && sa.is_some_and(|x| x < n))
                });
                if linked {
                    (", (JOIN)".to_string(), format!("Adds {label}; the WHERE condition that matches it to the earlier tables makes this an inner join."))
                } else {
                    ("CROSS JOIN".to_string(), format!("Pairs every row so far with every row of {label} (a Cartesian product)."))
                }
            }
            Some(j) => {
                let cond = match (&s.on, s.using.is_empty()) {
                    (Some(on), _) => format!(" where {on}"),
                    (None, false) => format!(" on matching {}", quote_list(&s.using)),
                    _ => String::new(),
                };
                let effect = if j.contains("LEFT") {
                    format!("keeps every row from the left side; {}'s columns are NULL where nothing matches", s.alias.clone().unwrap_or(s.name.clone()))
                } else if j.contains("RIGHT") {
                    format!("keeps every row of {label}, even when nothing on the left matches")
                } else if j.contains("FULL") {
                    "keeps unmatched rows from both sides, filling the gaps with NULL".into()
                } else if j.contains("CROSS") {
                    "pairs every row with every row (a Cartesian product)".into()
                } else if j.contains("NATURAL") {
                    "matches on every column the two sides share by name".into()
                } else {
                    "keeps only rows that have a match on both sides".into()
                };
                (j.to_string(), format!("Joins {label}{cond}: {effect}."))
            }
        };
        steps.push(Step { clause, text, depth });
        if let Some(sub) = &s.sub {
            let mut w = Vec::new();
            explain(sub, Dialect::Generic, schema, depth + 1, steps, &mut w);
        }
    }
}

fn explain_select(q: &Query, dialect: Dialect, schema: &Diagram, depth: usize, steps: &mut Vec<Step>, warnings: &mut Vec<String>) {
    let push = |steps: &mut Vec<Step>, clause: &str, text: String| steps.push(Step { clause: clause.into(), text, depth });
    if q.sources.is_empty() {
        push(steps, "SELECT", format!("Computes {} without reading any table.", quote_list(&q.items.iter().map(|i| i.expr.clone()).collect::<Vec<_>>())));
    } else {
        describe_sources(q, schema, depth, steps);
    }
    for (n, s) in q.sources.iter().enumerate() {
        if s.join.as_deref() == Some(",") || s.join.as_deref().is_some_and(|j| j.contains("CROSS")) {
            let linked = q.where_pairs.iter().any(|(a, b)| q.source_of(a, schema) == Some(n) || q.source_of(b, schema) == Some(n));
            if !linked {
                warnings.push(format!("{} is joined without a matching condition, so every row pairs with every other row; results grow multiplicatively.", s.name));
            }
        }
    }
    if let Some(w) = &q.where_ {
        push(steps, "WHERE", format!("Keeps only rows where {w}."));
        if q.where_.as_deref().is_some_and(|w| w.contains("LIKE '%") || w.contains("like '%")) {
            warnings.push("A LIKE pattern that starts with % cannot use an index; every row is checked.".into());
        }
    }
    for (ctx, sub) in &q.subqueries {
        push(steps, "SUB-QUERY", format!("For the {ctx} clause, runs this sub-query:"));
        explain(sub, dialect, schema, depth + 1, steps, warnings);
    }
    let has_agg = q.items.iter().any(|i| i.agg);
    if !q.group_by.is_empty() {
        push(steps, "GROUP BY", format!("Groups rows that share the same {}; each group becomes one output row.", quote_list(&q.group_by)));
    } else if has_agg && !q.items.iter().any(|i| i.window) {
        push(steps, "AGGREGATE", "Collapses all remaining rows into a single summary row.".into());
    }
    if let Some(h) = &q.having {
        push(steps, "HAVING", format!("Keeps only groups where {h}."));
    }
    if !q.items.is_empty() {
        let shown: Vec<String> = q
            .items
            .iter()
            .map(|i| if i.star { match &i.star_of { Some(t) => format!("every column of {t}"), None => "every column".into() } } else if i.expr == i.name || i.expr.ends_with(&format!(".{}", i.name)) { i.expr.clone() } else { format!("{} as {}", i.expr, i.name) })
            .collect();
        let n = q.items.len();
        let mut text = if n == 1 && q.items[0].star {
            format!("Returns {}.", shown[0])
        } else {
            format!("Returns {}: {}.", if n == 1 { "one column".to_string() } else { format!("{n} columns") }, shown.join(", "))
        };
        if q.items.iter().any(|i| i.window) {
            text.push_str(" Window functions (OVER) compute across related rows without merging them.");
        }
        push(steps, "SELECT", text);
        if q.items.iter().any(|i| i.star && i.star_of.is_none()) && !q.sources.is_empty() {
            warnings.push("SELECT * returns every column; listing only the ones you need is faster and survives schema changes.".into());
        }
    }
    if q.distinct {
        push(steps, "DISTINCT", "Removes duplicate rows from the result.".into());
    }
    for (op, part) in &q.set_ops {
        let how = match op.as_str() {
            "UNION" => "adds the rows of the next query, dropping duplicates",
            "UNION ALL" => "appends the rows of the next query, keeping duplicates",
            o if o.starts_with("INTERSECT") => "keeps only rows the next query also returns",
            _ => "removes rows the next query returns",
        };
        push(steps, op, format!("Then {how}:"));
        explain(part, dialect, schema, depth + 1, steps, warnings);
    }
    if !q.order_by.is_empty() {
        let parts: Vec<String> = q
            .order_by
            .iter()
            .map(|o| {
                let l = o.to_ascii_lowercase();
                if l.ends_with(" desc") {
                    format!("{} (highest first)", &o[..o.len() - 5])
                } else if l.ends_with(" asc") {
                    format!("{} (lowest first)", &o[..o.len() - 4])
                } else {
                    format!("{o} (lowest first)")
                }
            })
            .collect();
        push(steps, "ORDER BY", format!("Sorts by {}.", parts.join(", then ")));
    }
    match (&q.limit, &q.offset) {
        (Some(l), Some(o)) => push(steps, "LIMIT", format!("Skips the first {o} rows and returns the next {l}.")),
        (Some(l), None) => push(steps, "LIMIT", format!("Returns at most {l} rows.")),
        (None, Some(o)) => push(steps, "OFFSET", format!("Skips the first {o} rows.")),
        _ => {}
    }
    if q.limit.is_some() && q.order_by.is_empty() && dialect != Dialect::Cassandra && depth == 0 {
        warnings.push("LIMIT without ORDER BY returns whichever rows the database finds first; the choice can change between runs.".into());
    }
    if dialect == Dialect::Cassandra && !q.sources.is_empty() {
        let allow = q.extras.iter().any(|e| e == "ALLOW FILTERING");
        if allow {
            push(steps, "ALLOW FILTERING", "Lets Cassandra read partitions it cannot target by key and filter them in memory.".into());
            warnings.push("ALLOW FILTERING scans data across partitions; latency grows with table size. Prefer a table keyed by the columns you filter on.".into());
        }
        let t = q.sources[0].name.clone();
        check_partition_key(q, &t, schema, warnings);
        if q.sources.len() > 1 {
            warnings.push("Cassandra does not support joins; this query will be rejected.".into());
        }
        if !q.group_by.is_empty() {
            if let Some(e) = schema.find(&t) {
                let keys: Vec<&str> = schema.entities[e].columns.iter().filter(|c| matches!(c.key, Key::Partition | Key::Clustering)).map(|c| c.name.as_str()).collect();
                if q.group_by.iter().any(|g| !keys.iter().any(|k| k.eq_ignore_ascii_case(g))) {
                    warnings.push("Cassandra can only GROUP BY primary key columns, in key order.".into());
                }
            }
        }
    }
}

/// Cassandra needs every partition key column restricted with = or IN.
fn check_partition_key(q: &Query, table: &str, schema: &Diagram, warnings: &mut Vec<String>) {
    let Some(e) = schema.find(table) else { return };
    let ent = &schema.entities[e];
    let pk: Vec<&str> = ent.columns.iter().filter(|c| c.key == Key::Partition).map(|c| c.name.as_str()).collect();
    if pk.is_empty() {
        return;
    }
    let restricted = |c: &str| q.where_eq.iter().any(|r| r.col.eq_ignore_ascii_case(c));
    let missing: Vec<String> = pk.iter().filter(|c| !restricted(c)).map(|c| c.to_string()).collect();
    let allow = q.extras.iter().any(|e| e == "ALLOW FILTERING");
    let token = q.where_.as_deref().is_some_and(|w| w.to_ascii_lowercase().contains("token("));
    if missing.is_empty() || token {
        // Non-key filters need an index or ALLOW FILTERING.
        let filters: Vec<String> = q
            .where_refs
            .iter()
            .filter(|r| ent.col(&r.col).is_some_and(|c| ent.columns[c].key == Key::None && !ent.columns[c].indexed))
            .map(|r| r.col.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        if !filters.is_empty() && !allow && q.verb == "SELECT" {
            warnings.push(format!("Filtering on {} (not part of the primary key, and not indexed) needs ALLOW FILTERING or a secondary index; Cassandra rejects it as written.", quote_list(&filters)));
        }
        return;
    }
    if q.verb == "SELECT" {
        if q.where_.is_none() {
            warnings.push(format!("No WHERE clause: this reads every partition of {table} (a full table scan across the cluster)."));
        } else if !allow {
            warnings.push(format!("The WHERE clause does not restrict the full partition key ({}), so Cassandra will reject it without ALLOW FILTERING. Missing: {}.", pk.join(", "), missing.join(", ")));
        }
    } else {
        warnings.push(format!("Cassandra writes need the full primary key in WHERE; the partition key ({}) is not fully given. Missing: {}.", pk.join(", "), missing.join(", ")));
    }
}

fn summarize(q: &Query) -> String {
    let tables: Vec<String> = q.sources.iter().map(|s| s.name.clone()).collect();
    match q.verb.as_str() {
        "SELECT" => {
            let mut s = if tables.is_empty() {
                "Computes a single row of values".to_string()
            } else {
                format!("Reads {}", quote_list(&tables))
            };
            if q.sources.len() > 1 {
                s.push_str(&format!(" with {} join{}", q.sources.len() - 1, if q.sources.len() > 2 { "s" } else { "" }));
            }
            if q.where_.is_some() {
                s.push_str(", filters rows");
            }
            if !q.group_by.is_empty() {
                s.push_str(", groups them");
            }
            if !q.order_by.is_empty() {
                s.push_str(", sorts");
            }
            let n = q.items.len();
            let cols = if q.items.iter().any(|i| i.star) { "every column".to_string() } else { format!("{n} column{}", if n == 1 { "" } else { "s" }) };
            match &q.limit {
                Some(l) => s.push_str(&format!(" and returns up to {l} rows of {cols}")),
                None => s.push_str(&format!(" and returns {cols}")),
            }
            if !q.set_ops.is_empty() {
                s.push_str(&format!(", combined with {} more quer{}", q.set_ops.len(), if q.set_ops.len() == 1 { "y" } else { "ies" }));
            }
            s + "."
        }
        "INSERT" | "REPLACE" => format!("Writes new rows into {}.", q.target.clone().unwrap_or_default()),
        "UPDATE" => format!("Changes {} in {}{}.", quote_list(&q.assignments.iter().map(|a| a.0.clone()).collect::<Vec<_>>()), q.target.clone().unwrap_or_default(), if q.where_.is_some() { " for matching rows" } else { " for every row" }),
        "DELETE" => format!("Deletes {} {}.", if q.delete_cols.is_empty() { "rows from" } else { "values in" }, q.target.clone().unwrap_or_default()),
        _ => String::new(),
    }
}

// ---------------------------------------------------------------- query diagram

struct QBuild<'a> {
    schema: &'a Diagram,
    d: Diagram,
    /// Entities by lowercase name, so a table used twice appears once.
    names: HashMap<String, usize>,
}

impl QBuild<'_> {
    /// An entity for a table, copied from the schema when known.
    fn table(&mut self, name: &str, caption: Option<String>) -> usize {
        let key = name.to_ascii_lowercase();
        if let Some(&i) = self.names.get(&key) {
            if let Some(c) = caption {
                let e = &mut self.d.entities[i];
                if e.caption.as_deref() != Some(c.as_str()) {
                    e.caption = Some(match &e.caption {
                        Some(old) => format!("{old}, {c}"),
                        None => c,
                    });
                }
            }
            return i;
        }
        let mut e = match self.schema.find(name) {
            Some(s) => {
                let mut e = self.schema.entities[s].clone();
                e.caption = None;
                e
            }
            None => Entity::new(name, EntityKind::Table),
        };
        e.caption = caption;
        self.d.entities.push(e);
        let i = self.d.entities.len() - 1;
        self.names.insert(key, i);
        i
    }

    /// Mark (and if unknown, add) a column; returns its index.
    fn touch(&mut self, e: usize, col: &str, filtered: bool) -> Option<usize> {
        let ent = &mut self.d.entities[e];
        let c = match ent.col(col) {
            Some(c) => c,
            None => {
                // Only invent columns for tables we have no schema for.
                if self.schema.find(&ent.name).is_some() && ent.kind == EntityKind::Table {
                    return None;
                }
                ent.columns.push(Column::new(col, ""));
                ent.columns.len() - 1
            }
        };
        ent.columns[c].used = true;
        ent.columns[c].filtered |= filtered;
        Some(c)
    }

    /// Lay out a query's sources into the diagram, returning the entity index
    /// for each of its sources.
    fn sources(&mut self, q: &Query, ctes: &HashMap<String, usize>) -> Vec<usize> {
        let mut idx = Vec::new();
        for s in &q.sources {
            let i = if let Some(sub) = &s.sub {
                let node = self.result_node(sub, ctes, s.alias.clone().unwrap_or_else(|| "sub-query".into()), EntityKind::Derived);
                self.d.entities[node].caption = Some("sub-query".into());
                node
            } else if let Some(&c) = ctes.get(&s.name.to_ascii_lowercase()) {
                c
            } else {
                let cap = s.alias.clone().filter(|a| !a.eq_ignore_ascii_case(&s.name)).map(|a| format!("as {a}"));
                self.table(&s.name, cap)
            };
            idx.push(i);
        }
        idx
    }

    fn resolve(&mut self, q: &Query, idx: &[usize], r: &ColRef, filtered: bool) -> Option<(usize, usize)> {
        let s = q.source_of(r, self.schema).or_else(|| {
            // Bare column over several unknown tables: pick the one already holding it.
            idx.iter().position(|&e| self.d.entities[e].col(&r.col).is_some())
        })?;
        let e = *idx.get(s)?;
        let c = self.touch(e, &r.col, filtered)?;
        Some((e, c))
    }

    fn mark(&mut self, q: &Query, idx: &[usize], refs: &[ColRef], filtered: bool) {
        for r in refs {
            self.resolve(q, idx, r, filtered);
        }
    }

    fn joins(&mut self, q: &Query, idx: &[usize]) {
        for (n, s) in q.sources.iter().enumerate() {
            let Some(j) = &s.join else { continue };
            let mut linked = false;
            let pairs: Vec<(ColRef, ColRef)> = if j == "," { q.where_pairs.clone() } else { s.pairs.clone() };
            let label = if j == "," { "JOIN (WHERE)".to_string() } else { j.clone() };
            for (a, b) in &pairs {
                let (Some((ea, ca)), Some((eb, cb))) = (self.resolve(q, idx, a, true), self.resolve(q, idx, b, true)) else { continue };
                if ea == eb {
                    continue;
                }
                // Draw from the earlier table to the joined one.
                let (f, t) = if ea == idx[n] { ((eb, cb), (ea, ca)) } else { ((ea, ca), (eb, cb)) };
                if j != "," || f.0 == idx[n] || t.0 == idx[n] {
                    self.d.add_link(Link { from: f.0, from_col: Some(f.1), to: t.0, to_col: Some(t.1), kind: LinkKind::Join(label.clone()) });
                    linked = true;
                }
            }
            for u in &s.using {
                if let (Some(prev), Some(cur)) = (n.checked_sub(1).map(|p| idx[p]), idx.get(n)) {
                    let a = self.touch(prev, u, true);
                    let b = self.touch(*cur, u, true);
                    self.d.add_link(Link { from: prev, from_col: a, to: *cur, to_col: b, kind: LinkKind::Join(label.clone()) });
                    linked = true;
                }
            }
            if !linked && n > 0 {
                self.d.add_link(Link { from: idx[0], from_col: None, to: idx[n], to_col: None, kind: LinkKind::Join(label.clone()) });
            }
        }
    }

    /// Build the nodes for a SELECT and a node holding its output; returns that node.
    fn result_node(&mut self, q: &Query, outer: &HashMap<String, usize>, name: String, kind: EntityKind) -> usize {
        let mut ctes = outer.clone();
        for (cname, cte, _) in &q.ctes {
            let node = self.result_node(cte, &ctes, cname.clone(), EntityKind::Cte);
            self.d.entities[node].caption = Some("WITH".into());
            ctes.insert(cname.to_ascii_lowercase(), node);
        }
        let idx = self.sources(q, &ctes);
        self.joins(q, &idx);
        for s in &q.sources {
            self.mark(q, &idx, &s.on_refs, true);
        }
        self.mark(q, &idx, &q.where_refs, true);
        self.mark(q, &idx, &q.group_refs, false);
        self.mark(q, &idx, &q.having_refs, true);
        let mut out = Entity::new(name, kind);
        let mut flows = Vec::new();
        for (n, item) in q.items.iter().enumerate() {
            let ty = if item.star {
                String::new()
            } else if item.agg {
                "aggregate".into()
            } else if item.window {
                "window".into()
            } else if item.refs.len() == 1 && item.expr.ends_with(&item.refs[0].col) {
                String::new()
            } else if item.refs.is_empty() {
                "value".into()
            } else {
                "expression".into()
            };
            out.columns.push(Column { used: true, ..Column::new(item.name.clone(), ty) });
            if item.star {
                let targets: Vec<usize> = match &item.star_of {
                    Some(t) => q.resolve(t).map(|s| vec![idx[s]]).unwrap_or_default(),
                    None => idx.clone(),
                };
                for t in targets {
                    for c in &mut self.d.entities[t].columns {
                        c.used = true;
                    }
                    flows.push((t, None, n));
                }
            }
            for r in &item.refs {
                if let Some((e, c)) = self.resolve(q, &idx, r, false) {
                    flows.push((e, Some(c), n));
                }
            }
        }
        for (_, sub) in &q.subqueries {
            let node = self.result_node(sub, &ctes, "sub-query".into(), EntityKind::Derived);
            self.d.entities[node].caption = Some("filter".into());
            flows.push((node, None, usize::MAX));
        }
        let mut set_nodes = Vec::new();
        for (op, part) in &q.set_ops {
            let node = self.result_node(part, &ctes, op.clone(), EntityKind::Derived);
            set_nodes.push(node);
        }
        self.d.entities.push(out);
        let me = self.d.entities.len() - 1;
        for (e, c, n) in flows {
            let to_col = (n != usize::MAX).then_some(n);
            self.d.add_link(Link { from: e, from_col: c, to: me, to_col, kind: LinkKind::Flow });
        }
        for s in set_nodes {
            self.d.add_link(Link { from: s, from_col: None, to: me, to_col: None, kind: LinkKind::Flow });
        }
        me
    }

    fn dml(&mut self, q: &Query) {
        let mut ctes = HashMap::new();
        for (cname, cte, _) in &q.ctes {
            let node = self.result_node(cte, &ctes, cname.clone(), EntityKind::Cte);
            self.d.entities[node].caption = Some("WITH".into());
            ctes.insert(cname.to_ascii_lowercase(), node);
        }
        let target_name = q.target.clone().unwrap_or_default();
        let idx = if q.sources.is_empty() { vec![self.table(&target_name, None)] } else { self.sources(q, &ctes) };
        let target = idx[0];
        self.joins(q, &idx);
        self.mark(q, &idx, &q.where_refs, true);
        let mut op = Entity::new(q.verb.clone(), EntityKind::Result);
        op.caption = Some(format!("into {target_name}"));
        let mut writes = Vec::new();
        match q.verb.as_str() {
            "INSERT" | "REPLACE" => {
                let cols: Vec<String> = if q.insert_cols.is_empty() {
                    self.d.entities[target].columns.iter().map(|c| c.name.clone()).collect()
                } else {
                    q.insert_cols.clone()
                };
                for (n, c) in cols.iter().enumerate() {
                    let val = q.assignments.iter().find(|a| a.0.eq_ignore_ascii_case(c)).map(|a| a.1.clone());
                    op.columns.push(Column { used: true, ..Column::new(c.clone(), val.unwrap_or_else(|| "value".into())) });
                    if let Some(tc) = self.touch(target, c, false) {
                        writes.push((n, tc));
                    }
                }
            }
            "UPDATE" => {
                op.caption = Some(format!("set on {target_name}"));
                for (n, (c, v, refs)) in q.assignments.iter().enumerate() {
                    op.columns.push(Column { used: true, ..Column::new(c.clone(), shorten(v, 22)) });
                    if let Some(tc) = self.touch(target, c, false) {
                        writes.push((n, tc));
                    }
                    self.mark(q, &idx, refs, false);
                }
            }
            _ => {
                op.caption = Some(format!("from {target_name}"));
                if q.delete_cols.is_empty() {
                    op.columns.push(Column { used: true, ..Column::new("whole rows", "") });
                } else {
                    for (n, c) in q.delete_cols.iter().enumerate() {
                        op.columns.push(Column { used: true, ..Column::new(c.clone(), "cleared") });
                        if let Some(tc) = self.touch(target, c, false) {
                            writes.push((n, tc));
                        }
                    }
                }
            }
        }
        self.d.entities.push(op);
        let me = self.d.entities.len() - 1;
        if let Some(src) = &q.insert_query {
            let node = self.result_node(src, &ctes, "SELECT".into(), EntityKind::Derived);
            self.d.entities[node].caption = Some("rows to insert".into());
            self.d.add_link(Link { from: node, from_col: None, to: me, to_col: None, kind: LinkKind::Flow });
        }
        for (_, sub) in &q.subqueries {
            let node = self.result_node(sub, &ctes, "sub-query".into(), EntityKind::Derived);
            self.d.entities[node].caption = Some("filter".into());
            self.d.add_link(Link { from: node, from_col: None, to: target, to_col: None, kind: LinkKind::Flow });
        }
        for &e in &idx[1..] {
            if !self.d.links.iter().any(|l| l.to == target && l.from == e || l.from == target && l.to == e) {
                self.d.add_link(Link { from: e, from_col: None, to: target, to_col: None, kind: LinkKind::Flow });
            }
        }
        if writes.is_empty() {
            self.d.add_link(Link { from: me, from_col: None, to: target, to_col: None, kind: LinkKind::Flow });
        }
        for (n, tc) in writes {
            self.d.add_link(Link { from: me, from_col: Some(n), to: target, to_col: Some(tc), kind: LinkKind::Flow });
        }
    }
}

fn shorten(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n - 1).collect::<String>())
    }
}

fn query_diagram(q: &Query, schema: &Diagram) -> Diagram {
    let mut b = QBuild { schema, d: Diagram::default(), names: HashMap::new() };
    if q.verb == "SELECT" {
        b.result_node(q, &HashMap::new(), "Result".into(), EntityKind::Result);
    } else {
        b.dml(q);
    }
    // Wide schema tables: keep keys and touched columns, fold the rest.
    for e in &mut b.d.entities {
        if e.kind == EntityKind::Table && e.columns.len() > 14 && e.columns.iter().any(|c| c.used) {
            let hidden = e.columns.iter().filter(|c| !c.used && c.key == Key::None).count();
            if hidden > 3 {
                let keep: Vec<bool> = e.columns.iter().map(|c| c.used || c.key != Key::None).collect();
                let mut remap = Vec::new();
                let mut n = 0;
                for k in &keep {
                    remap.push(if *k { Some(n) } else { None });
                    if *k {
                        n += 1;
                    }
                }
                let ei = b.names.get(&e.name.to_ascii_lowercase()).copied();
                e.columns = e.columns.iter().zip(&keep).filter(|(_, k)| **k).map(|(c, _)| c.clone()).collect();
                e.columns.push(Column::new(format!("… {hidden} more"), ""));
                if let Some(ei) = ei {
                    for l in &mut b.d.links {
                        if l.from == ei {
                            l.from_col = l.from_col.and_then(|c| remap[c]);
                        }
                        if l.to == ei {
                            l.to_col = l.to_col.and_then(|c| remap[c]);
                        }
                    }
                }
            }
        }
    }
    b.d
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
    let mut queries_toks = Vec::new();
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
            "select" | "with" | "insert" | "update" | "delete" | "replace" | "upsert" => queries_toks.push(*s),
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
    let mut queries = Vec::new();
    for s in queries_toks {
        let Some(q) = parse_query(s) else {
            skipped.push(shorten(&render(&s[..s.len().min(12)]), 60));
            continue;
        };
        let mut steps = Vec::new();
        let mut warnings = Vec::new();
        explain(&q, dialect, &schema, 0, &mut steps, &mut warnings);
        let mut seen = HashSet::new();
        warnings.retain(|w| seen.insert(w.clone()));
        queries.push(QueryView {
            preview: shorten(&render(s), 64),
            summary: summarize(&q),
            steps,
            warnings,
            diagram: query_diagram(&q, &schema),
        });
    }
    Analysis { dialect, schema, relations, queries, skipped }
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
            LinkKind::Join(j) => ("}o--o{", j.clone()),
        };
        out.push_str(&format!("    {} {rel} {} : \"{}\"\n", clean(&f.name), clean(&t.name), label.replace('"', "'")));
    }
    out
}

/// A query walk-through as plain text.
pub fn steps_text(q: &QueryView) -> String {
    let mut out = format!("{}\n\n", q.summary);
    for (n, s) in q.steps.iter().enumerate() {
        out.push_str(&format!("{}{}. {} — {}\n", "   ".repeat(s.depth), n + 1, s.clause, s.text));
    }
    if !q.warnings.is_empty() {
        out.push_str("\nWatch out:\n");
        for w in &q.warnings {
            out.push_str(&format!("- {w}\n"));
        }
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
    fn cassandra_query_warnings() {
        let src = format!("{CQL} SELECT * FROM shop.orders_by_user WHERE total > 100;");
        let a = analyze(&src, false);
        let q = &a.queries[0];
        assert!(q.warnings.iter().any(|w| w.contains("partition key")), "{:?}", q.warnings);
        let ok = format!("{CQL} SELECT order_id, total FROM orders_by_user WHERE user_id = ? AND order_date > '2026-01-01' LIMIT 10;");
        let a = analyze(&ok, false);
        assert!(a.queries[0].warnings.is_empty(), "{:?}", a.queries[0].warnings);
        let ins = format!("{CQL} INSERT INTO users (user_id, email) VALUES (uuid(), 'a@b.c') USING TTL 86400;");
        let a = analyze(&ins, false);
        let steps: Vec<&str> = a.queries[0].steps.iter().map(|s| s.text.as_str()).collect();
        assert!(steps.iter().any(|s| s.contains("upsert")), "{steps:?}");
        assert!(steps.iter().any(|s| s.contains("1 day")), "{steps:?}");
    }

    #[test]
    fn select_walkthrough() {
        let src = format!("{SHOP}
select u.id, u.email, count(o.id) as orders from users u left join orders o on o.user_id = u.id
where u.created_at > '2026-01-01' group by u.id, u.email having count(o.id) > 3 order by orders desc limit 20;");
        let a = analyze(&src, false);
        let q = &a.queries[0];
        let clauses: Vec<&str> = q.steps.iter().map(|s| s.clause.as_str()).collect();
        assert_eq!(clauses, ["FROM", "LEFT JOIN", "WHERE", "GROUP BY", "HAVING", "SELECT", "ORDER BY", "LIMIT"]);
        assert!(q.steps[1].text.contains("o.user_id = u.id"));
        assert!(q.steps[6].text.contains("orders (highest first)"));
        // Diagram: users, orders, Result; one join link and flows into the result.
        let d = &q.diagram;
        assert_eq!(d.entities.len(), 3);
        let join = d.links.iter().find(|l| matches!(l.kind, LinkKind::Join(_))).unwrap();
        assert_eq!(d.entities[join.from].name, "users");
        assert_eq!(d.entities[join.to].name, "orders");
        let users = ent(d, "users");
        assert!(users.columns[2].filtered, "created_at is filtered");
        let result = ent(d, "Result");
        assert_eq!(result.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["id", "email", "orders"]);
        assert_eq!(result.columns[2].ty, "aggregate");
    }

    #[test]
    fn ctes_subqueries_and_unions() {
        let src = "WITH big AS (SELECT user_id, SUM(total_cents) t FROM orders GROUP BY user_id)
                   SELECT u.email FROM users u JOIN big b ON b.user_id = u.id
                   WHERE u.id IN (SELECT user_id FROM reviews) UNION ALL SELECT email FROM admins;";
        let a = analyze(src, false);
        let q = &a.queries[0];
        let clauses: Vec<&str> = q.steps.iter().map(|s| s.clause.as_str()).collect();
        assert_eq!(clauses[0], "WITH");
        assert!(clauses.contains(&"SUB-QUERY"));
        assert!(clauses.contains(&"UNION ALL"));
        let names: Vec<&str> = q.diagram.entities.iter().map(|e| e.name.as_str()).collect();
        for n in ["orders", "big", "users", "reviews", "admins", "Result"] {
            assert!(names.contains(&n), "{n} missing from {names:?}");
        }
        assert_eq!(names.iter().filter(|n| **n == "big").count(), 1);
    }

    #[test]
    fn dml_statements() {
        let a = analyze(&format!("{SHOP} UPDATE orders SET total_cents = total_cents * 2;"), false);
        assert!(a.queries[0].warnings.iter().any(|w| w.contains("every row")));
        let a = analyze("INSERT INTO t (a, b) VALUES (1, 2), (3, 4) ON DUPLICATE KEY UPDATE b = VALUES(b);", false);
        let q = &a.queries[0];
        assert!(q.steps[0].text.starts_with("Adds 2 rows to t"), "{:?}", q.steps);
        assert!(q.steps.iter().any(|s| s.clause == "ON CONFLICT" && s.text.contains("updates b")));
        let a = analyze("DELETE FROM sessions WHERE expires_at < NOW();", false);
        assert_eq!(a.queries[0].steps[0].text, "Removes rows from sessions where expires_at < NOW().");
        let a = analyze("UPDATE users u JOIN orders o ON o.user_id = u.id SET u.vip = 1 WHERE o.total > 100", false);
        let q = &a.queries[0];
        assert!(q.diagram.links.iter().any(|l| matches!(&l.kind, LinkKind::Join(j) if j == "INNER JOIN")));
    }

    #[test]
    fn comma_joins_and_warnings() {
        let a = analyze("SELECT * FROM a, b WHERE a.id = b.a_id AND b.name LIKE '%x'", false);
        let q = &a.queries[0];
        assert_eq!(q.steps[1].clause, ", (JOIN)");
        assert!(q.warnings.iter().any(|w| w.contains("SELECT *")));
        assert!(q.warnings.iter().any(|w| w.contains("LIKE")));
        assert!(!q.warnings.iter().any(|w| w.contains("multiplicatively")));
        let a = analyze("SELECT * FROM a, b", false);
        assert!(a.queries[0].warnings.iter().any(|w| w.contains("multiplicatively")));
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
        let m = to_mermaid(&a.schema);
        assert!(m.starts_with("erDiagram\n"));
        assert!(m.contains("orders }o--|| users : \"user_id\""));
        assert!(m.contains("bigint id PK"));
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
