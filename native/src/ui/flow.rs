//! Flow-Ansicht: PR → Checks je Workflow → Review → Merge → Ziel-Branch als Knotengraph.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::Workspace;
use super::parts::{avatar, card_segment, pipeline_icon, repo_name};
use crate::model::{Check, OpenPr, PipelineState, ReviewDecision, Snapshot};
use crate::time::ago;

/// Höhe eines Check-Knotens, Abstand dazwischen und Mindesthöhe der Zeile – in rem,
/// damit Kanten und Knoten gemeinsam zoomen.
const CHECK_H: f32 = 2.75;
const CHECK_GAP: f32 = 0.5;
const ROW_PAD: f32 = 1.0;
const MIN_H: f32 = 6.5;
const MAX_CHECK_NODES: usize = 4;

#[derive(Clone, Copy, PartialEq)]
enum Tone {
    Pass,
    Fail,
    Run,
    Accent,
    Idle,
}

impl Tone {
    fn of(state: PipelineState) -> Self {
        match state {
            PipelineState::Passed => Self::Pass,
            PipelineState::Failed => Self::Fail,
            PipelineState::Running => Self::Run,
            PipelineState::None => Self::Idle,
        }
    }

    fn color(self, cx: &App) -> Hsla {
        match self {
            Self::Pass => cx.theme().success,
            Self::Fail => cx.theme().danger,
            Self::Run => cx.theme().warning,
            Self::Accent => cx.theme().primary,
            Self::Idle => cx.theme().muted_foreground,
        }
    }
}

struct CheckNode {
    key: String,
    label: String,
    sub: String,
    state: PipelineState,
    url: Option<String>,
}

fn worst(states: impl IntoIterator<Item = PipelineState>) -> PipelineState {
    states
        .into_iter()
        .min_by_key(|s| s.rank())
        .unwrap_or(PipelineState::None)
}

/// Checks nach Workflow bündeln – ein Knoten pro Workflow, Rotes zuerst, Rest zusammengefasst.
fn check_nodes(pr: &OpenPr) -> Vec<CheckNode> {
    let mut groups: Vec<(String, Vec<&Check>)> = Vec::new();
    for c in &pr.pipeline.checks {
        let key = c.group.clone().unwrap_or_else(|| c.name.clone());
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, list)) => list.push(c),
            None => groups.push((key, vec![c])),
        }
    }
    if groups.is_empty() {
        return vec![CheckNode {
            key: "none".into(),
            label: "Keine Checks".into(),
            sub: "letzter Commit".into(),
            state: PipelineState::None,
            url: None,
        }];
    }
    let mut nodes: Vec<CheckNode> = groups
        .into_iter()
        .map(|(label, checks)| {
            let state = worst(checks.iter().map(|c| c.state.pipeline()));
            let passed = checks
                .iter()
                .filter(|c| c.state.pipeline() == PipelineState::Passed)
                .count();
            let sub = if checks.len() > 1 {
                format!("{passed}/{} grün", checks.len())
            } else if checks[0].group.is_some() {
                checks[0].name.clone()
            } else {
                state.label().to_string()
            };
            let url = checks
                .iter()
                .find(|c| c.state.pipeline() == state)
                .unwrap_or(&checks[0])
                .url
                .clone();
            CheckNode {
                key: label.clone(),
                label,
                sub,
                state,
                url,
            }
        })
        .collect();
    nodes.sort_by_key(|n| n.state.rank());
    if nodes.len() <= MAX_CHECK_NODES {
        return nodes;
    }
    let rest = nodes.split_off(MAX_CHECK_NODES - 1);
    let state = worst(rest.iter().map(|n| n.state));
    nodes.push(CheckNode {
        key: "more".into(),
        label: format!("+{} weitere", rest.len()),
        sub: state.label().into(),
        state,
        url: Some(pr.url.clone()),
    });
    nodes
}

struct Stage {
    tone: Tone,
    icon: Icon,
    title: String,
    sub: String,
}

fn review_stage(pr: &OpenPr, viewer: Option<&str>) -> Stage {
    let stage = |tone, icon: Lucide, title: &str, sub: &str| Stage {
        tone,
        icon: Icon::new(icon),
        title: title.into(),
        sub: sub.into(),
    };
    if pr.requests_review_from(viewer) && pr.review_decision != Some(ReviewDecision::Approved) {
        return stage(Tone::Accent, Lucide::UserCheck, "Dein Review", "angefragt");
    }
    match pr.review_decision {
        Some(ReviewDecision::Approved) => {
            stage(Tone::Pass, Lucide::ShieldCheck, "Approved", "Review ok")
        }
        Some(ReviewDecision::ChangesRequested) => stage(
            Tone::Fail,
            Lucide::MessageSquareWarning,
            "Änderungen",
            "angefordert",
        ),
        Some(ReviewDecision::ReviewRequired) => {
            let who = if pr.review_requests.is_empty() {
                "ausstehend".into()
            } else {
                pr.review_requests.join(", ")
            };
            Stage {
                tone: Tone::Idle,
                icon: Icon::new(IconName::Eye),
                title: "Review offen".into(),
                sub: who,
            }
        }
        None => stage(Tone::Idle, Lucide::Eye, "Review", "nicht erforderlich"),
    }
}

fn merge_stage(pr: &OpenPr) -> Stage {
    let stage = |tone, icon: Lucide, title: &str, sub: String| Stage {
        tone,
        icon: Icon::new(icon),
        title: title.into(),
        sub,
    };
    if pr.has_conflict() {
        stage(
            Tone::Fail,
            Lucide::TriangleAlert,
            "Konflikt",
            "nicht mergebar".into(),
        )
    } else if pr.is_draft {
        stage(
            Tone::Idle,
            Lucide::GitPullRequestDraft,
            "Draft",
            "noch nicht bereit".into(),
        )
    } else if let Some(am) = &pr.auto_merge {
        stage(
            Tone::Accent,
            Lucide::Zap,
            "Auto-Merge",
            format!("{} · wenn grün", am.method.label()),
        )
    } else if pr.ready_to_merge() {
        stage(
            Tone::Pass,
            Lucide::GitMerge,
            "Bereit",
            "kann gemergt werden".into(),
        )
    } else if pr.merge_state_status == "BEHIND" {
        stage(
            Tone::Run,
            Lucide::GitMerge,
            "Veraltet",
            "Base ist weiter".into(),
        )
    } else if pr.merge_state_status == "BLOCKED" {
        stage(
            Tone::Idle,
            Lucide::GitMerge,
            "Blockiert",
            "wartet auf Regeln".into(),
        )
    } else {
        stage(
            Tone::Idle,
            Lucide::GitMerge,
            "Merge",
            pr.merge_state_status.to_lowercase(),
        )
    }
}

/// Knoten: Symbol-Kachel, Titel, Untertitel, Anschlusspunkte links/rechts.
fn node(
    tone: Tone,
    icon: impl IntoElement,
    title: impl IntoElement,
    sub: impl IntoElement,
    ports: (bool, bool),
    cx: &App,
) -> Stateful<Div> {
    let color = tone.color(cx);
    let port = |left: bool| {
        div()
            .absolute()
            .top(relative(0.5))
            .mt(rems(-0.3125))
            .when(left, |d| d.left(rems(-0.375)))
            .when(!left, |d| d.right(rems(-0.375)))
            .size(rems(0.625))
            .rounded_full()
            .border_2()
            .border_color(color)
            .bg(cx.theme().background)
    };
    h_flex()
        .id(ElementId::Name("node".into()))
        .relative()
        .gap_2p5()
        .px_2p5()
        .py_2()
        .rounded(cx.theme().radius_lg)
        .border_1()
        .border_color(if tone == Tone::Idle {
            cx.theme().border
        } else {
            color.opacity(0.6)
        })
        .bg(cx.theme().background)
        .shadow_xs()
        .when(ports.0, |this| this.child(port(true)))
        .when(ports.1, |this| this.child(port(false)))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .size_8()
                .rounded(cx.theme().radius)
                .bg(color.opacity(0.12))
                .text_color(color)
                .child(icon),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(div().text_sm().font_medium().truncate().child(title))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .truncate()
                        .child(sub),
                ),
        )
}

/// Eine Kante: Start-/End-y in rem, Farbe, gestrichelt, läuft (mit wanderndem Punkt).
#[derive(Clone, Copy)]
struct Edge {
    y1: f32,
    y2: f32,
    color: Hsla,
    dashed: bool,
    running: bool,
}

fn paint_edges(bounds: Bounds<Pixels>, edges: &[Edge], phase: Option<f32>, window: &mut Window) {
    let rem = window.rem_size();
    let w = bounds.size.width;
    for e in edges {
        let start = bounds.origin + point(px(0.), rem * e.y1);
        let end = bounds.origin + point(w, rem * e.y2);
        let (c1, c2) = (start + point(w / 2., px(0.)), end - point(w / 2., px(0.)));
        // Strichstärke und Strichmuster sind physische Haarlinien, keine Layout-Größen.
        let mut path = PathBuilder::stroke(px(1.5));
        if e.dashed {
            path = path.dash_array(&[px(4.), px(4.)]);
        }
        path.move_to(start);
        path.cubic_bezier_to(end, c1, c2);
        if let Ok(path) = path.build() {
            window.paint_path(path, e.color);
        }
        // Laufende Pipeline: ein Punkt wandert entlang der Kante – „hier fließt gerade etwas“.
        if let Some(t) = phase.filter(|_| e.running) {
            let p = super::motion::bezier(start, c1, c2, end, t);
            let r = px(2.5);
            window.paint_quad(
                fill(
                    Bounds {
                        origin: p - point(r, r),
                        size: size(r * 2., r * 2.),
                    },
                    e.color.opacity(1.),
                )
                .corner_radii(r),
            );
        }
    }
}

/// Kanten zwischen zwei Spalten als Bezier-Kurven; y-Werte in rem ab Oberkante.
/// `beat` wechselt mit jedem Poll; dann wandert einmal ein Punkt über laufende Kanten.
fn edges(links: Vec<(f32, f32, Tone)>, beat: Option<i64>, cx: &App) -> AnyElement {
    let edges: Vec<Edge> = links
        .into_iter()
        .map(|(y1, y2, tone)| Edge {
            y1,
            y2,
            color: tone
                .color(cx)
                .opacity(if tone == Tone::Idle { 0.45 } else { 0.85 }),
            dashed: matches!(tone, Tone::Idle | Tone::Run),
            running: tone == Tone::Run,
        })
        .collect();
    let frame = || div().w_8().h_full().flex_shrink_0();

    let beat = beat.filter(|_| edges.iter().any(|e| e.running) && super::motion::enabled(cx));
    let Some(beat) = beat else {
        return frame()
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| paint_edges(bounds, &edges, None, window),
                )
                .size_full(),
            )
            .into_any_element();
    };
    frame()
        .with_animation(
            SharedString::from(format!("edge-flow-{beat}")),
            Animation::new(std::time::Duration::from_millis(1400)).with_easing(ease_in_out),
            move |el, t| {
                let edges = edges.clone();
                el.child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, window, _| paint_edges(bounds, &edges, Some(t), window),
                    )
                    .size_full(),
                )
            },
        )
        .into_any_element()
}

impl Workspace {
    pub(super) fn render_flow_row(
        &self,
        pr: &OpenPr,
        snapshot: &Snapshot,
        show_repo: bool,
        first: bool,
        last: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let beat = self
            .radar
            .read(cx)
            .status()
            .last_success
            .map(|t| t.timestamp_millis());
        let checks = check_nodes(pr);
        let n = checks.len() as f32;
        let checks_h = n * CHECK_H + (n - 1.) * CHECK_GAP;
        let h = MIN_H.max(checks_h + ROW_PAD);
        let mid = h / 2.;
        let top = (h - checks_h) / 2.;
        let ys: Vec<f32> = (0..checks.len())
            .map(|i| top + i as f32 * (CHECK_H + CHECK_GAP) + CHECK_H / 2.)
            .collect();

        let viewer = snapshot.viewer.as_deref();
        let review = review_stage(pr, viewer);
        let merge = merge_stage(pr);
        let repo = snapshot.repo(&pr.repo);
        let on_default =
            repo.and_then(|r| r.default_branch.as_deref()) == Some(pr.base_ref.as_str());
        let base_state = if on_default {
            repo.map_or(PipelineState::None, |r| r.default_branch_pipeline)
        } else {
            PipelineState::None
        };
        let url = pr.url.clone();
        let mono = cx.theme().mono_font_family.clone();

        let pr_node = node(
            if pr.is_by(viewer) {
                Tone::Accent
            } else {
                Tone::Idle
            },
            avatar(pr.author.as_ref()),
            div()
                .when(pr.is_draft, |d| d.text_color(cx.theme().muted_foreground))
                .child(pr.title.clone()),
            h_flex()
                .gap_1p5()
                .when(show_repo, |d| d.child(repo_name(&pr.repo, cx)))
                .child(format!("#{}", pr.number))
                .child(
                    div()
                        .truncate()
                        .font_family(mono.clone())
                        .child(pr.head_ref.clone()),
                )
                .child(format!("· {}", ago(Some(pr.updated_at)))),
            (false, true),
            cx,
        )
        .id(SharedString::from(format!("flow-pr-{}", pr.id)))
        .flex_1()
        .min_w_48()
        .cursor_pointer()
        .hover(|s| s.border_color(cx.theme().primary))
        .on_click(move |_, _, cx| cx.open_url(&url));

        let check_column = v_flex()
            .w(rems(11.))
            .flex_shrink_0()
            .justify_center()
            .gap_2()
            .children(checks.iter().map(|c| {
                let url = c.url.clone();
                node(
                    Tone::of(c.state),
                    pipeline_icon(c.state, cx),
                    c.label.clone(),
                    c.sub.clone(),
                    (true, true),
                    cx,
                )
                .id(SharedString::from(format!(
                    "flow-check-{}-{}",
                    pr.id, c.key
                )))
                .h(rems(CHECK_H))
                .when_some(url, |this, url| {
                    this.cursor_pointer()
                        .hover(|s| s.border_color(cx.theme().primary))
                        .on_click(move |_, _, cx| cx.open_url(&url))
                })
            }));

        let merge_tone = merge.tone;
        let merge_node = node(
            merge.tone,
            merge.icon.small(),
            merge.title,
            v_flex().gap_1p5().child(merge.sub).child(
                div()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(self.render_merge_control(pr, snapshot, cx)),
            ),
            (true, true),
            cx,
        )
        .id(SharedString::from(format!("flow-merge-{}", pr.id)))
        .w(rems(11.))
        .flex_shrink_0();

        card_segment(first, last, cx).child(
            div()
                .px_4()
                .when(pr.has_conflict(), |this| {
                    this.bg(cx.theme().danger.opacity(0.05))
                })
                .child(
                    h_flex()
                        .h(rems(h))
                        .items_center()
                        .child(pr_node)
                        .child(edges(
                            checks
                                .iter()
                                .zip(&ys)
                                .map(|(c, &y)| (mid, y, Tone::of(c.state)))
                                .collect(),
                            beat,
                            cx,
                        ))
                        .child(check_column)
                        .child(edges(
                            checks
                                .iter()
                                .zip(&ys)
                                .map(|(c, &y)| (y, mid, Tone::of(c.state)))
                                .collect(),
                            beat,
                            cx,
                        ))
                        .child(
                            node(
                                review.tone,
                                review.icon.small(),
                                review.title,
                                review.sub,
                                (true, true),
                                cx,
                            )
                            .id(SharedString::from(format!("flow-review-{}", pr.id)))
                            .w(rems(9.))
                            .flex_shrink_0(),
                        )
                        .child(edges(vec![(mid, mid, review.tone)], beat, cx))
                        .child(merge_node)
                        .child(edges(vec![(mid, mid, merge_tone)], beat, cx))
                        .child(
                            node(
                                Tone::of(base_state),
                                Icon::new(Lucide::GitBranch).small(),
                                div().font_family(mono).child(pr.base_ref.clone()),
                                if on_default {
                                    format!("Pipeline {}", base_state.label())
                                } else {
                                    "Ziel-Branch".into()
                                },
                                (true, false),
                                cx,
                            )
                            .id(SharedString::from(format!("flow-base-{}", pr.id)))
                            .w_32()
                            .flex_shrink_0(),
                        ),
                ),
        )
    }
}
