use gpui_kit::component::input::TextareaState;
use gpui_kit::{
    Context, Entity, FontWeight, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window, div, prelude::FluentBuilder, px,
};
use serde_json::{Value, json};

use super::date::{medium, relative};
use super::*;
use crate::logic::{self, plural};
use crate::ui::{self, Tone};

struct Decoded {
    header: String,
    payload: String,
    alg: String,
    claims: usize,
    sig_len: usize,
    state: (&'static str, Tone),
    rows: Vec<(&'static str, String, String, Tone)>,
}

fn decode(raw: &str) -> Result<Option<Decoded>, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    let parts: Vec<&str> = raw.split('.').collect();
    if parts.len() != 3 {
        return Err(format!("A JWT has three dot-separated parts. Found {}.", parts.len()));
    }
    let bad = || "This token could not be decoded. Check that it is a valid Base64URL-encoded JWT.".to_string();
    let part = |p: &str| -> Result<Value, String> {
        let bytes = logic::b64url_decode(p).map_err(|_| bad())?;
        serde_json::from_slice(&bytes).map_err(|_| bad())
    };
    let h = part(parts[0])?;
    let p = part(parts[1])?;
    let now = chrono::Utc::now().timestamp() as f64;
    let num = |k: &str| p.get(k).and_then(Value::as_f64);
    let expired = num("exp").is_some_and(|e| e < now);
    let early = num("nbf").is_some_and(|n| n > now);

    let mut rows = Vec::new();
    for (k, label) in [("iss", "Issuer"), ("sub", "Subject"), ("aud", "Audience"), ("iat", "Issued at"), ("nbf", "Not before"), ("exp", "Expires")] {
        let Some(v) = p.get(k) else { continue };
        let is_time = matches!(k, "iat" | "nbf" | "exp");
        if is_time && let Some(t) = v.as_f64() {
            let tone = match k {
                "exp" => if expired { Tone::Err } else { Tone::Ok },
                "nbf" if early => Tone::Err,
                _ => Tone::Info,
            };
            rows.push((label, medium(t), relative((t * 1000.) as i64), tone));
        } else {
            let value = match v {
                Value::String(s) => s.clone(),
                Value::Array(a) => a.iter().map(|x| x.as_str().map(String::from).unwrap_or_else(|| x.to_string())).collect::<Vec<_>>().join(", "),
                other => other.to_string(),
            };
            rows.push((label, value, k.to_string(), Tone::Neutral));
        }
    }
    Ok(Some(Decoded {
        header: serde_json::to_string_pretty(&h).unwrap_or_default(),
        payload: serde_json::to_string_pretty(&p).unwrap_or_default(),
        alg: h.get("alg").and_then(Value::as_str).unwrap_or("none").to_string(),
        claims: p.as_object().map_or(0, |o| o.len()),
        sig_len: parts[2].len(),
        state: if expired {
            ("Expired", Tone::Err)
        } else if early {
            ("Not yet valid", Tone::Err)
        } else {
            ("Active", Tone::Ok)
        },
        rows,
    }))
}

fn sample() -> String {
    let now = chrono::Utc::now().timestamp();
    format!(
        "{}.{}.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c",
        logic::b64url_json(&json!({"alg": "HS256", "typ": "JWT"})),
        logic::b64url_json(&json!({"sub": "1234567890", "name": "Jane Doe", "role": "admin", "iss": "auth.example.com", "iat": now - 3600, "exp": now + 86400}))
    )
}

pub struct JwtView {
    token: Entity<TextareaState>,
    header: Entity<TextareaState>,
    payload: Entity<TextareaState>,
    wrap: bool,
    _subs: Vec<Subscription>,
}

impl JwtView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let token = editor(&sample(), "Paste a JSON Web Token", window, cx);
        token.update(cx, |s, cx| s.set_soft_wrap(true, window, cx));
        let header = editor("", "", window, cx);
        let payload = editor("", "", window, cx);
        let subs = vec![
            watch(&token, window, cx, Self::recompute),
            watch(&header, window, cx, |_, _, _| {}),
            watch(&payload, window, cx, |_, _, _| {}),
        ];
        let mut this = Self { token, header, payload, wrap: false, _subs: subs };
        this.recompute(window, cx);
        this
    }

    pub fn set_input(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        set_text(&self.token, text, window, cx);
        self.recompute(window, cx);
    }

    fn recompute(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (h, p) = match decode(&text_of(&self.token, cx)) {
            Ok(Some(d)) => (d.header, d.payload),
            _ => (String::new(), String::new()),
        };
        set_text(&self.header, &h, window, cx);
        set_text(&self.payload, &p, window, cx);
        cx.notify();
    }
}

impl Render for JwtView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        sync_wrap(&[&self.header, &self.payload], &mut self.wrap, window, cx);
        let (decoded, err) = match decode(&text_of(&self.token, cx)) {
            Ok(d) => (d, None),
            Err(e) => (None, Some(e)),
        };
        let [paste, clear] = paste_clear("jwt", &self.token, &pal, cx, Self::recompute);

        let dot_badge = |color, label: String| {
            ui::badge_base(Tone::Neutral, &pal).child(div().text_color(color).child("●")).child(label)
        };

        let mut root = div().flex().flex_col().gap(px(10.)).child(
            ui::pane(is_focused(&self.token, window, cx), &pal)
                .h(px(150.))
                .flex_none()
                .child(
                    ui::pane_head("Token", None, &pal)
                        .when_some(decoded.as_ref(), |d, dec| d.child(ui::badge(dec.state.0, dec.state.1, &pal).mr(px(6.))))
                        .child(paste)
                        .child(clear),
                )
                .child(editor_el(&self.token, false, cx)),
        );
        if let Some(d) = &decoded {
            root = root.child(
                div()
                    .flex()
                    .gap(px(8.))
                    .items_center()
                    .child(dot_badge(pal.tok_h, format!("Header · {}", d.alg)))
                    .child(dot_badge(pal.tok_p, format!("Payload · {}", plural(d.claims, "claim"))))
                    .child(dot_badge(pal.tok_s, if d.sig_len > 0 { format!("Signature · {}", plural(d.sig_len, "char")) } else { "Signature · empty".into() })),
            );
        }
        if let Some(e) = err {
            root = root.child(ui::err_box(e, &pal));
        }
        let header_text: SharedString = decoded.as_ref().map(|d| d.header.clone()).unwrap_or_default().into();
        let payload_text: SharedString = decoded.as_ref().map(|d| d.payload.clone()).unwrap_or_default().into();
        root = root.child(
            div()
                .flex()
                .gap(px(12.))
                .child(
                    ui::pane(is_focused(&self.header, window, cx), &pal)
                        .flex_1()
                        .h(px(200.))
                        .child(ui::pane_head("Header", Some(pal.tok_h), &pal).child(ui::copy_btn("jwt-h", header_text, &pal, window, cx)))
                        .child(editor_el(&self.header, true, cx)),
                )
                .child(
                    ui::pane(is_focused(&self.payload, window, cx), &pal)
                        .flex_1()
                        .h(px(200.))
                        .child(ui::pane_head("Payload", Some(pal.tok_p), &pal).child(ui::copy_btn("jwt-p", payload_text, &pal, window, cx)))
                        .child(editor_el(&self.payload, true, cx)),
                ),
        );
        if let Some(d) = decoded.filter(|d| !d.rows.is_empty()) {
            let n = d.rows.len();
            root = root.child(ui::section_label("Registered claims", &pal).mt(px(6.))).child(
                ui::kv_card(&pal).children(d.rows.into_iter().enumerate().map(|(i, (label, value, badge, tone))| {
                    ui::kv_row(("claim", i), i + 1 == n, &pal)
                        .child(ui::kv_label(label, 150., &pal))
                        .child(ui::kv_val(value, false))
                        .child(ui::badge(badge, tone, &pal).mr(px(8.)).font_weight(FontWeight::SEMIBOLD))
                })),
            );
        }
        root
    }
}
