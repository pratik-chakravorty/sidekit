//! Break a URL into its parts and decode its query string.

use gpui_kit::component::input::InputState;
use gpui_kit::{
    Context, Entity, IntoElement, ParentElement, Render, Styled, Subscription, Window, div,
    prelude::FluentBuilder, px,
};

use super::*;
use crate::logic::{parse_url, plural};
use crate::ui;

pub struct UrlParseView {
    input: Entity<InputState>,
    _subs: Vec<Subscription>,
}

impl UrlParseView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = line(
            "https://user@api.example.com:8443/v2/search?q=dev+tools&tags=rust%2Cgpui&page=2#results",
            "Paste a URL",
            window,
            cx,
        );
        let subs = vec![watch(&input, window, cx, |_, _, cx| cx.notify())];
        Self { input, _subs: subs }
    }

    pub fn set_input(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        set_line(&self.input, text, window, cx);
        cx.notify();
    }
}

impl Render for UrlParseView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let raw = line_text(&self.input, cx);
        let parsed = if raw.trim().is_empty() { None } else { Some(parse_url(&raw)) };

        let mut body = div().flex().flex_col().gap(px(6.));
        match parsed {
            Some(Ok(u)) => {
                let n = u.rows.len();
                let mut card = ui::kv_card(&pal);
                for (i, (label, value)) in u.rows.into_iter().enumerate() {
                    card = card.child(ui::kv_copy_row(("up", i), label, 110., value.into(), true, i + 1 == n, &pal, window, cx));
                }
                body = body.child(ui::section_label("Parts", &pal).mt(px(14.))).child(card);
                if !u.params.is_empty() {
                    let n = u.params.len();
                    let mut params = ui::kv_card(&pal);
                    for (i, (k, v)) in u.params.into_iter().enumerate() {
                        params = params.child(ui::kv_copy_row(("uq", i), k, 150., v.into(), true, i + 1 == n, &pal, window, cx));
                    }
                    body = body
                        .child(ui::section_label(format!("Query parameters · {}", plural(n, "parameter")), &pal).mt(px(14.)))
                        .child(params);
                }
            }
            Some(Err(e)) => body = body.child(ui::err_box(e, &pal).mt(px(4.))),
            None => {}
        }

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(ui::section_label("URL", &pal).flex_1())
                    .when(!raw.is_empty(), |d| d.child(ui::copy_btn("up-copy", raw.clone().into(), &pal, window, cx))),
            )
            .child(field_el(&self.input, true, 44., 14., window, cx))
            .child(body)
    }
}
