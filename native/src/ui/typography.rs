//! Schriftstufen nach macOS-Maß (PRR-50).
//!
//! Die Grundschrift ist 13 px wie Fließtext und Seitenleiste in macOS-Apps. gpui-component
//! leitet daraus die rem-Größe ab (`Root` setzt `rem_size = theme.font_size`), Abstände und
//! rem-Maße schrumpfen also mit.
//!
//! Im Fenster gibt es genau drei Stufen, alle Texte nutzen eine davon:
//!
//! | Stufe | Größe | Wofür |
//! |---|---|---|
//! | Titel | 15 px, halbfett | Seitentitel („Offene Pull Requests“) |
//! | Text | 13 px | Fließtext, Zeilentitel (mit `font_medium`), Seitenleiste |
//! | Klein | 0,875 rem ≈ 11,4 px | Metadaten, Zähler, Badges, Gruppenköpfe (halbfett) |
//!
//! „Klein“ ist bewusst das `text_sm` von gpui-component: Buttons, Eingabefelder, Tooltips und
//! Menüs der Bibliothek schreiben in dieser Größe, so teilen sie sich eine Stufe mit den
//! Metadaten statt einer vierten, fast gleichen Größe (macOS: 11 pt).

use gpui_kit::*;

/// Grundschrift: Fließtext, Zeilentitel und Seitenleiste.
pub const BASE: Pixels = px(13.);
/// Seitentitel.
const TITLE: Pixels = px(15.);
/// Metadaten und Zähler; entspricht `text_sm` der Bedienelemente.
const SMALL: Rems = rems(0.875);

/// Die drei Schriftstufen als Stil-Methoden; statt `text_xs`/`text_sm`/`text_base` verwenden.
pub trait TypeScale: Styled + Sized {
    /// Seitentitel: 15 px, halbfett.
    fn text_title(self) -> Self {
        self.text_size(TITLE).font_weight(FontWeight::SEMIBOLD)
    }

    /// Fließtext und Zeilentitel: 13 px.
    fn text_body(self) -> Self {
        self.text_size(BASE)
    }

    /// Metadaten, Zähler und Badges: ≈ 11,4 px.
    fn text_small(self) -> Self {
        self.text_size(SMALL)
    }

    /// Gruppen- und Abschnittsköpfe: klein, halbfett.
    fn text_heading(self) -> Self {
        self.text_small().font_weight(FontWeight::SEMIBOLD)
    }
}

impl<E: Styled> TypeScale for E {}
