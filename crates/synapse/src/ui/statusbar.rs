use async_channel::Sender;
use cocoa::appkit::{
    NSApp, NSApplication,
    NSApplicationActivationPolicy::{
        NSApplicationActivationPolicyAccessory, NSApplicationActivationPolicyRegular,
    },
};
use gpui::Global;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

#[derive(Debug, Clone, Copy)]
pub enum Action {
    Open,
    Data,
    Install,
    SystemTheme,
    LightTheme,
    DarkTheme,
    Quit,
}

pub struct Statusbar {
    _tray: TrayIcon,
}

impl Global for Statusbar {}

pub fn install(sender: Sender<Action>) -> anyhow::Result<Statusbar> {
    let menu = Menu::new();
    let open = MenuItem::new("Open Synapse", true, None);
    let install = MenuItem::new("Install CLI", true, None);
    let data = MenuItem::new("Open data folder", true, None);
    let systemtheme = MenuItem::new("System", true, None);
    let lighttheme = MenuItem::new("Light", true, None);
    let darktheme = MenuItem::new("Dark", true, None);
    let appearance =
        Submenu::with_items("Appearance", true, &[&systemtheme, &lighttheme, &darktheme])?;
    let separator = PredefinedMenuItem::separator();
    let quit = MenuItem::new("Quit Synapse", true, None);
    menu.append_items(&[&open, &install, &data, &appearance, &separator, &quit])?;

    let openid = open.id().clone();
    let installid = install.id().clone();
    let dataid = data.id().clone();
    let systemid = systemtheme.id().clone();
    let lightid = lighttheme.id().clone();
    let darkid = darktheme.id().clone();
    let quitid = quit.id().clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let action = if event.id == openid {
            Some(Action::Open)
        } else if event.id == installid {
            Some(Action::Install)
        } else if event.id == dataid {
            Some(Action::Data)
        } else if event.id == systemid {
            Some(Action::SystemTheme)
        } else if event.id == lightid {
            Some(Action::LightTheme)
        } else if event.id == darkid {
            Some(Action::DarkTheme)
        } else if event.id == quitid {
            Some(Action::Quit)
        } else {
            None
        };
        if let Some(action) = action {
            let _ = sender.try_send(action);
        }
    }));

    let tray = TrayIconBuilder::new()
        .with_tooltip("Synapse · local memory")
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(true)
        .with_icon_as_template(true)
        .with_icon(icon()?)
        .build()?;

    Ok(Statusbar { _tray: tray })
}

/// macOS reads two separate things off the activation policy: whether the app
/// has a Dock icon, and whether it has a menu bar. An accessory app has
/// neither, and a window with no menu bar is a window where ⌘X does nothing —
/// there is no Edit menu to claim the keystroke and no Quit item to bind ⌘Q.
///
/// So the policy follows the window rather than the status item: regular while
/// the window is up, accessory once it is put away. The menu bar item is there
/// either way, which is what it was for.
pub fn foreground() {
    unsafe {
        NSApp().setActivationPolicy_(NSApplicationActivationPolicyRegular);
    }
}

pub fn background() {
    unsafe {
        NSApp().setActivationPolicy_(NSApplicationActivationPolicyAccessory);
    }
}

fn icon() -> anyhow::Result<Icon> {
    const SIZE: usize = 20;
    let mut rgba = vec![0; SIZE * SIZE * 4];
    let nodes = [(5, 5), (14, 5), (10, 10), (5, 15), (14, 15)];
    for (from, to) in [(0, 2), (1, 2), (2, 3), (2, 4), (0, 3), (1, 4)] {
        line(&mut rgba, nodes[from], nodes[to]);
    }
    for node in nodes {
        circle(&mut rgba, node);
    }
    Icon::from_rgba(rgba, SIZE as u32, SIZE as u32).map_err(Into::into)
}

fn pixel(rgba: &mut [u8], x: i32, y: i32) {
    const SIZE: i32 = 20;
    if !(0..SIZE).contains(&x) || !(0..SIZE).contains(&y) {
        return;
    }
    let index = ((y * SIZE + x) * 4) as usize;
    rgba[index..index + 4].copy_from_slice(&[0, 0, 0, 255]);
}

fn circle(rgba: &mut [u8], center: (i32, i32)) {
    for y in -2..=2 {
        for x in -2..=2 {
            if x * x + y * y <= 4 {
                pixel(rgba, center.0 + x, center.1 + y);
            }
        }
    }
}

fn line(rgba: &mut [u8], start: (i32, i32), end: (i32, i32)) {
    let steps = (end.0 - start.0).abs().max((end.1 - start.1).abs());
    for step in 0..=steps {
        let ratio = step as f32 / steps as f32;
        let x = start.0 as f32 + (end.0 - start.0) as f32 * ratio;
        let y = start.1 as f32 + (end.1 - start.1) as f32 * ratio;
        pixel(rgba, x.round() as i32, y.round() as i32);
    }
}
