use gpui_kit::component::input::{Input, InputState, TextareaState};
use gpui_kit::{
    Context, Entity, FontWeight, HighlightStyle, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement, StyledText, Styled, Subscription, UnderlineStyle,
    Window, div, prelude::FluentBuilder, px, relative, InteractiveElement,
};

use super::*;
use crate::logic::plural;
use crate::theme::MONO_FONT;
use crate::ui::{self, Tone};

const FLAGS: [(&str, &str); 4] = [("g", "Global"), ("i", "Ignore case"), ("m", "Multiline"), ("s", "Dot matches newline")];

pub struct Match {
    pub range: std::ops::Range<usize>,
    pub text: String,
    pub groups: String,
}

/// Run `pattern` over `text` with JavaScript-style flags. Returns at most 500 matches.
pub fn run(pattern: &str, flags: [bool; 4], text: &str) -> Result<Vec<Match>, String> {
    let mut prefix = String::new();
    if flags[1] { prefix.push('i') }
    if flags[2] { prefix.push('m') }
    if flags[3] { prefix.push('s') }
    let full = if prefix.is_empty() { pattern.to_string() } else { format!("(?{prefix}){pattern}") };
    let re = fancy_regex::Regex::new(&full).map_err(|e| format!("Invalid regular expression: {e}"))?;
    let mut out = Vec::new();
    for caps in re.captures_iter(text) {
        let caps = caps.map_err(|e| e.to_string())?;
        let m = caps.get(0).unwrap();
        if m.as_str().is_empty() {
            continue;
        }
        let groups = if caps.len() > 1 {
            (1..caps.len())
                .map(|i| format!("${i} {}", caps.get(i).map_or("∅", |g| g.as_str())))
                .collect::<Vec<_>>()
                .join("  ·  ")
        } else {
            "No groups".into()
        };
        out.push(Match { range: m.range(), text: m.as_str().to_string(), groups });
        if !flags[0] || out.len() >= 500 {
            break;
        }
    }
    Ok(out)
}

pub struct RegexView {
    pattern: Entity<InputState>,
    text: Entity<TextareaState>,
    flags: [bool; 4],
    _subs: Vec<Subscription>,
}

impl RegexView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let pattern = line(r"([\w.]+)@(\w+)\.com", "Pattern", window, cx);
        let text = editor(
            "Contact alice@example.com or bob.smith@sidekit.com for access.\nNot matched: carol@site.org",
            "Text to test",
            window,
            cx,
        );
        text.update(cx, |s, cx| s.set_soft_wrap(true, window, cx));
        let subs = vec![
            watch(&pattern, window, cx, |_, _, cx| cx.notify()),
            watch(&text, window, cx, |_, _, cx| cx.notify()),
        ];
        Self { pattern, text, flags: [true, true, false, false], _subs: subs }
    }
}

impl Render for RegexView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Pal::get(cx);
        let pattern = line_text(&self.pattern, cx);
        let text = text_of(&self.text, cx);
        let (matches, err) = if pattern.is_empty() {
            (Vec::new(), None)
        } else {
            match run(&pattern, self.flags, &text) {
                Ok(m) => (m, None),
                Err(e) => (Vec::new(), Some(e)),
            }
        };
        let flag_str: String = FLAGS.iter().zip(self.flags).filter(|(_, on)| *on).map(|((k, _), _)| *k).collect();
        let (count_label, count_tone) = if err.is_some() {
            ("Invalid pattern".to_string(), Tone::Err)
        } else if matches.is_empty() {
            (plural(0, "match"), Tone::Neutral)
        } else {
            (plural(matches.len(), "match"), Tone::Info)
        };

        let highlight = HighlightStyle {
            background_color: Some(pal.mark),
            underline: Some(UnderlineStyle { thickness: px(2.), color: Some(pal.accent), wavy: false }),
            ..Default::default()
        };
        let styled = StyledText::new(SharedString::from(text.clone()))
            .with_highlights(matches.iter().map(|m| (m.range.clone(), highlight)));
        let pattern_focused = is_focused(&self.pattern, window, cx);
        let [paste, clear] = paste_clear("rx", &self.text, &pal, cx, |_: &mut Self, _, cx| cx.notify());
        let fs = crate::settings::Settings::get(cx).font_size as f32;
        let zoom = ui::PaneZoom::new("regex-matches", window, cx);

        let flag_btns = FLAGS.iter().enumerate().map(|(i, (k, title))| {
            let on = self.flags[i];
            let p = pal;
            div()
                .id(("rx-flag", i))
                .w(px(40.))
                .h(px(44.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .border_1()
                .font_family(MONO_FONT)
                .text_size(px(13.))
                .cursor_pointer()
                .when(on, |d| d.bg(p.accent_soft).border_color(p.accent).text_color(p.accent).font_weight(FontWeight::SEMIBOLD))
                .when(!on, |d| d.border_color(p.stroke_strong).text_color(p.text2).hover(move |s| s.bg(p.subtle)))
                .tooltip(move |w, cx| gpui_kit::component::tooltip::Tooltip::new(*title).build(w, cx))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.flags[i] = !this.flags[i];
                    cx.notify();
                }))
                .child(*k)
        });

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .flex_1()
            .child(ui::section_label("Regular expression", &pal))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .items_center()
                    .child(
                        ui::field(pattern_focused, &pal)
                            .flex_1()
                            .h(px(44.))
                            .px(px(12.))
                            .gap(px(4.))
                            .font_family(MONO_FONT)
                            .text_size(px(15.))
                            .child(div().text_color(pal.text3).child("/"))
                            .child(
                                div().flex_1().min_w_0().child(
                                    Input::new(&self.pattern).appearance(false).text_size(px(15.)).font_family(MONO_FONT),
                                ),
                            )
                            .child(div().text_color(pal.text3).child(format!("/{flag_str}"))),
                    )
                    .children(flag_btns),
            )
            .when_some(err, |d, e| d.child(ui::err_box(e, &pal).mt(px(4.))))
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .flex_1()
                    .min_h(px(250.))
                    .mt(px(10.))
                    .child(
                        ui::pane(is_focused(&self.text, window, cx), &pal)
                            .flex_1()
                            .child(ui::pane_head("Text", None, &pal).child(paste).child(clear))
                            .child(editor_el(&self.text, false, cx)),
                    )
                    .child(zoom.wrap(
                        ui::pane(false, &pal)
                            .flex_1()
                            .child(ui::pane_head("Matches", None, &pal)
                                .child(ui::badge(count_label, count_tone, &pal).mr(px(8.)))
                                .child(zoom.button(&pal)))
                            .child(
                                div()
                                    .id("rx-out")
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scroll()
                                    .px(px(14.))
                                    .py(px(12.))
                                    .font_family(MONO_FONT)
                                    .text_size(px(fs))
                                    .line_height(relative(1.65))
                                    .child(styled),
                            ),
                        &pal, window,
                    )),
            )
            .when(!matches.is_empty(), |d| {
                let n = matches.len();
                d.child(ui::section_label("Match details", &pal).mt(px(10.))).child(ui::kv_card(&pal).children(
                    matches.iter().enumerate().map(|(i, m)| {
                        ui::kv_row(("rx-m", i), i + 1 == n, &pal)
                            .child(ui::kv_label(format!("#{}", i + 1), 44., &pal).text_color(pal.text3))
                            .child(ui::kv_val(m.text.clone(), false).text_color(pal.accent).flex_grow(1.2))
                            .child(ui::kv_val(m.groups.clone(), false).text_color(pal.text2).text_size(px(12.)))
                            .child(
                                div()
                                    .w(px(110.))
                                    .flex_none()
                                    .flex()
                                    .justify_end()
                                    .pr(px(10.))
                                    .text_size(px(12.))
                                    .text_color(pal.text3)
                                    .child(format!("{}–{}", m.range.start, m.range.end)),
                            )
                    }),
                ))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::run;

    #[test]
    fn finds_design_sample_matches() {
        let text = "Contact alice@example.com or bob.smith@sidekit.com for access.\nNot matched: carol@site.org";
        let m = run(r"([\w.]+)@(\w+)\.com", [true, true, false, false], text).unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].groups, "$1 alice  ·  $2 example");
        let first = run(r"\w+", [false, false, false, false], text).unwrap();
        assert_eq!(first.len(), 1);
        assert!(run("(", [true; 4], text).is_err());
    }
}
