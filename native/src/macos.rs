//! AppKit-Teile, die GPUI nicht anbietet: Menüleisten-Symbol, Dock-Badge und
//! „Beim Anmelden starten“ (`SMAppService`, ab macOS 13).
//!
//! Klicks im Menü gehen als Tag über einen Kanal an die App zurück; was sie
//! bedeuten, entscheidet `crate::menubar`.

use std::cell::RefCell;

use futures::channel::mpsc::UnboundedSender;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSCellImagePosition, NSControlStateValueMixed, NSControlStateValueOff,
    NSControlStateValueOn, NSImage, NSMenu, NSMenuDelegate, NSMenuItem, NSStatusBar, NSStatusItem,
    NSVariableStatusItemLength,
};
use objc2_foundation::{NSBundle, NSObject, NSString};
use objc2_service_management::{SMAppService, SMAppServiceStatus};

/// SF Symbol des Menüleisten-Symbols (Vorlage, passt sich Hell/Dunkel an).
const SYMBOL: &str = "arrow.triangle.pull";

/// Ein Eintrag im Menü des Menüleisten-Symbols.
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    /// Nicht anklickbare Zeile, z. B. die Überschrift.
    Label(String),
    /// Anklickbar; `tag` kommt beim Klick über den Kanal zurück.
    Item {
        title: String,
        tag: isize,
        tooltip: Option<String>,
    },
    /// „Beim Anmelden starten“ mit Häkchen nach aktuellem Zustand.
    LoginItem {
        title: String,
        tag: isize,
    },
    Separator,
}

struct Ivars {
    clicks: UnboundedSender<isize>,
    entries: RefCell<Vec<Entry>>,
}

define_class!(
    // SAFETY: `NSObject` stellt keine Anforderungen an Unterklassen, `MenuTarget`
    // implementiert kein `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "PRRadarStatusMenuTarget"]
    #[ivars = Ivars]
    struct MenuTarget;

    impl MenuTarget {
        #[unsafe(method(itemClicked:))]
        fn item_clicked(&self, sender: &NSMenuItem) {
            self.ivars().clicks.unbounded_send(sender.tag()).ok();
        }
    }

    unsafe impl NSObjectProtocol for MenuTarget {}

    unsafe impl NSMenuDelegate for MenuTarget {
        // Das Menü wird erst beim Öffnen gebaut: So stimmt das Häkchen bei
        // „Beim Anmelden starten“ auch nach Änderungen in den Systemeinstellungen,
        // und ein Poll verändert kein gerade offenes Menü.
        #[unsafe(method(menuNeedsUpdate:))]
        fn menu_needs_update(&self, menu: &NSMenu) {
            self.fill(menu);
        }
    }
);

impl MenuTarget {
    fn new(clicks: UnboundedSender<isize>, mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            clicks,
            entries: RefCell::new(Vec::new()),
        });
        // SAFETY: `init` ist der designierte Initialisierer von `NSObject`.
        unsafe { msg_send![super(this), init] }
    }

    fn fill(&self, menu: &NSMenu) {
        let mtm = self.mtm();
        menu.removeAllItems();
        for entry in self.ivars().entries.borrow().iter() {
            let item = match entry {
                Entry::Separator => NSMenuItem::separatorItem(mtm),
                Entry::Label(title) => {
                    let item = self.item(title, 0);
                    item.setEnabled(false);
                    item
                }
                Entry::Item {
                    title,
                    tag,
                    tooltip,
                } => {
                    let item = self.item(title, *tag);
                    if let Some(tip) = tooltip {
                        item.setToolTip(Some(&NSString::from_str(tip)));
                    }
                    item
                }
                Entry::LoginItem { title, tag } => {
                    let item = self.item(title, *tag);
                    let state = login_state();
                    item.setState(match state {
                        LoginState::On => NSControlStateValueOn,
                        LoginState::NeedsApproval => NSControlStateValueMixed,
                        LoginState::Off | LoginState::Unavailable => NSControlStateValueOff,
                    });
                    item.setEnabled(state != LoginState::Unavailable);
                    if let Some(tip) = state.hint() {
                        item.setToolTip(Some(&NSString::from_str(tip)));
                    }
                    item
                }
            };
            menu.addItem(&item);
        }
    }

    fn item(&self, title: &str, tag: isize) -> Retained<NSMenuItem> {
        let mtm = self.mtm();
        // SAFETY: `itemClicked:` ist oben mit passender Signatur definiert, und das
        // Ziel lebt so lange wie das Menü (beide hält `StatusItem`).
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(title),
                Some(sel!(itemClicked:)),
                &NSString::from_str(""),
            )
        };
        let target: &AnyObject = self;
        // SAFETY: siehe oben.
        unsafe { item.setTarget(Some(target)) };
        item.setTag(tag);
        item
    }
}

/// Das Symbol rechts in der Menüleiste. Verschwindet, sobald es fallen gelassen wird.
pub struct StatusItem {
    item: Retained<NSStatusItem>,
    _menu: Retained<NSMenu>,
    target: Retained<MenuTarget>,
    mtm: MainThreadMarker,
}

impl StatusItem {
    /// Legt das Symbol an. `None`, wenn nicht auf dem Haupt-Thread aufgerufen.
    pub fn new(clicks: UnboundedSender<isize>) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        let target = MenuTarget::new(clicks, mtm);
        let menu = NSMenu::new(mtm);
        menu.setAutoenablesItems(false);
        menu.setDelegate(Some(ProtocolObject::from_ref(&*target)));
        item.setMenu(Some(&menu));

        if let Some(button) = item.button(mtm) {
            let symbol = NSImage::imageWithSystemSymbolName_accessibilityDescription(
                &NSString::from_str(SYMBOL),
                Some(&NSString::from_str("PR Radar")),
            );
            match symbol {
                Some(image) => {
                    image.setTemplate(true);
                    button.setImage(Some(&image));
                    button.setImagePosition(NSCellImagePosition::ImageLeading);
                }
                None => button.setTitle(&NSString::from_str("PR")),
            }
        }
        Some(Self {
            item,
            _menu: menu,
            target,
            mtm,
        })
    }

    /// Zahl neben dem Symbol (`None` = nur das Symbol) und Tooltip.
    pub fn set_badge(&self, badge: Option<&str>, tooltip: &str) {
        let Some(button) = self.item.button(self.mtm) else {
            return;
        };
        // Schmales Leerzeichen zwischen Symbol und Zahl, wie bei Systemsymbolen.
        let title = badge.map(|b| format!("\u{2009}{b}")).unwrap_or_default();
        button.setTitle(&NSString::from_str(&title));
        button.setToolTip(Some(&NSString::from_str(tooltip)));
    }

    /// Ersetzt die Menüeinträge; gebaut wird beim nächsten Öffnen.
    pub fn set_entries(&self, entries: Vec<Entry>) {
        *self.target.ivars().entries.borrow_mut() = entries;
    }
}

impl Drop for StatusItem {
    fn drop(&mut self) {
        NSStatusBar::systemStatusBar().removeStatusItem(&self.item);
    }
}

/// Badge am Dock-Symbol (`None` entfernt es).
pub fn set_dock_badge(label: Option<&str>) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let label = label.map(NSString::from_str);
    NSApplication::sharedApplication(mtm)
        .dockTile()
        .setBadgeLabel(label.as_deref());
}

/// Zustand von „Beim Anmelden starten“.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginState {
    On,
    Off,
    /// Eingetragen, aber in den Systemeinstellungen (Anmeldeobjekte) noch nicht erlaubt.
    NeedsApproval,
    /// Nur aus dem App-Bundle möglich, nicht unter `cargo run`.
    Unavailable,
}

impl LoginState {
    pub fn hint(self) -> Option<&'static str> {
        match self {
            Self::NeedsApproval => Some(
                "In den Systemeinstellungen unter Allgemein → Anmeldeobjekte noch nicht erlaubt",
            ),
            Self::Unavailable => Some("Nur in der installierten App verfügbar"),
            Self::On | Self::Off => None,
        }
    }
}

/// Läuft die App aus einem Bundle? Nur dann kann sie sich als Anmeldeobjekt eintragen.
fn is_bundled() -> bool {
    NSBundle::mainBundle().bundleIdentifier().is_some()
}

pub fn login_state() -> LoginState {
    if !is_bundled() {
        return LoginState::Unavailable;
    }
    // SAFETY: einfache Abfrage ohne Vorbedingungen.
    let status = unsafe { SMAppService::mainAppService().status() };
    match status {
        SMAppServiceStatus::Enabled => LoginState::On,
        SMAppServiceStatus::RequiresApproval => LoginState::NeedsApproval,
        // `NotFound` meldet macOS auch für eine App, die noch nie eingetragen war.
        _ => LoginState::Off,
    }
}

/// Trägt die App als Anmeldeobjekt ein bzw. aus.
pub fn set_login(enabled: bool) -> Result<(), String> {
    if !is_bundled() {
        return Err("Nur in der installierten App verfügbar".into());
    }
    // SAFETY: `mainAppService` beschreibt die laufende App; Fehler kommen als NSError zurück.
    let result = unsafe {
        let service = SMAppService::mainAppService();
        if enabled {
            service.registerAndReturnError()
        } else {
            service.unregisterAndReturnError()
        }
    };
    result.map_err(|e| e.localizedDescription().to_string())
}

/// Öffnet Systemeinstellungen → Allgemein → Anmeldeobjekte.
pub fn open_login_settings() {
    // SAFETY: öffnet nur die Systemeinstellungen.
    unsafe { SMAppService::openSystemSettingsLoginItems() };
}
