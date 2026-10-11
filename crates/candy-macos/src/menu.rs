//! Native macOS menu bar. Commands are caller-defined values; AppKit dispatches
//! menu actions and key equivalents on the main thread, before the window sees
//! the key event, so Cmd shortcuts must be declared here rather than in egui.
use anyhow::{Context, Result};
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained,
    runtime::Sel, sel,
};
use objc2_app_kit::{
    NSApplication, NSControlStateValueOff, NSControlStateValueOn, NSEventModifierFlags, NSMenu,
    NSMenuItem,
};
use objc2_foundation::{NSObject, NSObjectProtocol, NSString};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

/// A Command-key shortcut; `shift` adds the Shift modifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub key: char,
    pub shift: bool,
}
impl Shortcut {
    pub const fn cmd(key: char) -> Self {
        Self { key, shift: false }
    }
    pub const fn shift_cmd(key: char) -> Self {
        Self { key, shift: true }
    }
    fn key_equivalent(self) -> (String, NSEventModifierFlags) {
        let mut flags = NSEventModifierFlags::Command;
        if self.shift {
            flags |= NSEventModifierFlags::Shift;
        }
        (self.key.to_lowercase().collect(), flags)
    }
}

pub enum Item<C> {
    Command {
        title: &'static str,
        command: C,
        shortcut: Option<Shortcut>,
    },
    Separator,
}
impl<C> Item<C> {
    pub fn command(title: &'static str, command: C, shortcut: Option<Shortcut>) -> Self {
        Self::Command {
            title,
            command,
            shortcut,
        }
    }
}

pub struct Menu<C> {
    pub title: &'static str,
    pub items: Vec<Item<C>>,
}

/// Commands in menu-item tag order. Tags are assigned as items are created, so
/// the table and the menu can never disagree about which command a tag means.
struct Registry<C> {
    commands: RefCell<Vec<C>>,
}
impl<C: Copy> Registry<C> {
    fn new() -> Rc<Self> {
        Rc::new(Self {
            commands: RefCell::new(Vec::new()),
        })
    }
    fn register(&self, command: C) -> usize {
        let mut commands = self.commands.borrow_mut();
        commands.push(command);
        commands.len() - 1
    }
    fn lookup(&self, tag: isize) -> Option<C> {
        let index = usize::try_from(tag).ok()?;
        self.commands.borrow().get(index).copied()
    }
}

struct TargetIvars {
    on_command: Box<dyn Fn(isize)>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and MenuTarget has no Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "CandyMenuTarget"]
    #[ivars = TargetIvars]
    struct MenuTarget;

    // SAFETY: NSObjectProtocol has no safety requirements.
    unsafe impl NSObjectProtocol for MenuTarget {}

    impl MenuTarget {
        // SAFETY: Menu actions receive the sending NSMenuItem.
        #[unsafe(method(dispatch:))]
        fn dispatch(&self, sender: &NSMenuItem) {
            (self.ivars().on_command)(sender.tag());
        }
    }
);

impl MenuTarget {
    fn new(mtm: MainThreadMarker, on_command: Box<dyn Fn(isize)>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TargetIvars { on_command });
        // SAFETY: NSObject's init takes no arguments and returns the initialized object.
        unsafe { msg_send![super(this), init] }
    }
}

struct Entry<C> {
    command: C,
    item: Retained<NSMenuItem>,
    enabled: Cell<bool>,
    checked: Cell<bool>,
}

/// The installed menu bar. Keep it alive for the life of the application: menu
/// items only hold a weak reference to the action target.
pub struct MenuBar<C> {
    entries: Vec<Entry<C>>,
    _target: Retained<MenuTarget>,
}

impl<C: Copy + PartialEq + 'static> MenuBar<C> {
    /// Replaces the application menu bar with the standard application menu,
    /// `menus`, and a Window menu. Quit dispatches `quit` instead of terminating,
    /// so the caller can confirm unsaved work. `on_command` runs on the main thread.
    pub fn install(
        app_name: &str,
        menus: Vec<Menu<C>>,
        quit: C,
        on_command: impl Fn(C) + 'static,
    ) -> Result<Self> {
        let mtm = MainThreadMarker::new()
            .context("The menu bar can only be installed from the main thread")?;
        let registry = Registry::new();
        let target = MenuTarget::new(mtm, {
            let registry = Rc::clone(&registry);
            Box::new(move |tag| {
                // Resolve before calling out so the handler never holds the borrow.
                if let Some(command) = registry.lookup(tag) {
                    on_command(command);
                }
            })
        });
        let mut entries = Vec::new();
        let mut add_command =
            |menu: &NSMenu, title: &str, command: C, shortcut: Option<Shortcut>| {
                let item = standard_item(mtm, title, Some(sel!(dispatch:)), shortcut);
                // SAFETY: The target outlives the menu bar (it is stored below), and
                // dispatch: is defined on MenuTarget with the matching signature.
                unsafe { item.setTarget(Some(&target)) };
                item.setTag(registry.register(command) as isize);
                menu.addItem(&item);
                entries.push(Entry {
                    command,
                    item,
                    enabled: Cell::new(true),
                    checked: Cell::new(false),
                });
            };

        let bar = NSMenu::new(mtm);
        let app_menu = NSMenu::new(mtm);
        app_menu.addItem(&standard_item(
            mtm,
            &format!("About {app_name}"),
            Some(sel!(orderFrontStandardAboutPanel:)),
            None,
        ));
        app_menu.addItem(&NSMenuItem::separatorItem(mtm));
        let services = NSMenu::initWithTitle(mtm.alloc(), &NSString::from_str("Services"));
        let services_item = standard_item(mtm, "Services", None, None);
        services_item.setSubmenu(Some(&services));
        app_menu.addItem(&services_item);
        app_menu.addItem(&NSMenuItem::separatorItem(mtm));
        app_menu.addItem(&standard_item(
            mtm,
            &format!("Hide {app_name}"),
            Some(sel!(hide:)),
            Some(Shortcut::cmd('h')),
        ));
        let hide_others = standard_item(
            mtm,
            "Hide Others",
            Some(sel!(hideOtherApplications:)),
            Some(Shortcut::cmd('h')),
        );
        hide_others.setKeyEquivalentModifierMask(
            NSEventModifierFlags::Command | NSEventModifierFlags::Option,
        );
        app_menu.addItem(&hide_others);
        app_menu.addItem(&standard_item(
            mtm,
            "Show All",
            Some(sel!(unhideAllApplications:)),
            None,
        ));
        app_menu.addItem(&NSMenuItem::separatorItem(mtm));
        add_command(
            &app_menu,
            &format!("Quit {app_name}"),
            quit,
            Some(Shortcut::cmd('q')),
        );
        attach(mtm, &bar, &app_menu);

        for menu in &menus {
            let native = NSMenu::initWithTitle(mtm.alloc(), &NSString::from_str(menu.title));
            native.setAutoenablesItems(false);
            for item in &menu.items {
                match item {
                    Item::Command {
                        title,
                        command,
                        shortcut,
                    } => add_command(&native, title, *command, *shortcut),
                    Item::Separator => native.addItem(&NSMenuItem::separatorItem(mtm)),
                }
            }
            attach(mtm, &bar, &native);
        }

        let window_menu = NSMenu::initWithTitle(mtm.alloc(), &NSString::from_str("Window"));
        window_menu.addItem(&standard_item(
            mtm,
            "Minimize",
            Some(sel!(performMiniaturize:)),
            Some(Shortcut::cmd('m')),
        ));
        window_menu.addItem(&standard_item(mtm, "Zoom", Some(sel!(performZoom:)), None));
        window_menu.addItem(&NSMenuItem::separatorItem(mtm));
        window_menu.addItem(&standard_item(
            mtm,
            "Bring All to Front",
            Some(sel!(arrangeInFront:)),
            None,
        ));
        attach(mtm, &bar, &window_menu);

        let app = NSApplication::sharedApplication(mtm);
        app.setServicesMenu(Some(&services));
        app.setWindowsMenu(Some(&window_menu));
        app.setMainMenu(Some(&bar));
        Ok(Self {
            entries,
            _target: target,
        })
    }

    pub fn set_enabled(&self, command: C, enabled: bool) {
        for entry in self.entries.iter().filter(|e| e.command == command) {
            if entry.enabled.replace(enabled) != enabled {
                entry.item.setEnabled(enabled);
            }
        }
    }

    pub fn set_checked(&self, command: C, checked: bool) {
        for entry in self.entries.iter().filter(|e| e.command == command) {
            if entry.checked.replace(checked) != checked {
                entry.item.setState(if checked {
                    NSControlStateValueOn
                } else {
                    NSControlStateValueOff
                });
            }
        }
    }
}

fn standard_item(
    mtm: MainThreadMarker,
    title: &str,
    action: Option<Sel>,
    shortcut: Option<Shortcut>,
) -> Retained<NSMenuItem> {
    let (key, flags) = shortcut
        .map(Shortcut::key_equivalent)
        .unwrap_or_else(|| (String::new(), NSEventModifierFlags::Command));
    // SAFETY: Every selector passed here is either defined on MenuTarget or a
    // documented AppKit action resolved through the responder chain.
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc(),
            &NSString::from_str(title),
            action,
            &NSString::from_str(&key),
        )
    };
    item.setKeyEquivalentModifierMask(flags);
    item
}

fn attach(mtm: MainThreadMarker, bar: &NSMenu, menu: &NSMenu) {
    let holder = NSMenuItem::new(mtm);
    holder.setSubmenu(Some(menu));
    bar.addItem(&holder);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shortcuts_map_to_lowercase_key_equivalents_with_command_masks() {
        assert_eq!(
            Shortcut::cmd('O').key_equivalent(),
            ("o".to_string(), NSEventModifierFlags::Command)
        );
        assert_eq!(
            Shortcut::shift_cmd('s').key_equivalent(),
            (
                "s".to_string(),
                NSEventModifierFlags::Command | NSEventModifierFlags::Shift
            )
        );
        assert_eq!(Shortcut::cmd('0').key_equivalent().0, "0");
    }
    #[test]
    fn tags_resolve_to_commands_in_registration_order() {
        let registry = Registry::new();
        assert_eq!(registry.register("quit"), 0);
        assert_eq!(registry.register("open"), 1);
        assert_eq!(registry.register("open"), 2);
        assert_eq!(registry.lookup(0), Some("quit"));
        assert_eq!(registry.lookup(2), Some("open"));
        assert_eq!(registry.lookup(3), None);
        assert_eq!(registry.lookup(-1), None);
    }
    #[test]
    fn installing_off_the_main_thread_fails_cleanly() {
        let error = std::thread::spawn(|| {
            MenuBar::install("Test", Vec::<Menu<u8>>::new(), 0, |_| {}).err()
        })
        .join()
        .unwrap()
        .expect("worker threads must not install menus");
        assert!(error.to_string().contains("main thread"));
    }
}
