//! Einträge für „Gemergt“ (Tages-Timeline) und „Releases“ (mit gerenderten Release Notes).

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    tag::Tag,
    text::{TextView, TextViewStyle},
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::Workspace;
use super::parts::{avatar, card_segment, meta_row, repo_name};
use super::theme;
use super::typography::{self, TypeScale as _};
use crate::model::{MergedPr, Release};
use crate::time::{ago, clock, full};

/// Höhe, auf die aufgeklappte Release Notes gekappt werden, bis „Mehr anzeigen“ geklickt ist.
const NOTES_MAX_HEIGHT: f32 = 16.;
/// Höhe des Verlaufs, der gekappte Notes nach unten ausblendet.
const NOTES_FADE_HEIGHT: f32 = 2.5;

/// Stil der Release Notes: Überschriften in derselben Größe wie der Release-Name (Grundschrift,
/// nur fett), damit keine größer als der Name wird und es bei den drei Schriftstufen aus PRR-50
/// bleibt. Absätze und Listen enger als im Standard (1 rem).
/// `name` ist die Schriftgröße des Release-Namens.
fn notes_style(name: Pixels) -> TextViewStyle {
    TextViewStyle::default()
        .paragraph_gap(rems(0.5))
        .heading_font_size(move |_, _| name)
}

impl Workspace {
    pub(super) fn render_merged_row(
        &self,
        m: &MergedPr,
        first: bool,
        last: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let url = m.url.clone();
        let muted = cx.theme().muted_foreground;
        card_segment(first, last, cx)
            .child(
                h_flex()
                    .id(SharedString::from(format!("merged-{}", m.id)))
                    .gap_3()
                    .px_4()
                    .py_3()
                    .cursor_pointer()
                    .hover(|s| s.bg(cx.theme().list_hover))
                    .on_click(move |_, _, cx| cx.open_url(&url))
                    .child(
                        Icon::new(Lucide::GitMerge)
                            .small()
                            .text_color(theme::merged(cx)),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .min_w_0()
                                    .child(div().font_medium().truncate().child(m.title.clone()))
                                    .child(
                                        div()
                                            .text_small()
                                            .text_color(muted)
                                            .child(format!("#{}", m.number)),
                                    ),
                            )
                            .child(
                                meta_row(cx)
                                    .child(repo_name(&m.repo, cx))
                                    .child(
                                        h_flex().gap_1p5().child(avatar(m.author.as_ref())).child(
                                            m.author
                                                .as_ref()
                                                .map_or("ghost".into(), |a| a.login.clone()),
                                        ),
                                    )
                                    .when_some(
                                        m.merged_by.clone().filter(|b| {
                                            Some(b) != m.author.as_ref().map(|a| &a.login)
                                        }),
                                        |this, by| {
                                            this.child(trf!("merged by {by}", "gemergt von {by}"))
                                        },
                                    )
                                    .child(
                                        div()
                                            .font_family(cx.theme().mono_font_family.clone())
                                            .child(format!("→ {}", m.base_ref)),
                                    ),
                            ),
                    )
                    .child(
                        v_flex()
                            .items_end()
                            .text_small()
                            .text_color(muted)
                            .child(clock(m.merged_at))
                            .child(ago(Some(m.merged_at))),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_release(
        &self,
        r: &Release,
        default_open: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let open = default_open != self.toggled_releases.contains(&r.id);
        let has_body = !r.description_html.trim().is_empty();
        let id = r.id.clone();
        let url = r.url.clone();
        let muted = cx.theme().muted_foreground;

        card_segment(true, true, cx)
            .child(
                h_flex()
                    .id(SharedString::from(format!("release-{}", r.id)))
                    .gap_3()
                    .px_4()
                    .py_3()
                    .when(has_body, |this| {
                        this.hover(|s| s.bg(cx.theme().list_hover))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.toggled_releases.remove(&id) {
                                    this.toggled_releases.insert(id.clone());
                                }
                                this.remeasure_key(&format!("release:{id}"));
                                cx.notify();
                            }))
                    })
                    .child(
                        Icon::new(Lucide::Tag)
                            .small()
                            .text_color(cx.theme().primary),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                h_flex()
                                    .flex_wrap()
                                    .gap_2()
                                    .child(div().font_semibold().child(r.name.clone()))
                                    .when(r.name != r.tag_name, |this| {
                                        this.child(
                                            div()
                                                .text_small()
                                                .text_color(muted)
                                                .font_family(cx.theme().mono_font_family.clone())
                                                .child(r.tag_name.clone()),
                                        )
                                    })
                                    .when(r.is_latest, |this| {
                                        this.child(
                                            Tag::success().xsmall().text_small().child("Latest"),
                                        )
                                    })
                                    .when(r.is_prerelease, |this| {
                                        this.child(
                                            Tag::warning()
                                                .xsmall()
                                                .text_small()
                                                .child("Pre-Release"),
                                        )
                                    }),
                            )
                            .child(
                                meta_row(cx)
                                    .child(repo_name(&r.repo, cx))
                                    .when_some(r.author.clone(), |this, a| {
                                        this.child(trf!("by {a}", "von {a}"))
                                    })
                                    .child(
                                        div()
                                            .id(SharedString::from(format!(
                                                "release-time-{}",
                                                r.id
                                            )))
                                            .child(ago(r.published_at))
                                            .when_some(r.published_at, |this, t| {
                                                let text: SharedString = full(t).into();
                                                this.tooltip(move |window, cx| {
                                                    gpui_kit::component::tooltip::Tooltip::new(
                                                        text.clone(),
                                                    )
                                                    .build(window, cx)
                                                })
                                            }),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(
                                Button::new(SharedString::from(format!("release-open-{}", r.id)))
                                    .ghost()
                                    .small()
                                    .icon(IconName::ExternalLink)
                                    .tooltip(tr!("Open on GitHub", "Auf GitHub öffnen"))
                                    .on_click(move |_, _, cx| cx.open_url(&url)),
                            ),
                    )
                    .when(has_body, |this| {
                        this.child(
                            Icon::new(if open {
                                IconName::ChevronUp
                            } else {
                                IconName::ChevronDown
                            })
                            .small()
                            .text_color(muted),
                        )
                    }),
            )
            .when(open && has_body, |this| {
                this.child(self.render_notes(r, cx))
            })
            .into_any_element()
    }

    /// Aufgeklappte Release Notes eines Releases mit dem Kappungs-Zustand aus dem Workspace.
    fn render_notes(&self, r: &Release, cx: &mut Context<Self>) -> Div {
        let workspace = cx.entity().downgrade();
        let id = r.id.clone();
        let on_long = move |is_long: bool, cx: &mut App| {
            workspace
                .update(cx, |this, cx| {
                    let changed = if is_long {
                        this.long_notes.insert(id.clone())
                    } else {
                        this.long_notes.remove(&id)
                    };
                    if changed {
                        this.remeasure_key(&format!("release:{id}"));
                        cx.notify();
                    }
                })
                .ok();
        };
        let id = r.id.clone();
        let on_toggle = cx.listener(move |this, _: &ClickEvent, _, cx| {
            if !this.full_notes.remove(&id) {
                this.full_notes.insert(id.clone());
            }
            this.remeasure_key(&format!("release:{id}"));
            cx.notify();
        });
        notes(
            &r.id,
            r.description_html.clone().into(),
            NotesState {
                long: self.long_notes.contains(&r.id),
                full: self.full_notes.contains(&r.id),
            },
            on_long,
            on_toggle,
            cx,
        )
        .border_t_1()
        .border_color(cx.theme().border)
    }
}

/// Kappungs-Zustand aufgeklappter Release Notes.
#[derive(Clone, Copy, Default)]
struct NotesState {
    /// Gemessen höher als [`NOTES_MAX_HEIGHT`]
    long: bool,
    /// Per „Mehr anzeigen“ vollständig sichtbar
    full: bool,
}

/// Release Notes, gekappt auf [`NOTES_MAX_HEIGHT`] mit Verlauf und „Mehr anzeigen“.
///
/// Ob die Notes über die Kappung hinausgehen, hängt von der Fensterbreite ab und lässt sich erst
/// nach dem Layout sagen: Die volle Höhe des Inhalts wird beim Zeichnen gemessen und über
/// `on_long` gemeldet, sobald sie von `state.long` abweicht. Erst dann erscheinen Verlauf und
/// Knopf.
fn notes(
    id: &str,
    html: SharedString,
    state: NotesState,
    on_long: impl Fn(bool, &mut App) + Clone + 'static,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Div {
    let NotesState { long, full } = state;
    let card = cx.theme().group_box;

    let clip = div()
        .relative()
        .when(!full, |d| d.max_h(rems(NOTES_MAX_HEIGHT)).overflow_hidden())
        .on_children_prepainted(move |bounds, window, cx| {
            let Some(content) = bounds.first() else {
                return;
            };
            let is_long = content.size.height > rems(NOTES_MAX_HEIGHT).to_pixels(window.rem_size());
            if is_long != long {
                // Nicht mitten im Zeichnen ändern: erst danach, dann neu messen und zeichnen.
                let on_long = on_long.clone();
                cx.defer(move |cx| on_long(is_long, cx));
            }
        })
        .child(
            div().flex_shrink_0().child(
                // descriptionHTML ist von GitHub gerendertes und bereinigtes HTML
                TextView::html(SharedString::from(format!("notes-{id}")), html)
                    .style(notes_style(typography::BASE)),
            ),
        )
        .when(long && !full, |d| {
            d.child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .h(rems(NOTES_FADE_HEIGHT))
                    .bg(linear_gradient(
                        180.,
                        linear_color_stop(card.opacity(0.), 0.),
                        linear_color_stop(card, 1.),
                    )),
            )
        });

    v_flex()
        .px_5()
        .py_4()
        .gap_2()
        .text_body()
        .child(clip)
        .when(long, |this| {
            this.child(
                div().child(
                    Button::new(SharedString::from(format!("release-more-{id}")))
                        .ghost()
                        .xsmall()
                        .label(if full {
                            tr!("Show Less", "Weniger anzeigen")
                        } else {
                            tr!("Show More", "Mehr anzeigen")
                        })
                        .on_click(on_toggle),
                ),
            )
        })
}

#[cfg(test)]
mod tests {
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{
        AnyWindowHandle, AppContext as _, Bounds, Context, Entity, IntoElement, ParentElement as _,
        Point, Render, SharedString, Styled as _, TestAppContext, Window, WindowBounds,
        WindowOptions, div, px, size,
    };

    // Kein `use super::*`: Der Glob brächte GPUIs `test`-Makro mit und verdeckte `#[test]`.
    use super::{NOTES_MAX_HEIGHT, NotesState, notes, notes_style};

    /// Rendert nur die Notes und führt ihren Zustand wie der Workspace.
    struct Probe {
        html: SharedString,
        state: NotesState,
        reports: usize,
    }

    impl Render for Probe {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let probe = cx.entity().downgrade();
            let on_long = move |is_long: bool, cx: &mut gpui_kit::App| {
                probe
                    .update(cx, |this, cx| {
                        this.reports += 1;
                        this.state.long = is_long;
                        cx.notify();
                    })
                    .ok();
            };
            let on_toggle = cx.listener(|this, _: &gpui_kit::ClickEvent, _, cx| {
                this.state.full = !this.state.full;
                cx.notify();
            });
            div().w(px(600.)).child(notes(
                "r1",
                self.html.clone(),
                self.state,
                on_long,
                on_toggle,
                cx,
            ))
        }
    }

    fn open(cx: &mut TestAppContext, html: String) -> (AnyWindowHandle, Entity<Probe>) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Point::default(),
                    size: size(px(800.), px(2000.)),
                })),
                ..Default::default()
            };
            let (handle, probe) = gpui_kit::open_window(options, cx, |_, cx| {
                cx.new(|_| Probe {
                    html: html.into(),
                    state: NotesState::default(),
                    reports: 0,
                })
            })
            .expect("Testfenster");
            (handle, probe)
        })
    }

    fn frame(cx: &mut TestAppContext, handle: AnyWindowHandle) {
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
        cx.run_until_parked();
    }

    fn more_button(cx: &mut TestAppContext, handle: AnyWindowHandle) -> bool {
        cx.update_window(handle, |_, window, _| {
            window.try_find("release-more-r1").is_some()
        })
        .unwrap()
    }

    /// Oberkante des Knopfs unter den Notes, in Grundschrift-Einheiten (rem).
    fn button_top_rem(cx: &mut TestAppContext, handle: AnyWindowHandle) -> f32 {
        cx.update_window(handle, |_, window, _| {
            let top = window.find("release-more-r1").bounds().origin.y;
            top / window.rem_size()
        })
        .unwrap()
    }

    fn long_notes() -> String {
        let items: String = (1..=40)
            .map(|i| format!("<li>Änderung Nummer {i}</li>"))
            .collect();
        format!("<h2>What's Changed</h2><ul>{items}</ul>")
    }

    #[gpui_kit::test]
    async fn lange_notes_werden_gekappt_und_lassen_sich_aufklappen(cx: &mut TestAppContext) {
        let (handle, probe) = open(cx, long_notes());
        frame(cx, handle);
        frame(cx, handle);

        let state = cx.read(|cx| probe.read(cx).state);
        assert!(state.long, "40 Listenpunkte müssen als lang erkannt werden");
        assert!(more_button(cx, handle), "„Mehr anzeigen“ fehlt");
        let clipped = button_top_rem(cx, handle);
        assert!(
            clipped < NOTES_MAX_HEIGHT + 3.,
            "gekappte Notes enden bei {clipped} rem"
        );

        cx.update_window(handle, |_, window, cx| window.click("release-more-r1", cx))
            .unwrap();
        frame(cx, handle);
        frame(cx, handle);
        let (state, reports) = cx.read(|cx| (probe.read(cx).state, probe.read(cx).reports));
        assert!(
            state.full && state.long,
            "aufgeklappt bleibt die Messung „lang“"
        );
        assert_eq!(reports, 1, "die Messung darf nicht hin und her springen");
        assert!(more_button(cx, handle), "„Weniger anzeigen“ fehlt");
        let open = button_top_rem(cx, handle);
        assert!(
            open > clipped * 2.,
            "aufgeklappt nur {open} rem statt voller Höhe"
        );
    }

    #[test]
    fn ueberschriften_nie_groesser_als_der_release_name() {
        for name in [px(13.), px(16.)] {
            let style = notes_style(name);
            let size = style.heading_font_size.expect("eigene Überschriftengrößen");
            for level in 1..=6 {
                assert!(
                    size(level, style.heading_base_font_size) <= name,
                    "h{level}"
                );
            }
        }
    }

    #[gpui_kit::test]
    async fn kurze_notes_bleiben_ohne_knopf(cx: &mut TestAppContext) {
        let (handle, probe) = open(cx, "<p>Nur ein Bugfix.</p>".into());
        frame(cx, handle);
        frame(cx, handle);

        let (state, reports) = cx.read(|cx| (probe.read(cx).state, probe.read(cx).reports));
        assert!(!state.long);
        assert_eq!(reports, 0, "kurze Notes melden nichts");
        assert!(!more_button(cx, handle));
    }
}
