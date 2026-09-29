//! SQL / CQL diagrams: an ER diagram of the schema and a data-flow picture of
//! each query, on a canvas that pans, zooms and lets tables be dragged.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui_kit::component::input::EditorState;
use gpui_kit::{
    Bounds, Context, CursorStyle, DispatchPhase, Entity, FontWeight, Hsla, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, PathBuilder, Pixels, Point, Render,
    ScrollWheelEvent, SharedString, StatefulInteractiveElement, Styled, Subscription, Task, Window, canvas, div,
    point, prelude::FluentBuilder, px, size,
};

use super::*;
use super::big;
use crate::logic::{self, sqlviz::{self, Analysis, Diagram, Entity as Ent, EntityKind, Key, LinkKind}};
use crate::theme::MONO_FONT;
use crate::ui::{self, BtnKind, MenuEntry, Tone};

// Box metrics in diagram units (multiplied by the zoom when drawn). All box
// text is monospaced, so widths can be worked out from character counts.
const HEAD_H: f32 = 36.;
const ROW_H: f32 = 26.;
const FOOT: f32 = 6.;
const PAD: f32 = 12.;
const BADGE_W: f32 = 28.;
const FONT: f32 = 12.;
const HEAD_FONT: f32 = 13.;
const CAP_FONT: f32 = 11.;
/// JetBrains Mono advance width, in ems.
const ADV: f32 = 0.6;
const MAX_TYPE: usize = 26;
const ZOOM_MIN: f32 = 0.25;
const ZOOM_MAX: f32 = 2.5;

const EXAMPLES: &[(&str, &str)] = &[
    ("MySQL shop schema and queries", MYSQL_SAMPLE),
    ("PostgreSQL analytics query", PG_SAMPLE),
    ("Cassandra (CQL) data model", CQL_SAMPLE),
];

const MYSQL_SAMPLE: &str = "-- A small e-commerce schema (MySQL)
CREATE TABLE users (
  id         BIGINT AUTO_INCREMENT PRIMARY KEY,
  email      VARCHAR(255) NOT NULL UNIQUE,
  full_name  VARCHAR(120),
  created_at TIMESTAMP NOT NULL
);

CREATE TABLE addresses (
  id      BIGINT AUTO_INCREMENT PRIMARY KEY,
  user_id BIGINT NOT NULL REFERENCES users(id),
  line1   VARCHAR(255) NOT NULL,
  city    VARCHAR(80),
  country CHAR(2) NOT NULL
);

CREATE TABLE products (
  id          BIGINT AUTO_INCREMENT PRIMARY KEY,
  sku         VARCHAR(64) NOT NULL UNIQUE,
  name        VARCHAR(200) NOT NULL,
  price_cents INT NOT NULL,
  active      BOOLEAN NOT NULL
);

CREATE TABLE orders (
  id          BIGINT AUTO_INCREMENT PRIMARY KEY,
  user_id     BIGINT NOT NULL,
  address_id  BIGINT,
  status      VARCHAR(32) NOT NULL,
  total_cents INT NOT NULL,
  placed_at   TIMESTAMP NOT NULL,
  FOREIGN KEY (user_id) REFERENCES users(id),
  FOREIGN KEY (address_id) REFERENCES addresses(id)
) ENGINE=InnoDB;

CREATE TABLE order_items (
  id         BIGINT AUTO_INCREMENT PRIMARY KEY,
  order_id   BIGINT NOT NULL,
  product_id BIGINT NOT NULL,
  quantity   INT NOT NULL,
  unit_cents INT NOT NULL,
  CONSTRAINT fk_oi_order   FOREIGN KEY (order_id)   REFERENCES orders(id),
  CONSTRAINT fk_oi_product FOREIGN KEY (product_id) REFERENCES products(id)
);

CREATE TABLE reviews (
  id         BIGINT AUTO_INCREMENT PRIMARY KEY,
  product_id BIGINT NOT NULL REFERENCES products(id),
  user_id    BIGINT NOT NULL REFERENCES users(id),
  rating     SMALLINT NOT NULL,
  body       TEXT,
  created_at TIMESTAMP NOT NULL
);

-- Top customers this year
SELECT u.id, u.email, COUNT(o.id) AS orders, SUM(o.total_cents) AS spent
FROM users u
LEFT JOIN orders o ON o.user_id = u.id AND o.status = 'paid'
WHERE u.created_at >= '2026-01-01'
GROUP BY u.id, u.email
HAVING COUNT(o.id) > 3
ORDER BY spent DESC
LIMIT 20;

-- Best-selling products with their average rating
SELECT p.name, SUM(oi.quantity) AS sold,
       (SELECT AVG(r.rating) FROM reviews r WHERE r.product_id = p.id) AS avg_rating
FROM order_items oi
JOIN products p ON p.id = oi.product_id
WHERE p.active = TRUE
GROUP BY p.id, p.name
ORDER BY sold DESC
LIMIT 10;

UPDATE orders o
JOIN users u ON u.id = o.user_id
SET o.status = 'flagged'
WHERE u.email LIKE '%@example.com';
";

const PG_SAMPLE: &str = "-- Monthly revenue with a running total (PostgreSQL)
CREATE TABLE customers (
  id         BIGSERIAL PRIMARY KEY,
  name       TEXT NOT NULL,
  region     TEXT NOT NULL
);

CREATE TABLE invoices (
  id          BIGSERIAL PRIMARY KEY,
  customer_id BIGINT NOT NULL REFERENCES customers(id),
  issued_at   TIMESTAMPTZ NOT NULL,
  amount      NUMERIC(12, 2) NOT NULL,
  paid        BOOLEAN NOT NULL DEFAULT FALSE
);

WITH monthly AS (
  SELECT c.region, date_trunc('month', i.issued_at) AS month, SUM(i.amount) AS revenue
  FROM invoices i
  JOIN customers c ON c.id = i.customer_id
  WHERE i.paid
  GROUP BY c.region, date_trunc('month', i.issued_at)
)
SELECT region, month, revenue,
       SUM(revenue) OVER (PARTITION BY region ORDER BY month) AS running_total
FROM monthly
ORDER BY region, month;

INSERT INTO invoices (customer_id, issued_at, amount)
VALUES (42, now(), 199.00)
ON CONFLICT (id) DO NOTHING
RETURNING id;
";

const CQL_SAMPLE: &str = "-- A Cassandra data model: one table per query
CREATE KEYSPACE IF NOT EXISTS shop
  WITH replication = {'class': 'NetworkTopologyStrategy', 'dc1': 3};

CREATE TYPE shop.address (street text, city text, zip text);

CREATE TABLE shop.users (
  user_id uuid PRIMARY KEY,
  email   text,
  name    text,
  home    frozen<address>
);

CREATE TABLE shop.products (
  product_id uuid PRIMARY KEY,
  name       text,
  price      decimal,
  tags       set<text>
);

CREATE TABLE shop.orders_by_user (
  user_id    uuid,
  order_date date,
  order_id   timeuuid,
  status     text,
  total      decimal,
  items      map<uuid, int>,
  PRIMARY KEY ((user_id), order_date, order_id)
) WITH CLUSTERING ORDER BY (order_date DESC, order_id ASC);

CREATE TABLE shop.reviews_by_product (
  product_id uuid,
  review_id  timeuuid,
  user_id    uuid,
  rating     int,
  body       text,
  PRIMARY KEY (product_id, review_id)
) WITH CLUSTERING ORDER BY (review_id DESC);

CREATE INDEX ON shop.orders_by_user (status);

-- Fast: the partition key is given, rows come back newest first
SELECT order_id, status, total
FROM shop.orders_by_user
WHERE user_id = ? AND order_date >= '2026-01-01'
LIMIT 50;

-- Slow: filters on a column outside the primary key
SELECT * FROM shop.reviews_by_product WHERE rating < 2 ALLOW FILTERING;

INSERT INTO shop.users (user_id, email, name)
VALUES (uuid(), 'ada@example.com', 'Ada')
IF NOT EXISTS;

UPDATE shop.orders_by_user USING TTL 2592000
SET status = 'shipped'
WHERE user_id = ? AND order_date = '2026-09-01' AND order_id = ?;
";

enum Drag {
    Pan { start: Point<Pixels>, pan: (f32, f32) },
    Node { idx: usize, start: Point<Pixels>, pos: (f32, f32) },
}

/// A link, resolved to canvas coordinates.
struct Edge {
    pts: [Point<Pixels>; 4],
    color: Hsla,
    width: f32,
    dashed: bool,
    /// Direction each end leaves its box: +1 to the right, -1 to the left.
    dirs: (f32, f32),
    kind: LinkKind,
    label: Option<(Point<Pixels>, String)>,
}

pub struct SqlVizView {
    input: Entity<EditorState>,
    analysis: Rc<Analysis>,
    infer: bool,
    /// 0 = the schema; n = query n.
    view: usize,
    sizes: Vec<(f32, f32)>,
    positions: Vec<(f32, f32)>,
    /// Boxes the user dragged, by view and entity name.
    moved: HashMap<(usize, String), (f32, f32)>,
    signature: String,
    pan: (f32, f32),
    zoom: f32,
    fit_pending: bool,
    drag: Option<Drag>,
    hover: Option<usize>,
    selected: Option<usize>,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    task: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

fn type_label(c: &sqlviz::Column) -> String {
    let mut t = c.ty.clone();
    if t.chars().count() > MAX_TYPE {
        t = format!("{}…", t.chars().take(MAX_TYPE - 1).collect::<String>());
    }
    match c.note.as_deref() {
        Some("desc") => format!("{t} ↓"),
        Some("asc") => format!("{t} ↑"),
        Some(n) if t.is_empty() => n.to_string(),
        Some(n) => format!("{t} · {n}"),
        None => t,
    }
}

fn badge_of(c: &sqlviz::Column) -> Option<&'static str> {
    match c.key {
        Key::Primary | Key::Partition => Some("PK"),
        Key::Clustering => Some("CK"),
        Key::None if c.fk => Some("FK"),
        Key::None if c.unique => Some("UQ"),
        Key::None if c.indexed => Some("IX"),
        Key::None => None,
    }
}

fn chars(s: &str) -> f32 {
    s.chars().count() as f32
}

fn box_size(e: &Ent) -> (f32, f32) {
    let head = PAD * 2. + chars(&e.name) * HEAD_FONT * ADV + e.caption.as_ref().map_or(0., |c| 10. + chars(c) * CAP_FONT * ADV);
    let name_w = e.columns.iter().map(|c| chars(&c.name)).fold(0., f32::max) * FONT * ADV;
    let type_w = e.columns.iter().map(|c| chars(&type_label(c))).fold(0., f32::max) * FONT * ADV;
    let filter = if e.columns.iter().any(|c| c.filtered) { 18. } else { 0. };
    let body = PAD * 2. + BADGE_W + name_w + filter + 20. + type_w;
    let rows = e.columns.len().max(1) as f32;
    (head.max(body).max(170.).ceil(), HEAD_H + rows * ROW_H + FOOT)
}

impl SqlVizView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = code_editor(MYSQL_SAMPLE, "Paste CREATE TABLE statements, queries, or both", "sql", window, cx);
        let subs = vec![watch(&input, window, cx, Self::recompute)];
        let mut this = Self {
            input,
            analysis: Rc::new(sqlviz::analyze("", true)),
            infer: true,
            view: 0,
            sizes: Vec::new(),
            positions: Vec::new(),
            moved: HashMap::new(),
            signature: String::new(),
            pan: (40., 40.),
            zoom: 1.,
            fit_pending: true,
            drag: None,
            hover: None,
            selected: None,
            bounds: Rc::new(Cell::new(Bounds::default())),
            task: None,
            _subs: subs,
        };
        this.recompute(window, cx);
        this
    }

    fn recompute(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let src = text_of(&self.input, cx);
        let infer = self.infer;
        big::run(src.len(), self, |v| &mut v.task, window, cx, move || sqlviz::analyze(&src, infer), |this, a, _, cx| {
            let had_schema = this.analysis.table_count() > 0;
            this.analysis = Rc::new(a);
            let views = 1 + this.analysis.queries.len();
            let has_schema = this.analysis.table_count() > 0;
            if this.view >= views || (this.view == 0 && !has_schema) || (!had_schema && has_schema && this.view > 0 && this.analysis.queries.is_empty()) {
                this.view = if has_schema || this.analysis.queries.is_empty() { 0 } else { 1 };
            }
            this.relayout();
            cx.notify();
        });
    }

    fn diagram(&self) -> &Diagram {
        match self.view {
            0 => &self.analysis.schema,
            n => self.analysis.queries.get(n - 1).map(|q| &q.diagram).unwrap_or(&self.analysis.schema),
        }
    }

    /// Size and place every box; refit when a different set of boxes is shown.
    fn relayout(&mut self) {
        let d = self.diagram();
        let sizes: Vec<(f32, f32)> = d.entities.iter().map(box_size).collect();
        let links: Vec<(usize, usize)> = d.links.iter().map(|l| (l.from, l.to)).collect();
        let mut positions = sqlviz::layout(&sizes, &links, 90., 36.);
        for (i, e) in d.entities.iter().enumerate() {
            if let Some(p) = self.moved.get(&(self.view, e.name.clone())) {
                positions[i] = *p;
            }
        }
        let signature = format!("{}|{}", self.view, d.entities.iter().map(|e| e.name.as_str()).collect::<Vec<_>>().join(","));
        if signature != self.signature {
            self.signature = signature;
            self.fit_pending = true;
            self.hover = None;
            self.selected = None;
        }
        self.sizes = sizes;
        self.positions = positions;
    }

    fn set_view(&mut self, view: usize, cx: &mut Context<Self>) {
        self.view = view;
        self.relayout();
        cx.notify();
    }

    fn fit(&mut self) {
        let b = self.bounds.get();
        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
        if w < 10. || self.positions.is_empty() {
            return;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (p, s) in self.positions.iter().zip(&self.sizes) {
            x0 = x0.min(p.0);
            y0 = y0.min(p.1);
            x1 = x1.max(p.0 + s.0);
            y1 = y1.max(p.1 + s.1);
        }
        let margin = 48.;
        let z = ((w - margin * 2.) / (x1 - x0)).min((h - margin * 2. - 40.) / (y1 - y0)).clamp(ZOOM_MIN, 1.25);
        self.zoom = z;
        self.pan = ((w - (x1 - x0) * z) / 2. - x0 * z, (h - 40. - (y1 - y0) * z) / 2. - y0 * z);
        self.fit_pending = false;
    }

    fn zoom_at(&mut self, factor: f32, at: (f32, f32)) {
        let z = (self.zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
        let k = z / self.zoom;
        self.pan = (at.0 - (at.0 - self.pan.0) * k, at.1 - (at.1 - self.pan.1) * k);
        self.zoom = z;
    }

    fn zoom_center(&mut self, factor: f32) {
        let b = self.bounds.get();
        self.zoom_at(factor, (f32::from(b.size.width) / 2., f32::from(b.size.height) / 2.));
    }

    fn local(&self, p: Point<Pixels>) -> (f32, f32) {
        let o = self.bounds.get().origin;
        (f32::from(p.x - o.x), f32::from(p.y - o.y))
    }

    fn drag_to(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) {
        match &self.drag {
            Some(Drag::Pan { start, pan }) => {
                self.pan = (pan.0 + f32::from(p.x - start.x), pan.1 + f32::from(p.y - start.y));
            }
            Some(Drag::Node { idx, start, pos }) => {
                let np = (pos.0 + f32::from(p.x - start.x) / self.zoom, pos.1 + f32::from(p.y - start.y) / self.zoom);
                if let Some(slot) = self.positions.get_mut(*idx) {
                    *slot = np;
                }
                if let Some(e) = self.diagram().entities.get(*idx) {
                    let key = (self.view, e.name.clone());
                    self.moved.insert(key, np);
                }
            }
            None => return,
        }
        cx.notify();
    }

    fn arrange(&mut self, cx: &mut Context<Self>) {
        let view = self.view;
        self.moved.retain(|k, _| k.0 != view);
        self.signature.clear();
        self.relayout();
        self.fit();
        cx.notify();
    }

    fn load_example(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.moved.clear();
        self.view = 0;
        set_text(&self.input, EXAMPLES[i].1, window, cx);
        self.recompute(window, cx);
    }

    fn view_label(&self, i: usize) -> String {
        match i {
            0 => {
                let a = &self.analysis;
                format!("Schema · {} · {}", logic::plural(a.table_count(), "table"), logic::plural(a.link_count(), "link"))
            }
            n => match self.analysis.queries.get(n - 1) {
                Some(q) => format!("Query {n} · {}", q.preview),
                None => String::new(),
            },
        }
    }

    // ------------------------------------------------------------ drawing

    fn edges(&self, pal: &Pal) -> Vec<Edge> {
        let d = self.diagram();
        let z = self.zoom;
        let focus = self.selected.or(self.hover);
        let scr = |x: f32, y: f32| point(px(self.pan.0 + x * z), px(self.pan.1 + y * z));
        let row_y = |e: usize, c: Option<usize>| {
            let top = self.positions[e].1;
            match c {
                Some(c) => top + HEAD_H + c as f32 * ROW_H + ROW_H / 2.,
                None => top + HEAD_H / 2.,
            }
        };
        let mut out = Vec::new();
        for l in &d.links {
            if l.from >= self.positions.len() || l.to >= self.positions.len() {
                continue;
            }
            let (fp, fs) = (self.positions[l.from], self.sizes[l.from]);
            let (tp, ts) = (self.positions[l.to], self.sizes[l.to]);
            let (fy, ty) = (row_y(l.from, l.from_col), row_y(l.to, l.to_col));
            let (fx, tx, dirs) = if tp.0 >= fp.0 + fs.0 + 16. {
                (fp.0 + fs.0, tp.0, (1., -1.))
            } else if fp.0 >= tp.0 + ts.0 + 16. {
                (fp.0, tp.0 + ts.0, (-1., 1.))
            } else {
                (fp.0 + fs.0, tp.0 + ts.0, (1., 1.))
            };
            let reach = if dirs.0 == dirs.1 { 40. + (fy - ty).abs() * 0.15 } else { ((tx - fx).abs() * 0.5).max(40.) };
            let pts = [
                scr(fx, fy),
                scr(fx + dirs.0 * reach, fy),
                scr(tx + dirs.1 * reach, ty),
                scr(tx, ty),
            ];
            let lit = focus.is_some_and(|f| f == l.from || f == l.to);
            let dim = focus.is_some() && !lit;
            let (mut color, dashed) = match l.kind {
                LinkKind::ForeignKey => (pal.text3, false),
                LinkKind::Inferred => (pal.text3, true),
                LinkKind::Join(_) => (pal.accent, false),
                LinkKind::Flow => (pal.tok_p, true),
                LinkKind::Uses => (pal.tok_s, true),
            };
            if lit {
                color = pal.accent;
            }
            if dim {
                color.a *= 0.3;
            }
            let label = match &l.kind {
                LinkKind::Join(j) => {
                    let m = bezier_at(&pts, 0.5);
                    Some((m, j.clone()))
                }
                _ => None,
            };
            out.push(Edge { pts, color, width: if lit { 2. } else { 1.4 }, dashed, dirs, kind: l.kind.clone(), label });
        }
        out
    }

    fn render_box(&self, i: usize, e: &Ent, pal: &Pal, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let z = self.zoom;
        let (x, y) = self.positions[i];
        let (w, h) = self.sizes[i];
        let p = *pal;
        let lit = self.selected == Some(i) || self.hover == Some(i);
        let (tint, kind_label) = match e.kind {
            EntityKind::Table => (None, None),
            EntityKind::View => (Some(p.tok_p), None),
            EntityKind::Type => (Some(p.tok_s), None),
            EntityKind::Cte => (Some(p.accent), Some("CTE")),
            EntityKind::Derived => (Some(p.accent), None),
            EntityKind::Result => (Some(p.ok), None),
        };
        let head_bg = match tint {
            Some(mut c) => {
                c.a = if p.dark { 0.16 } else { 0.10 };
                ui::mix(p.card, c, 1.)
            }
            None => p.subtle,
        };
        let query_view = self.view > 0;
        let rows = e.columns.iter().enumerate().map(move |(_, c)| {
            let badge = badge_of(c);
            let badge_color = match badge {
                Some("PK") => p.star,
                Some("CK") => p.tok_p,
                Some("FK") => p.accent,
                _ => p.text3,
            };
            let hl = query_view && c.used && e.kind == EntityKind::Table;
            let faded = query_view && !c.used && e.kind == EntityKind::Table;
            let fg = if faded { p.text3 } else { p.text };
            div()
                .flex()
                .items_center()
                .h(px(ROW_H * z))
                .px(px(PAD * z))
                .when(hl, |d| d.bg(p.accent_soft))
                .child(
                    div()
                        .w(px(BADGE_W * z))
                        .flex_none()
                        .text_size(px(10. * z))
                        .font_weight(FontWeight::BOLD)
                        .text_color(badge_color)
                        .when_some(badge, |d, b| d.child(b)),
                )
                .child(div().flex_none().text_color(fg).child(c.name.clone()))
                .when(c.filtered, |d| d.child(div().ml(px(4. * z)).child(crate::icons::icon("filter", 12. * z, p.accent))))
                .child(div().flex_1().min_w(px(20. * z)))
                .child(div().flex_none().text_color(p.text3).child(type_label(c)))
        });
        let caption = e.caption.clone().or(kind_label.map(String::from));
        div()
            .id(("sv-box", i))
            .absolute()
            .left(px(self.pan.0 + x * z))
            .top(px(self.pan.1 + y * z))
            .w(px(w * z))
            .h(px(h * z))
            .flex()
            .flex_col()
            .bg(p.card)
            .border_1()
            .border_color(if lit { p.accent } else { p.stroke_strong })
            .rounded(px(8. * z))
            .overflow_hidden()
            .shadow(if lit { vec![Pal::ring(p.accent_soft, 3.)] } else { p.shadow() })
            .font_family(MONO_FONT)
            .text_size(px(FONT * z))
            .cursor(CursorStyle::OpenHand)
            .on_hover(cx.listener(move |this, h: &bool, _, cx| {
                if *h {
                    this.hover = Some(i);
                } else if this.hover == Some(i) {
                    this.hover = None;
                }
                cx.notify();
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                let pos = this.positions.get(i).copied().unwrap_or_default();
                this.drag = Some(Drag::Node { idx: i, start: ev.position, pos });
                this.selected = if this.selected == Some(i) { None } else { Some(i) };
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_none()
                    .gap(px(10. * z))
                    .h(px(HEAD_H * z))
                    .px(px(PAD * z))
                    .bg(head_bg)
                    .border_b_1()
                    .border_color(p.stroke)
                    .child(div().flex_none().text_size(px(HEAD_FONT * z)).font_weight(FontWeight::SEMIBOLD).text_color(p.text).child(e.name.clone()))
                    .when_some(caption, |d, c| d.child(div().flex_none().text_size(px(CAP_FONT * z)).text_color(tint.unwrap_or(p.text3)).child(c))),
            )
            .children(rows)
            .when(e.columns.is_empty(), |d| {
                d.child(div().h(px(ROW_H * z)).px(px(PAD * z)).flex().items_center().text_color(p.text3).child("no columns"))
            })
    }

    fn canvas_el(&mut self, pal: &Pal, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        if self.fit_pending {
            self.fit();
        }
        let p = *pal;
        let edges = self.edges(pal);
        let labels: Vec<(Point<Pixels>, String, Hsla)> = edges.iter().filter_map(|e| e.label.clone().map(|(pt, s)| (pt, s, e.color))).collect();
        let z = self.zoom;
        let pan = self.pan;
        let bounds_cell = self.bounds.clone();
        let refit = self.fit_pending;
        let dragging = self.drag.is_some();
        let weak = cx.weak_entity();
        let d = self.diagram();
        let empty = d.entities.is_empty();
        let boxes: Vec<_> = d.entities.iter().enumerate().map(|(i, e)| self.render_box(i, e, pal, cx).into_any_element()).collect();

        let paint = canvas(
            move |bounds, window, _| {
                if bounds_cell.get() != bounds {
                    bounds_cell.set(bounds);
                    if refit {
                        window.refresh();
                    }
                }
                bounds
            },
            move |bounds, _, window, _| {
                let o = bounds.origin;
                // Dot grid that moves with the diagram.
                let step = 24. * z;
                if step >= 8. {
                    let dot = (1.6 * z.max(0.6)).min(2.);
                    let (ox, oy) = (pan.0.rem_euclid(step), pan.1.rem_euclid(step));
                    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                    let mut gx = ox;
                    while gx < w {
                        let mut gy = oy;
                        while gy < h {
                            window.paint_quad(gpui_kit::fill(Bounds::new(o + point(px(gx), px(gy)), size(px(dot), px(dot))), p.stroke_strong));
                            gy += step;
                        }
                        gx += step;
                    }
                }
                for e in &edges {
                    let at = |q: Point<Pixels>| o + q;
                    let mut b = PathBuilder::stroke(px(e.width));
                    if e.dashed {
                        b = b.dash_array(&[px(5.), px(4.)]);
                    }
                    b.move_to(at(e.pts[0]));
                    b.cubic_bezier_to(at(e.pts[3]), at(e.pts[1]), at(e.pts[2]));
                    if let Ok(path) = b.build() {
                        window.paint_path(path, e.color);
                    }
                    let m = z.clamp(0.6, 1.4);
                    let mut marks = PathBuilder::stroke(px(e.width));
                    let (f, t) = (at(e.pts[0]), at(e.pts[3]));
                    let (df, dt) = (e.dirs.0, e.dirs.1);
                    match e.kind {
                        LinkKind::ForeignKey | LinkKind::Inferred => {
                            // Crow's foot (many) at the referencing end...
                            let base = f + point(px(df * 12. * m), px(0.));
                            for dy in [-6., 0., 6.] {
                                marks.move_to(base);
                                marks.line_to(f + point(px(0.), px(dy * m)));
                            }
                            // ...and a bar (one) at the referenced end.
                            let bar = t + point(px(dt * 9. * m), px(0.));
                            marks.move_to(bar + point(px(0.), px(-6. * m)));
                            marks.line_to(bar + point(px(0.), px(6. * m)));
                        }
                        LinkKind::Flow | LinkKind::Uses => {
                            let mut tri = PathBuilder::fill();
                            tri.move_to(t);
                            tri.line_to(t + point(px(dt * 8. * m), px(-4.5 * m)));
                            tri.line_to(t + point(px(dt * 8. * m), px(4.5 * m)));
                            tri.close();
                            if let Ok(path) = tri.build() {
                                window.paint_path(path, e.color);
                            }
                        }
                        LinkKind::Join(_) => {
                            for (pt, dir) in [(f, df), (t, dt)] {
                                let c = pt + point(px(dir * 3. * m), px(0.));
                                let r = 3. * m;
                                window.paint_quad(
                                    gpui_kit::fill(Bounds::new(c - point(px(r), px(r)), size(px(r * 2.), px(r * 2.))), e.color).corner_radii(px(r)),
                                );
                            }
                        }
                    }
                    if let Ok(path) = marks.build() {
                        window.paint_path(path, e.color);
                    }
                }
                // While dragging, follow the mouse even outside the canvas.
                if dragging {
                    let w1 = weak.clone();
                    window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _, cx| {
                        if phase == DispatchPhase::Bubble {
                            let _ = w1.update(cx, |v, cx| v.drag_to(ev.position, cx));
                        }
                    });
                    let w2 = weak.clone();
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                        if phase == DispatchPhase::Bubble {
                            let _ = w2.update(cx, |v, cx| {
                                v.drag = None;
                                cx.notify();
                            });
                        }
                    });
                }
            },
        )
        .absolute()
        .size_full();

        let fs = 10. * z.clamp(0.7, 1.2);
        let join_labels = labels.into_iter().map(move |(pt, text, color)| {
            div()
                .absolute()
                .left(pt.x - px(chars(&text) * fs * ADV / 2. + 6.))
                .top(pt.y - px(fs * 0.9 + 3.))
                .px(px(6.))
                .py(px(2.))
                .rounded(px(4.))
                .bg(p.card)
                .border_1()
                .border_color(color)
                .font_family(MONO_FONT)
                .text_size(px(fs))
                .text_color(color)
                .child(text)
        });

        let tool = |id: &'static str, label: SharedString| {
            div()
                .id(id)
                .flex()
                .items_center()
                .justify_center()
                .h(px(28.))
                .min_w(px(28.))
                .px(px(8.))
                .rounded(px(5.))
                .text_size(px(12.))
                .text_color(p.text2)
                .cursor_pointer()
                .hover(move |s| s.bg(p.subtle2).text_color(p.text))
                .child(label)
        };
        let toolbar = div()
            .absolute()
            .bottom(px(12.))
            .right(px(12.))
            .flex()
            .items_center()
            .gap(px(2.))
            .p(px(3.))
            .rounded(px(8.))
            .bg(p.card)
            .border_1()
            .border_color(p.stroke_strong)
            .shadow(p.shadow())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(tool("sv-arrange", "Arrange".into()).on_click(cx.listener(|this, _, _, cx| this.arrange(cx))))
            .child(tool("sv-fit", "Fit".into()).on_click(cx.listener(|this, _, _, cx| {
                this.fit();
                cx.notify();
            })))
            .child(div().w(px(1.)).h(px(16.)).mx(px(2.)).bg(p.stroke_strong))
            .child(tool("sv-out", "−".into()).on_click(cx.listener(|this, _, _, cx| {
                this.zoom_center(1. / 1.2);
                cx.notify();
            })))
            .child(div().w(px(44.)).text_center().text_size(px(12.)).text_color(p.text2).child(format!("{:.0}%", self.zoom * 100.)))
            .child(tool("sv-in", "+".into()).on_click(cx.listener(|this, _, _, cx| {
                this.zoom_center(1.2);
                cx.notify();
            })));

        let cassandra = self.analysis.dialect == sqlviz::Dialect::Cassandra;
        let legend_item = |badge: &'static str, color: Hsla, text: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(px(5.))
                .child(div().font_family(MONO_FONT).text_size(px(10.)).font_weight(FontWeight::BOLD).text_color(color).child(badge))
                .child(text)
        };
        let legend = div()
            .absolute()
            .top(px(10.))
            .left(px(12.))
            .flex()
            .gap(px(12.))
            .text_size(px(11.))
            .text_color(p.text3)
            .when(cassandra, |d| d.child(legend_item("PK", p.star, "partition key")).child(legend_item("CK", p.tok_p, "clustering key")))
            .when(!cassandra, |d| d.child(legend_item("PK", p.star, "primary key")))
            .child(legend_item("FK", p.accent, "foreign key"))
            .child(legend_item("UQ", p.text3, "unique"))
            .child(legend_item("IX", p.text3, "indexed"))
            .when(self.view > 0, |d| d.child(div().flex().items_center().gap(px(5.)).child(crate::icons::icon("filter", 11., p.accent)).child("filtered")));

        let _ = window;
        div()
            .id("sv-canvas")
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .bg(p.editor)
            .cursor(if matches!(self.drag, Some(Drag::Pan { .. })) { CursorStyle::ClosedHand } else { CursorStyle::Arrow })
            .on_mouse_down(MouseButton::Left, cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                this.drag = Some(Drag::Pan { start: ev.position, pan: this.pan });
                this.selected = None;
                cx.notify();
            }))
            .on_scroll_wheel(cx.listener(|this, ev: &ScrollWheelEvent, _, cx| {
                cx.stop_propagation();
                let delta = ev.delta.pixel_delta(px(20.));
                if ev.modifiers.control || ev.modifiers.platform {
                    let at = this.local(ev.position);
                    let factor = (1.0015f32).powf(f32::from(delta.y));
                    this.zoom_at(factor, at);
                } else {
                    this.pan = (this.pan.0 + f32::from(delta.x), this.pan.1 + f32::from(delta.y));
                }
                cx.notify();
            }))
            .child(paint)
            .children(join_labels)
            .children(boxes)
            .when(empty, |d| {
                d.child(
                    div()
                        .absolute()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(13.))
                        .text_color(p.text3)
                        .child("Nothing to draw yet: add CREATE TABLE statements or a query."),
                )
            })
            .child(legend)
            .child(toolbar)
    }

    // ------------------------------------------------------------ explanation

    fn explain_el(&self, pal: &Pal) -> gpui_kit::AnyElement {
        let p = *pal;
        let a = &self.analysis;
        if self.view == 0 {
            if a.relations.is_empty() && a.table_count() == 0 {
                return div().text_size(px(13.)).text_color(p.text3).child("Add CREATE TABLE statements to see how the tables relate, or a query to see what it does step by step.").into_any_element();
            }
            let mut card = ui::kv_card(&p);
            let n = a.relations.len();
            if n == 0 {
                card = card.child(ui::kv_row("sv-rel-none", true, &p).child(div().text_size(px(13.)).text_color(p.text3).child("No relationships found. Turn on \"Guess missing links\" to connect columns like user_id to users.")));
            }
            for (i, r) in a.relations.iter().enumerate() {
                let (link, text) = r.split_once(" · ").unwrap_or((r.as_str(), ""));
                card = card.child(
                    ui::kv_row(("sv-rel", i), i + 1 == n, &p)
                        .child(div().w(px(300.)).flex_none().font_family(MONO_FONT).text_size(px(12.)).child(link.to_string()))
                        .child(ui::kv_val(text.to_string(), true).text_color(p.text2)),
                );
            }
            return card.into_any_element();
        }
        let Some(q) = a.queries.get(self.view - 1) else { return div().into_any_element() };
        let n = q.steps.len();
        let mut card = ui::kv_card(&p);
        for (i, s) in q.steps.iter().enumerate() {
            card = card.child(
                ui::kv_row(("sv-step", i), i + 1 == n, &p)
                    .items_start()
                    .pl(px(14. + s.depth as f32 * 22.))
                    .child(div().w(px(22.)).flex_none().text_size(px(12.)).text_color(p.text3).pt(px(2.)).child(format!("{}.", i + 1)))
                    .child(div().w(px(130.)).flex_none().child(ui::badge(s.clause.clone(), Tone::Info, &p)))
                    .child(ui::kv_val(s.text.clone(), true)),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).text_color(p.text).child(q.summary.clone()))
            .child(card)
            .when(!q.warnings.is_empty(), |d| {
                d.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .p(px(12.))
                        .rounded(px(8.))
                        .bg(p.danger_soft)
                        .border_1()
                        .border_color(p.stroke)
                        .children(q.warnings.iter().map(|w| {
                            div()
                                .flex()
                                .gap(px(8.))
                                .items_start()
                                .text_size(px(13.))
                                .text_color(p.text)
                                .child(crate::icons::icon("warn", 15., p.warn).mt(px(2.)))
                                .child(div().flex_1().child(w.clone()))
                        })),
                )
            })
            .into_any_element()
    }
}

/// A point on a cubic Bézier.
fn bezier_at(p: &[Point<Pixels>; 4], t: f32) -> Point<Pixels> {
    let u = 1. - t;
    let (a, b, c, d) = (u * u * u, 3. * u * u * t, 3. * u * t * t, t * t * t);
    let f = |i: usize| (f32::from(p[i].x), f32::from(p[i].y));
    let (p0, p1, p2, p3) = (f(0), f(1), f(2), f(3));
    point(px(a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0), px(a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1))
}

impl Render for SqlVizView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let [paste, clear] = paste_clear("sv", &self.input, &pal, cx, Self::recompute);
        let on_example = on_index(cx, |this: &mut Self, i, w, cx| this.load_example(i, w, cx));
        let on_view = on_index(cx, |this: &mut Self, i, _, cx| this.set_view(i, cx));
        let examples = ui::menu_btn(
            "sv-examples",
            None,
            Some("Examples".into()),
            BtnKind::Normal,
            EXAMPLES.iter().map(|(label, _)| MenuEntry::new(*label, Some("db"))).collect(),
            &pal,
            window,
            cx,
            on_example,
        );
        let views = 1 + self.analysis.queries.len();
        let entries: Vec<MenuEntry> = (0..views).map(|i| MenuEntry::new(self.view_label(i), Some(if i == 0 { "table" } else { "flow" }))).collect();
        let picker = ui::menu_btn(
            "sv-view",
            Some(if self.view == 0 { "table" } else { "flow" }),
            Some(big::preview(&self.view_label(self.view), 48).into()),
            BtnKind::Normal,
            entries,
            &pal,
            window,
            cx,
            on_view,
        );
        let export: SharedString = if self.view == 0 {
            sqlviz::to_mermaid(&self.analysis.schema).into()
        } else {
            self.analysis.queries.get(self.view - 1).map(sqlviz::steps_text).unwrap_or_default().into()
        };
        let copy_tip = if self.view == 0 { "Copy as Mermaid erDiagram" } else { "Copy the walk-through as text" };
        let zoom = ui::PaneZoom::new("sv-diagram", window, cx);
        let canvas = self.canvas_el(&pal, window, cx);
        let a = &self.analysis;
        let status = format!(
            "{} · {} · {}",
            a.dialect.label(),
            logic::plural(a.table_count(), "table"),
            match a.queries.len() {
                1 => "1 query".to_string(),
                n => format!("{n} queries"),
            }
        );
        let skipped = a.skipped.len();

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(ui::section_label("Configuration", &pal))
            .child(ui::setting(
                "sv-infer",
                ui::setting_icon("link", &pal),
                "Guess missing links",
                Some("Connect columns such as user_id to users when no foreign key is declared (dashed lines). Useful for Cassandra and schemas without constraints.".into()),
                ui::toggle_labeled("sv-infer-tg", self.infer, &pal, cx.listener(|this, _, w, cx| {
                    this.infer = !this.infer;
                    this.recompute(w, cx);
                })),
                &pal,
            ))
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .h(px(620.))
                    .mt(px(8.))
                    .child(
                        ui::pane(is_focused(&self.input, window, cx), &pal)
                            .w(px(380.))
                            .flex_none()
                            .child(ui::pane_head("SQL / CQL", None, &pal).child(examples).child(paste).child(clear))
                            .child(code_editor_el(&self.input, false, cx))
                            .child(
                                ui::pane_foot(&pal)
                                    .child(ui::dot(Some(if skipped > 0 { Tone::Info } else { Tone::Ok }), &pal))
                                    .child(status)
                                    .when(skipped > 0, |d| d.child(format!("· {} skipped", skipped))),
                            ),
                    )
                    .child(zoom.wrap(
                        ui::pane(false, &pal)
                            .flex_1()
                            .child(
                                ui::pane_head("Diagram", None, &pal)
                                    .gap(px(6.))
                                    .child(picker)
                                    .child(ui::copy_btn("sv-copy", export, &pal, window, cx).tooltip(move |w, cx| {
                                        gpui_kit::component::tooltip::Tooltip::new(copy_tip).build(w, cx)
                                    }))
                                    .child(zoom.button(&pal)),
                            )
                            .child(canvas),
                        &pal,
                        window,
                    )),
            )
            .child(ui::section_label(if self.view == 0 { "Relationships" } else { "What this query does" }, &pal).mt(px(14.)))
            .child(self.explain_el(&pal))
            .when(skipped > 0, |d| {
                d.child(
                    div()
                        .mt(px(8.))
                        .text_size(px(12.))
                        .text_color(pal.text3)
                        .child(format!("Skipped statements this tool does not draw: {}", self.analysis.skipped.join(" · "))),
                )
            })
    }
}

