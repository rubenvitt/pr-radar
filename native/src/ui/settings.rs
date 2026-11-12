//! Repositories-Sheet: GitHub-Konto (Anmeldung per Device Flow, Konto wechseln), Repos aus
//! dem Konto auswählen oder per Name hinzufügen, beobachtete Repos entfernen, Status.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
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
use crate::radar::{Login, Radar, RepoChoices};
use crate::time::ago;

pub struct RepoSettings {
    radar: Entity<Radar>,
    input: Entity<InputState>,
    /// Suche in der Repo-Auswahl
    search: Entity<InputState>,
    error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl RepoSettings {
    pub fn new(radar: Entity<Radar>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(tr!(
                "owner/repo or GitHub URL",
                "owner/repo oder GitHub-URL"
            ))
        });
        let search = cx.new(|cx| {
            InputState::new(window, cx).placeholder(tr!(
                "Search your repositories",
                "Deine Repositories durchsuchen"
            ))
        });
        let subscriptions = vec![
            cx.subscribe_in(&input, window, |this, _, event, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.add(window, cx);
                }
            }),
            cx.subscribe(&search, |_, _, event, cx| {
                if let InputEvent::Change = event {
                    cx.notify();
                }
            }),
            cx.observe(&radar, |_, _, cx| cx.notify()),
        ];
        Self {
            radar,
            input,
            search,
            error: None,
            _subscriptions: subscriptions,
        }
    }

    /// Beim Öffnen (nach `open_sheet`): alte Fehlermeldung verwerfen und Eingabe fokussieren.
    pub fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.error = None;
        self.input.update(cx, |s, cx| s.focus(window, cx));
        self.radar.update(cx, |r, cx| r.ensure_repo_choices(cx));
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
            (Some(TokenSource::OAuth), Some(v)) => trf!("Signed in as {v}", "Angemeldet als {v}"),
            (Some(TokenSource::OAuth), None) => tr!("Signed in", "Angemeldet").into(),
            (Some(TokenSource::Env), _) => {
                tr!("Token from GITHUB_TOKEN", "Token aus GITHUB_TOKEN").into()
            }
            (Some(TokenSource::GhCli), _) => {
                tr!("Token from the GitHub CLI", "Token aus der GitHub-CLI").into()
            }
            (None, _) => tr!("Not signed in", "Nicht angemeldet").into(),
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
                .child(div().text_body().child(tr!(
                    "Enter this code on GitHub (it’s on the clipboard):",
                    "Gib diesen Code auf GitHub ein (liegt in der Zwischenablage):"
                )))
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
                                .label(tr!(
                                    "Copy Code and Open GitHub",
                                    "Code kopieren und GitHub öffnen"
                                ))
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
                        .child(tr!(
                            "Waiting for confirmation on GitHub…",
                            "Warte auf die Bestätigung auf GitHub …"
                        )),
                )
                .into_any_element(),
            Login::Starting => h_flex()
                .gap_2()
                .text_body()
                .child(Spinner::new().small())
                .child(
                    div()
                        .flex_1()
                        .child(tr!("Connecting to GitHub…", "Verbinde mit GitHub …")),
                )
                .child(self.cancel_button(cx))
                .into_any_element(),
            Login::Idle | Login::Failed(_) => {
                let radar = self.radar.clone();
                h_flex()
                    .gap_2()
                    .child(div().flex_1().text_body().child(state))
                    .when(source == Some(TokenSource::OAuth), |this| {
                        let switch = radar.clone();
                        let radar = radar.clone();
                        this.child(
                            Button::new("switch-account")
                                .small()
                                .ghost()
                                .label(tr!("Switch Account…", "Konto wechseln …"))
                                .tooltip(tr!(
                                    "Signs out and starts a new sign-in. GitHub uses the account \
                                     you’re signed in with in the browser; switch it there first.",
                                    "Meldet ab und startet eine neue Anmeldung. GitHub nimmt das \
                                     Konto, mit dem du im Browser angemeldet bist; wechsle es dort vorher."
                                ))
                                .on_click(move |_, _, cx| {
                                    switch.update(cx, |r, cx| r.switch_account(cx));
                                }),
                        )
                        .child(
                            Button::new("sign-out")
                                .small()
                                .label(tr!("Sign Out", "Abmelden"))
                                .on_click(move |_, _, cx| {
                                    radar.update(cx, |r, cx| r.sign_out(cx));
                                }),
                        )
                    })
                    .when(source != Some(TokenSource::OAuth), |this| {
                        this.child(
                            Button::new("sign-in")
                                .primary()
                                .small()
                                .label(tr!("Sign In with GitHub", "Mit GitHub anmelden"))
                                .disabled(!configured)
                                .when(!configured, |b| {
                                    b.tooltip(tr!(
                                        "No OAuth app is configured in this build",
                                        "In diesem Build ist keine OAuth-App hinterlegt"
                                    ))
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
            .child(
                div()
                    .text_heading()
                    .text_color(muted)
                    .child(tr!("GitHub Account", "GitHub-Konto")),
            )
            .child(body)
            .when_some(error, |this, e| {
                this.child(div().text_body().text_color(cx.theme().danger).child(e))
            })
            .child(div().text_small().text_color(muted).child(tr!(
                "Advanced: Without signing in, PR Radar uses GITHUB_TOKEN/GH_TOKEN or the \
                     GitHub CLI (gh auth login). The sign-in is kept in your keychain until you sign out.",
                "Erweitert: Ohne Anmeldung nutzt PR Radar GITHUB_TOKEN/GH_TOKEN oder die \
                     GitHub-CLI (gh auth login). Die Anmeldung bleibt im Schlüsselbund, bis du dich abmeldest."
            )))
            .into_any_element()
    }

    /// Repos aus dem Konto zum Anhaken, mit Suche. Jedes Häkchen gilt sofort.
    fn render_choices(&self, cx: &mut Context<Self>) -> AnyElement {
        let radar = self.radar.read(cx);
        let choices = radar.repo_choices().clone();
        let signed_in = radar.status().token_source.is_some();
        let logging_in = !matches!(radar.login(), Login::Idle | Login::Failed(_));
        let watched: Vec<String> = radar.repos().iter().map(|r| r.to_lowercase()).collect();
        let muted = cx.theme().muted_foreground;
        let filter = self.search.read(cx).value().trim().to_lowercase();

        let reload = Button::new("reload-choices")
            .ghost()
            .small()
            .icon(Icon::new(gpui_kit::component::IconName::RefreshCw))
            .tooltip(tr!("Reload List", "Liste neu laden"))
            .on_click(cx.listener(|this, _, _, cx| {
                this.radar.update(cx, |r, cx| r.load_repo_choices(cx));
            }));
        let header = h_flex()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .text_heading()
                    .text_color(muted)
                    .child(tr!("Your Repositories", "Deine Repositories")),
            )
            .when(
                matches!(choices, RepoChoices::Loaded(_) | RepoChoices::Failed(_)) && signed_in,
                |this| this.child(reload),
            );

        let body = match choices {
            RepoChoices::Idle | RepoChoices::Failed(_) if !signed_in || logging_in => div()
                .text_body()
                .text_color(muted)
                .child(tr!(
                    "Sign in above to pick from your repositories and those of your organizations.",
                    "Melde dich oben an, dann kannst du aus deinen Repositories und denen deiner \
                     Organisationen wählen."
                ))
                .into_any_element(),
            RepoChoices::Idle | RepoChoices::Loading => h_flex()
                .gap_2()
                .text_body()
                .text_color(muted)
                .child(Spinner::new().small())
                .child(tr!(
                    "Loading your repositories…",
                    "Lade deine Repositories …"
                ))
                .into_any_element(),
            RepoChoices::Failed(e) => div()
                .text_body()
                .text_color(cx.theme().danger)
                .child(e)
                .into_any_element(),
            RepoChoices::Loaded(list) if list.is_empty() => div()
                .text_body()
                .text_color(muted)
                .child(tr!(
                    "No repositories found in this account. Add one by name below.",
                    "In diesem Konto gibt es keine Repositories. Füge unten eins per Name hinzu."
                ))
                .into_any_element(),
            RepoChoices::Loaded(list) => {
                let shown: Vec<_> = list
                    .iter()
                    .filter(|c| filter.is_empty() || c.full.to_lowercase().contains(&filter))
                    .cloned()
                    .collect();
                let rows = shown.into_iter().map(|choice| {
                    let checked = watched.contains(&choice.full.to_lowercase());
                    let full = choice.full.clone();
                    v_flex()
                        .py_1()
                        .gap_0p5()
                        .child(
                            h_flex()
                                .gap_2()
                                .child(
                                    Checkbox::new(SharedString::from(format!(
                                        "choice-{}",
                                        choice.full
                                    )))
                                    .checked(checked)
                                    .label(choice.full.clone())
                                    .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                                        let result = this
                                            .radar
                                            .update(cx, |r, cx| r.set_watched(&full, *checked, cx));
                                        this.error = result.err().map(|e| e.to_string());
                                        cx.notify();
                                    })),
                                )
                                .when(choice.private, |this| {
                                    this.child(
                                        div()
                                            .text_small()
                                            .text_color(muted)
                                            .child(tr!("private", "privat")),
                                    )
                                }),
                        )
                        .when_some(choice.description.clone(), |this, d| {
                            this.child(
                                div()
                                    .pl_6()
                                    .text_small()
                                    .text_color(muted)
                                    .truncate()
                                    .child(d),
                            )
                        })
                });
                let count = list.len();
                v_flex()
                    .gap_2()
                    .child(Input::new(&self.search).small().cleanable(true))
                    .child(
                        div()
                            .id("repo-choices")
                            .max_h(px(260.))
                            .overflow_y_scroll()
                            .child(v_flex().children(rows)),
                    )
                    .child(div().text_small().text_color(muted).child(trf!(
                        "{count} repositories, most recently active first. Organization repos \
                         only appear if the organization allows PR Radar.",
                        "{count} Repositories, zuletzt aktive zuerst. Repos einer Organisation \
                         erscheinen nur, wenn sie PR Radar zugelassen hat."
                    )))
                    .into_any_element()
            }
        };

        v_flex()
            .gap_2()
            .pb_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(header)
            .child(body)
            .into_any_element()
    }

    fn cancel_button(&self, _cx: &mut Context<Self>) -> Button {
        let radar = self.radar.clone();
        Button::new("cancel-login")
            .ghost()
            .small()
            .label(tr!("Cancel", "Abbrechen"))
            .on_click(move |_, _, cx| radar.update(cx, |r, cx| r.cancel_login(cx)))
    }
}

impl Render for RepoSettings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let account = self.render_account(cx);
        let choices = self.render_choices(cx);
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
            .child(choices)
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_small().text_color(muted).child(tr!(
                        "Another repository by name, e.g. a public one:",
                        "Weiteres Repository per Name, z. B. ein öffentliches:"
                    )))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(div().flex_1().child(Input::new(&self.input)))
                            .child(
                                Button::new("add-repo")
                                    .icon(Icon::new(gpui_kit::component::IconName::Plus))
                                    .label(tr!("Add", "Hinzufügen"))
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
                                .child(tr!(
                                    "No repos yet – add one above.",
                                    "Noch keine Repos – füge oben eins hinzu."
                                )),
                        )
                    })
                    .children(repos.into_iter().map(|full| {
                        let info = snapshot.as_ref().and_then(|s| s.repo(&full).cloned());
                        let detail = match &info {
                            Some(i) if i.error.is_some() => None,
                            Some(i) => Some(format!(
                                "{} · {}",
                                i.default_branch.clone().unwrap_or_else(|| "–".into()),
                                if i.auto_merge_allowed {
                                    tr!("Auto-merge allowed", "Auto-Merge erlaubt")
                                } else {
                                    tr!("Auto-merge off", "Auto-Merge aus")
                                }
                            )),
                            None => Some(tr!("loading…", "wird geladen …").into()),
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
                                    .tooltip(trf!("Remove {full}", "{full} entfernen"))
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
                    .child(trf!(
                        "Updates every {} · last {}",
                        "Aktualisierung alle {} · zuletzt {}",
                        interval_label(status.interval),
                        ago(status.last_success)
                    ))
                    .when_some(status.rate_limit, |this, rl| {
                        this.child(trf!(
                            "API quota: {} / {}",
                            "API-Kontingent: {} / {}",
                            rl.remaining,
                            rl.limit
                        ))
                    }),
            )
    }
}

/// „30 s“ bzw. bei längeren Pausen (Backoff, Drosselung) „5 min“ / „5 Min.“
fn interval_label(interval: std::time::Duration) -> String {
    let secs = interval.as_secs();
    if secs < 120 {
        format!("{secs} s")
    } else {
        trf!("{} min", "{} Min.", secs.div_ceil(60))
    }
}
