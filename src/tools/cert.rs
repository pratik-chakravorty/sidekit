//! Decode X.509 certificates, one or a whole chain.

use gpui_kit::component::input::TextareaState;
use gpui_kit::{
    Context, Entity, FontWeight, IntoElement, ParentElement, Render, Styled, Subscription, Window, div,
    prelude::FluentBuilder, px,
};

use super::*;
use crate::logic::cert;
use crate::ui::{self, Tone};

pub struct CertView {
    input: Entity<TextareaState>,
    wrap: bool,
    _subs: Vec<Subscription>,
}

impl CertView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = editor("", "Paste a PEM certificate or chain (-----BEGIN CERTIFICATE-----), or Base64 DER", window, cx);
        let subs = vec![watch(&input, window, cx, |_, _, cx| cx.notify())];
        Self { input, wrap: false, _subs: subs }
    }

    pub fn set_input(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        set_text(&self.input, text, window, cx);
        cx.notify();
    }
}

impl Render for CertView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        sync_wrap(&[&self.input], &mut self.wrap, window, cx);
        let src = text_of(&self.input, cx);
        let [paste, clear] = paste_clear("cert", &self.input, &pal, cx, |_: &mut Self, _, cx| cx.notify());

        let mut out = div().flex().flex_col().gap(px(6.));
        if !src.trim().is_empty() {
            match cert::decode(&src) {
                Ok(certs) => {
                    let many = certs.len() > 1;
                    for (ci, c) in certs.into_iter().enumerate() {
                        let (status, tone) = if c.not_yet_valid {
                            ("Not yet valid".to_string(), Tone::Err)
                        } else if c.days_left < 0 {
                            (format!("Expired {} days ago", -c.days_left), Tone::Err)
                        } else if c.days_left < 30 {
                            (format!("Expires in {} days", c.days_left), Tone::Err)
                        } else {
                            (format!("Valid · {} days left", c.days_left), Tone::Ok)
                        };
                        let n = c.rows.len();
                        let mut card = ui::kv_card(&pal);
                        for (i, (k, v)) in c.rows.into_iter().enumerate() {
                            card = card.child(ui::kv_copy_row(crate::id!("cert-{ci}-{i}"), k, 170., v.into(), true, i + 1 == n, &pal, window, cx));
                        }
                        out = out
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(10.))
                                    .mt(px(14.))
                                    .when(many, |d| d.child(div().text_size(px(12.)).text_color(pal.text3).child(format!("#{}", ci + 1))))
                                    .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child(c.title))
                                    .child(ui::badge(status, tone, &pal)),
                            )
                            .child(card);
                    }
                }
                Err(e) => out = out.child(ui::err_box(e, &pal).mt(px(8.))),
            }
        }

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                ui::pane(is_focused(&self.input, window, cx), &pal)
                    .h(px(200.))
                    .child(ui::pane_head("Certificate", None, &pal).child(paste).child(clear))
                    .child(editor_el(&self.input, false, cx)),
            )
            .child(div().text_size(px(12.)).text_color(pal.text3).child("Decoded on this computer. Private keys are refused and never needed."))
            .child(out)
    }
}
