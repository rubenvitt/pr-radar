//! Repositories-Sheet: GitHub-Konto (Anmeldung per Device Flow), Repos hinzufügen und
//! entfernen, Status.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    spinner::Spinner,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::parts::{pipeline_icon, repo_name};
use super::typography::TypeScale as _;
use crate::github::{TokenSource, oauth};
use crate::model::PipelineState;
use crate::radar::{Login, Radar};
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

    /// Beim Öffnen (nach `open_sheet`): alte Fehlermeldung verwerfen und Eingabe fokussieren.
    pub fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.error = None;
        self.input.update(cx, |s, cx| s.focus(window, cx));
    }

    #[cfg(test)]
    pub fn input_focused(&self, window: &Window, cx: &App) -> bool {
        self.input.focus_handle(cx).is_focused(window)
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

    /// GitHub-Konto: Anmeldestatus, „Mit GitHub anmelden“ bzw. der Code zum Eingeben.
    fn render_account(&self, cx: &mut Context<Self>) -> AnyElement {
        let radar = self.radar.read(cx);
        let login = radar.login().clone();
        let source = radar.status().token_source;
        let viewer = radar.snapshot().and_then(|s| s.viewer.clone());
        let muted = cx.theme().muted_foreground;
        let configured = oauth::client_id().is_some();

        let state = match (source, viewer) {
            (Some(TokenSource::OAuth), Some(v)) => format!("Angemeldet als {v}"),
            (Some(TokenSource::OAuth), None) => "Angemeldet".into(),
            (Some(TokenSource::Env), _) => "Token aus GITHUB_TOKEN".into(),
            (Some(TokenSource::GhCli), _) => "Token aus der GitHub-CLI".into(),
            (None, _) => "Nicht angemeldet".into(),
        };

        let error = match &login {
            Login::Failed(e) => Some(e.clone()),
            _ => None,
        };
        let body = match login {
            Login::Waiting {
                user_code,
                verification_uri,
            } => v_flex()
                .gap_2()
                .child(
                    div()
                        .text_body()
                        .child("Gib diesen Code auf GitHub ein (liegt in der Zwischenablage):"),
                )
                .child(
                    div()
                        .text_2xl()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(user_code.clone()),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("open-device")
                                .primary()
                                .small()
                                .label("Code kopieren und GitHub öffnen")
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        user_code.clone(),
                                    ));
                                    cx.open_url(&verification_uri);
                                }),
                        )
                        .child(self.cancel_button(cx)),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .text_small()
                        .text_color(muted)
                        .child(Spinner::new().small())
                        .child("Warte auf die Bestätigung auf GitHub …"),
                )
                .into_any_element(),
            Login::Starting => h_flex()
                .gap_2()
                .text_body()
                .child(Spinner::new().small())
                .child(div().flex_1().child("Verbinde mit GitHub …"))
                .child(self.cancel_button(cx))
                .into_any_element(),
            Login::Idle | Login::Failed(_) => {
                let radar = self.radar.clone();
                h_flex()
                    .gap_2()
                    .child(div().flex_1().text_body().child(state))
                    .when(source != Some(TokenSource::OAuth), |this| {
                        this.child(
                            Button::new("sign-in")
                                .primary()
                                .small()
                                .label("Mit GitHub anmelden")
                                .disabled(!configured)
                                .when(!configured, |b| {
                                    b.tooltip("In diesem Build ist keine OAuth-App hinterlegt")
                                })
                                .on_click(move |_, _, cx| {
                                    radar.update(cx, |r, cx| r.start_login(cx));
                                }),
                        )
                    })
                    .into_any_element()
            }
        };

        v_flex()
            .gap_1()
            .pb_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(div().text_heading().text_color(muted).child("GitHub-Konto"))
            .child(body)
            .when_some(error, |this, e| {
                this.child(div().text_body().text_color(cx.theme().danger).child(e))
            })
            .child(div().text_small().text_color(muted).child(
                "Erweitert: Ohne Anmeldung nutzt PR Radar GITHUB_TOKEN/GH_TOKEN oder die \
                 GitHub-CLI (gh auth login). Die Anmeldung gilt bis zum Beenden der App.",
            ))
            .into_any_element()
    }

    fn cancel_button(&self, _cx: &mut Context<Self>) -> Button {
        let radar = self.radar.clone();
        Button::new("cancel-login")
            .ghost()
            .small()
            .label("Abbrechen")
            .on_click(move |_, _, cx| radar.update(cx, |r, cx| r.cancel_login(cx)))
    }
}

impl Render for RepoSettings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let account = self.render_account(cx);
        let radar = self.radar.read(cx);
        let repos = radar.repos().to_vec();
        let snapshot = radar.snapshot();
        let status = radar.status().clone();
        let empty_input = self.input.read(cx).value().trim().is_empty();
        let muted = cx.theme().muted_foreground;

        v_flex()
            .size_full()
            .gap_4()
            .child(account)
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
                        this.child(div().text_small().text_color(cx.theme().danger).child(e))
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
                                .text_small()
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
                                    .child(div().text_body().child(repo_name(&full, cx)))
                                    .when_some(detail, |this, d| {
                                        this.child(div().text_small().text_color(muted).child(d))
                                    })
                                    .when_some(error, |this, e| {
                                        this.child(
                                            div()
                                                .text_small()
                                                .text_color(cx.theme().danger)
                                                .child(e),
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
                    .text_small()
                    .text_color(muted)
                    .child(format!(
                        "Token: {}",
                        status.token_source.map_or("–", |t| t.label())
                    ))
                    .child(format!(
                        "Aktualisierung alle {} · zuletzt {}",
                        interval_label(status.interval),
                        ago(status.last_success)
                    ))
                    .when_some(status.rate_limit, |this, rl| {
                        this.child(format!("API-Kontingent: {} / {}", rl.remaining, rl.limit))
                    }),
            )
    }
}

/// „30 s“ bzw. bei längeren Pausen (Backoff, Drosselung) „5 Min.“
fn interval_label(interval: std::time::Duration) -> String {
    let secs = interval.as_secs();
    if secs < 120 {
        format!("{secs} s")
    } else {
        format!("{} Min.", secs.div_ceil(60))
    }
}
