//! Fake records as JSON or CSV.

use std::collections::HashSet;

use gpui_kit::component::input::{EditorState, InputState};
use gpui_kit::{
    Context, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Window, div, prelude::FluentBuilder, px,
};

use super::*;
use crate::logic::{self, Indent, mock, random_u32};
use crate::ui::{self, BtnKind};

pub struct MockView {
    count: Entity<InputState>,
    output: Entity<EditorState>,
    fields: HashSet<&'static str>,
    csv: bool,
    seed: u32,
    out: SharedString,
    _subs: Vec<Subscription>,
}

impl MockView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let count = line("10", "", window, cx);
        let output = code_editor("", "", "json", window, cx);
        let subs = vec![watch(&count, window, cx, Self::generate), watch(&output, window, cx, |_, _, _| {})];
        let fields = ["id", "name", "email", "company", "city", "active"].into_iter().collect();
        let mut this = Self { count, output, fields, csv: false, seed: random_u32(), out: SharedString::default(), _subs: subs };
        this.generate(window, cx);
        this
    }

    fn generate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let n = line_text(&self.count, cx).trim().parse::<usize>().unwrap_or(1).clamp(1, 1000);
        // Keep the catalogue's column order, not the order fields were ticked.
        let fields: Vec<&str> = mock::FIELDS.iter().map(|f| f.0).filter(|f| self.fields.contains(f)).collect();
        let rows = mock::records(&fields, n, self.seed);
        let v = serde_json::Value::Array(rows);
        let out = if self.csv { logic::csv::json_to_csv(&v, ',').unwrap_or_default() } else { logic::to_json(&v, &Indent::Spaces(2)) };
        self.out = out.into();
        set_text(&self.output, &self.out, window, cx);
        cx.notify();
    }
}

impl Render for MockView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let on_format = on_index(cx, |this: &mut Self, i, w, cx| {
            this.csv = i == 1;
            this.output.update(cx, |s, cx| s.set_highlighter(if i == 1 { "plaintext" } else { "json" }, cx));
            this.generate(w, cx);
        });
        let chips = mock::FIELDS.iter().map(|(key, label)| {
            let on = self.fields.contains(key);
            let key = *key;
            div()
                .id(crate::id!("mock-f-{key}"))
                .flex()
                .items_center()
                .h(px(28.))
                .px(px(12.))
                .rounded(px(14.))
                .border_1()
                .cursor_pointer()
                .text_size(px(12.5))
                .when(on, |d| d.bg(pal.accent_soft).border_color(pal.accent).text_color(pal.accent).font_weight(FontWeight::SEMIBOLD))
                .when(!on, |d| d.border_color(pal.stroke).text_color(pal.text2).hover(move |s| s.bg(pal.subtle)))
                .child(*label)
                .on_click(cx.listener(move |this, _, w, cx| {
                    if !this.fields.remove(key) {
                        this.fields.insert(key);
                    }
                    this.generate(w, cx);
                }))
        }).collect::<Vec<_>>();
        let zoom = ui::PaneZoom::new("mock-out", window, cx);

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .flex_1()
            .child(ui::section_label("Configuration", &pal))
            .child(ui::setting(
                "mock-fmt",
                ui::setting_icon("braces", &pal),
                "Format",
                None,
                ui::seg("mock-fmt-seg", &["JSON", "CSV"], self.csv as usize, &pal, on_format),
                &pal,
            ))
            .child(ui::section_label("Fields", &pal).mt(px(10.)))
            .child(div().flex().flex_wrap().gap(px(6.)).children(chips))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .mt(px(14.))
                    .child(ui::section_label("Generate", &pal).flex_1())
                    .child(field_el(&self.count, false, 32., 13., window, cx).w(px(80.)))
                    .child(div().text_size(px(13.)).text_color(pal.text2).child("records"))
                    .child(ui::btn("mock-gen", Some("refresh"), "New data", BtnKind::Accent, &pal, cx.listener(|this, _, w, cx| {
                        this.seed = random_u32();
                        this.generate(w, cx);
                    }))),
            )
            .child(zoom.wrap(
                ui::pane(is_focused(&self.output, window, cx), &pal)
                    .flex_1()
                    .min_h(px(320.))
                    .child(ui::pane_head("Output", None, &pal).child(ui::copy_btn("mock-copy", self.out.clone(), &pal, window, cx)).child(zoom.button(&pal)))
                    .child(code_editor_el(&self.output, true, cx)),
                &pal,
                window,
            ))
            .child(div().text_size(px(12.)).text_color(pal.text3).child("Emails, sites and IPs use reserved example domains and documentation ranges, so they never reach real people."))
    }
}
