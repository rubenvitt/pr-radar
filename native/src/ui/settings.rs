//! Repositories verwalten (Inhalt der Seitenleiste-Sheet): hinzufügen, entfernen, Status.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::parts::{pipeline_icon, repo_name};
use crate::model::PipelineState;
use crate::radar::Radar;
use crate::time::ago;

pub struct RepoSettings {
    radar: Entity<Radar>,
    input: Entity<InputState>,
    error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl RepoSettings {
    pub fn new(radar: Entity<Radar>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder("owner/repo oder GitHub-URL"));
        let subscriptions = vec![
            cx.subscribe_in(&input, window, |this, _, event, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.add(window, cx);
                }
            }),
            cx.observe(&radar, |_, _, cx| cx.notify()),
        ];
        Self {
            radar,
            input,
            error: None,
            _subscriptions: subscriptions,
        }
    }

    /// Vor dem Öffnen: alte Fehlermeldung verwerfen und Eingabe fokussieren.
    pub fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.error = None;
        self.input.update(cx, |s, cx| s.focus(window, cx));
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.input.read(cx).value().to_string();
        if value.trim().is_empty() {
            return;
        }
        match self.radar.update(cx, |r, cx| r.add_repo(&value, cx)) {
            Ok(()) => {
                self.error = None;
                self.input.update(cx, |s, cx| s.set_value("", window, cx));
            }
            Err(e) => self.error = Some(e.to_string()),
        }
        cx.notify();
    }
}

impl Render for RepoSettings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let radar = self.radar.read(cx);
        let repos = radar.repos().to_vec();
        let snapshot = radar.snapshot();
        let status = radar.status().clone();
        let empty_input = self.input.read(cx).value().trim().is_empty();
        let muted = cx.theme().muted_foreground;

        v_flex()
            .size_full()
            .gap_4()
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(div().flex_1().child(Input::new(&self.input)))
                            .child(
                                Button::new("add-repo")
                                    .icon(Icon::new(gpui_kit::component::IconName::Plus))
                                    .label("Hinzufügen")
                                    .disabled(empty_input)
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.add(window, cx)),
                                    ),
                            ),
                    )
                    .when_some(self.error.clone(), |this, e| {
                        this.child(div().text_sm().text_color(cx.theme().danger).child(e))
                    }),
            )
            .child(
                v_flex()
                    .flex_1()
                    .gap_0p5()
                    .when(repos.is_empty(), |this| {
                        this.child(
                            div()
                                .py_6()
                                .text_sm()
                                .text_color(muted)
                                .text_center()
                                .child("Noch keine Repos – füge oben eins hinzu."),
                        )
                    })
                    .children(repos.into_iter().map(|full| {
                        let info = snapshot.as_ref().and_then(|s| s.repo(&full).cloned());
                        let detail = match &info {
                            Some(i) if i.error.is_some() => None,
                            Some(i) => Some(format!(
                                "{} · Auto-Merge {}",
                                i.default_branch.clone().unwrap_or_else(|| "–".into()),
                                if i.auto_merge_allowed {
                                    "erlaubt"
                                } else {
                                    "aus"
                                }
                            )),
                            None => Some("wird geladen …".into()),
                        };
                        let error = info.as_ref().and_then(|i| i.error.clone());
                        let state = info
                            .as_ref()
                            .map_or(PipelineState::None, |i| i.default_branch_pipeline);
                        let radar = self.radar.clone();
                        let key = full.clone();
                        h_flex()
                            .gap_3()
                            .px_2()
                            .py_2()
                            .rounded(cx.theme().radius)
                            .hover(|s| s.bg(cx.theme().list_hover))
                            .child(pipeline_icon(state, cx))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .child(div().text_sm().child(repo_name(&full, cx)))
                                    .when_some(detail, |this, d| {
                                        this.child(div().text_xs().text_color(muted).child(d))
                                    })
                                    .when_some(error, |this, e| {
                                        this.child(
                                            div().text_xs().text_color(cx.theme().danger).child(e),
                                        )
                                    }),
                            )
                            .child(
                                Button::new(SharedString::from(format!("remove-{full}")))
                                    .ghost()
                                    .small()
                                    .icon(Icon::new(Lucide::Trash))
                                    .tooltip(format!("{full} entfernen"))
                                    .on_click(move |_, _, cx| {
                                        let _ = radar.update(cx, |r, cx| r.remove_repo(&key, cx));
                                    }),
                            )
                    })),
            )
            .child(
                v_flex()
                    .gap_1()
                    .pt_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .text_xs()
                    .text_color(muted)
                    .child(format!(
                        "Token: {}",
                        status.token_source.map_or("–", |t| t.label())
                    ))
                    .child(format!(
                        "Aktualisierung alle {} s · zuletzt {}",
                        status.interval.as_secs(),
                        ago(status.last_success)
                    ))
                    .when_some(status.rate_limit, |this, rl| {
                        this.child(format!("API-Kontingent: {} / {}", rl.remaining, rl.limit))
                    }),
            )
    }
}
