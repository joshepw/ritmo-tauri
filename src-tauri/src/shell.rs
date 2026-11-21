use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, LogicalPosition, LogicalSize, Manager, PhysicalPosition, PhysicalSize, Rect, WebviewWindow,
};

// El icono queda al centro de la agenda. La ventana ya no trae margen transparente.
const PANEL_ANCHOR: f64 = 178.0;
const AGENDA_W: f64 = 356.0;
const PANEL_H: f64 = 800.0;
const MENU_GAP: f64 = 6.0;
static PANEL_SHOWN_MS: AtomicU64 = AtomicU64::new(0);
static PANEL_FOCUSED: AtomicBool = AtomicBool::new(false);
static PANEL_OPEN: AtomicBool = AtomicBool::new(false);
static CHAT_OPEN: AtomicBool = AtomicBool::new(false);
static GEMINI_OPEN: AtomicBool = AtomicBool::new(false);
static GEMINI_BUBBLE: AtomicBool = AtomicBool::new(false);
static SETTINGS_OPEN: AtomicBool = AtomicBool::new(false);
static BUBBLE_X: AtomicU64 = AtomicU64::new(0);
static BUBBLE_Y: AtomicU64 = AtomicU64::new(0);
const AVATAR_SIZE: f64 = 72.0;
const BUBBLE_CHAT_W: f64 = 360.0;
const BUBBLE_CHAT_H: f64 = 560.0;
static HIDE_TICK: AtomicU64 = AtomicU64::new(0);
static WELCOME_OPEN: AtomicBool = AtomicBool::new(false);
static SHOW_DOCK: AtomicBool = AtomicBool::new(false);

/// Puts the app in the Dock while `body` runs and returns it to the menu bar after.
/// macOS only shows the permission prompt, and only lists the app in
/// System Settings → Notifications, if the request comes from a regular app.
#[cfg(target_os = "macos")]
fn as_regular_app<T>(app: &AppHandle, body: impl FnOnce() -> T) -> T {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

    let (ready, wait) = std::sync::mpsc::channel();
    let _ = app.run_on_main_thread(move || {
        if let Some(marker) = MainThreadMarker::new() {
            let ns_app = NSApplication::sharedApplication(marker);
            let _ = ns_app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
            #[allow(deprecated)]
            ns_app.activateIgnoringOtherApps(true);
        }
        let _ = ready.send(());
    });
    let _ = wait.recv_timeout(std::time::Duration::from_secs(2));
    let result = body();
    set_menu_bar_policy(app);
    result
}

#[cfg(target_os = "macos")]
fn set_menu_bar_policy(app: &AppHandle) {
    if WELCOME_OPEN.load(Ordering::Relaxed) {
        return;
    }
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

    let show_dock = SHOW_DOCK.load(Ordering::Relaxed);
    let _ = app.run_on_main_thread(move || {
        if let Some(marker) = MainThreadMarker::new() {
            let ns_app = NSApplication::sharedApplication(marker);
            let policy = if show_dock {
                NSApplicationActivationPolicy::Regular
            } else {
                NSApplicationActivationPolicy::Accessory
            };
            let _ = ns_app.setActivationPolicy(policy);
            if show_dock {
                #[allow(deprecated)]
                ns_app.activateIgnoringOtherApps(true);
            }
        }
        // El panel del avatar solo entra en un espacio a pantalla completa
        // si se crea con la política ya aplicada.
        crate::bubble_panel::recreate_after_policy();
    });
}

/// Remembers whether the Dock icon should stay visible. Applied on the next policy change.
pub fn remember_dock_icon(show: bool) {
    SHOW_DOCK.store(show, Ordering::Relaxed);
}

/// Shows or hides the Dock icon and recreates the floating avatar under that policy.
pub fn apply_dock_icon(app: &AppHandle, show: bool) {
    remember_dock_icon(show);
    #[cfg(target_os = "macos")]
    set_menu_bar_policy(app);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

/// Asks macOS for notification permission. Blocks: call it off the main thread.
#[cfg(target_os = "macos")]
pub fn request_notification_auth(app: &AppHandle) -> Result<bool, mac_usernotifications::Error> {
    as_regular_app(app, mac_usernotifications::blocking::request_auth)
}

/// Hides the Dock icon once the app is running. Notification permission is asked
/// from the welcome window or from "Probar aviso", never on its own at launch.
#[cfg(target_os = "macos")]
pub fn enter_menu_bar(app: &AppHandle) {
    set_menu_bar_policy(app);
    #[cfg(target_os = "macos")]
    {
        let _ = COMPANION_APP.set(app.clone());
        watch_system_theme();
        watch_bubble_front();
        lift_bubble(app);
    }
}

/// Opens the welcome window as a normal app, so macOS shows a Dock icon
/// until that window closes.
pub fn open_welcome(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window("welcome").is_some() {
        return Ok(());
    }
    WELCOME_OPEN.store(true, Ordering::Relaxed);
    show_in_dock(app);
    let window = tauri::WebviewWindowBuilder::new(app, "welcome", tauri::WebviewUrl::App("welcome.html".into()))
        .title("Ritmo")
        .inner_size(880.0, 640.0)
        .resizable(false)
        .maximizable(false)
        .center()
        .focused(true)
        .visible(true)
        .build()?;
    let _ = window.set_focus();
    Ok(())
}

/// The welcome window is gone: return to the menu-bar app and hide the Dock icon.
pub fn release_welcome(app: &AppHandle) {
    WELCOME_OPEN.store(false, Ordering::Relaxed);
    #[cfg(target_os = "macos")]
    set_menu_bar_policy(app);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

fn show_in_dock(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

        if let Some(marker) = MainThreadMarker::new() {
            let ns_app = NSApplication::sharedApplication(marker);
            let _ = ns_app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
            #[allow(deprecated)]
            ns_app.activateIgnoringOtherApps(true);
            return;
        }
        let _ = app.run_on_main_thread(move || {
            if let Some(marker) = MainThreadMarker::new() {
                let ns_app = NSApplication::sharedApplication(marker);
                let _ = ns_app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
                #[allow(deprecated)]
                ns_app.activateIgnoringOtherApps(true);
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoApp {
    pub name: String,
    pub abbr: String,
    pub color: String,
    pub status: String,
}

pub fn video_apps() -> Vec<VideoApp> {
    fn installed(names: &[&str]) -> bool {
        let home = std::env::var("HOME").unwrap_or_default();
        let roots = [
            "/Applications".to_string(),
            "/System/Applications".to_string(),
            format!("{home}/Applications"),
        ];
        names.iter().any(|name| roots.iter().any(|root| std::path::Path::new(root).join(name).is_dir()))
    }
    let row = |name: &str, abbr: &str, color: &str, names: &[&str], browser: bool| VideoApp {
        name: name.to_string(),
        abbr: abbr.to_string(),
        color: color.to_string(),
        status: if browser {
            "navegador".into()
        } else if installed(names) {
            "instalada".into()
        } else {
            "no instalada".into()
        },
    };
    vec![
        row("Zoom", "Z", "#2d8cff", &["zoom.us.app"], false),
        row(
            "Microsoft Teams",
            "T",
            "#5b5fc7",
            &["Microsoft Teams.app", "Microsoft Teams (work or school).app"],
            false,
        ),
        row("Slack", "S", "#e01e5a", &["Slack.app"], false),
        row("Webex", "W", "#00a0d1", &["Cisco Webex Meetings.app", "Webex.app"], false),
        row("Google Meet", "M", "#1e8e3e", &[], true),
    ]
}

pub fn note_panel_focused() {
    PANEL_FOCUSED.store(true, Ordering::Relaxed);
    HIDE_TICK.fetch_add(1, Ordering::Relaxed);
}

pub fn should_hide_on_blur() -> bool {
    PANEL_FOCUSED.load(Ordering::Relaxed) && !panel_just_shown()
}

fn panel_just_shown() -> bool {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    now.saturating_sub(PANEL_SHOWN_MS.load(Ordering::Relaxed)) < 800
}

pub fn install(app: &mut tauri::App, shortcut: &str) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Abrir Ritmo", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Salir de Ritmo", true, Some("CmdOrCtrl+Q"))?;
    let menu = Menu::with_items(app, &[&open, &quit])?;

    TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
        .icon_as_template(true)
        .tooltip("Ritmo")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => toggle_panel(app, None),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                toggle_panel(tray.app_handle(), Some(rect));
            }
        })
        .build(app)?;

    #[cfg(desktop)]
    {
        use tauri_plugin_global_shortcut::ShortcutState;
        app.handle().plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _incoming, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle_panel(app, None);
                    }
                })
                .build(),
        )?;
        if let Err(err) = register_panel_shortcut(app.handle(), shortcut) {
            eprintln!("No se pudo registrar el atajo: {err}");
        }
    }
    #[cfg(not(desktop))]
    let _ = shortcut;
    Ok(())
}

pub fn register_panel_shortcut(app: &AppHandle, spec: &str) -> Result<(), String> {
    #[cfg(desktop)]
    {
        use std::str::FromStr;
        use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
        let shortcuts = app.global_shortcut();
        let _ = shortcuts.unregister_all();
        if spec.trim().is_empty() {
            return Ok(());
        }
        let shortcut = Shortcut::from_str(spec).map_err(|_| {
            "Esa combinación no se puede usar como atajo.".to_string()
        })?;
        shortcuts.register(shortcut).map_err(|_| {
            "Esa combinación ya la usa el sistema u otra app.".to_string()
        })?;
    }
    #[cfg(not(desktop))]
    let _ = (app, spec);
    Ok(())
}

#[derive(Clone, Copy)]
struct ScreenBox {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    menu: f64,
    scale: f64,
}

fn place_panel(app: &AppHandle, win: &WebviewWindow, rect: Option<Rect>) {
    let screens = screen_boxes(app);
    let Some(screen) = choose_screen(app, &screens, rect.as_ref()) else {
        return;
    };
    let win_scale = win.scale_factor().unwrap_or(screen.scale).max(0.1);
    let outer = win
        .outer_size()
        .unwrap_or(PhysicalSize::new((AGENDA_W * win_scale) as u32, (PANEL_H * win_scale) as u32));
    let width = outer.width as f64 / win_scale;
    let height = outer.height as f64 / win_scale;
    let icon = rect.as_ref().and_then(|rect| icon_point(rect, &screens));
    let mut x = if icon.is_some_and(|icon| contains_screen(&screen, icon.0, icon.1)) {
        let (cx, _) = icon.unwrap_or((0.0, 0.0));
        cx - (width - PANEL_ANCHOR)
    } else {
        screen.x + screen.width - width - 8.0
    };
    let mut y = screen.y + screen.menu + MENU_GAP;
    let max_x = (screen.x + screen.width - width).max(screen.x);
    let max_y = (screen.y + screen.height - height).max(screen.y);
    x = x.clamp(screen.x, max_x);
    y = y.clamp(screen.y, max_y);
    move_window(win, x, y);
}

fn choose_screen(app: &AppHandle, screens: &[ScreenBox], rect: Option<&Rect>) -> Option<ScreenBox> {
    if screens.is_empty() {
        return None;
    }
    if let Some(rect) = rect {
        if let Some((x, y)) = icon_point(rect, screens) {
            if let Some(screen) = screens.iter().copied().find(|screen| contains_screen(screen, x, y)) {
                return Some(screen);
            }
        }
    }
    focused_screen(screens)
        .or_else(|| cursor_screen(app, screens))
        .or_else(|| screens.first().copied())
}

fn contains_screen(screen: &ScreenBox, x: f64, y: f64) -> bool {
    x >= screen.x && x < screen.x + screen.width && y >= screen.y && y < screen.y + screen.height
}

fn icon_point(rect: &Rect, screens: &[ScreenBox]) -> Option<(f64, f64)> {
    use tauri::{PhysicalPosition, PhysicalSize, Position, Size};
    let (x, y, w, h, physical) = match (rect.position, rect.size) {
        (Position::Physical(PhysicalPosition { x, y }), Size::Physical(PhysicalSize { width, height })) => {
            (x as f64, y as f64, width as f64, height as f64, true)
        }
        (Position::Physical(PhysicalPosition { x, y }), Size::Logical(size)) => {
            (x as f64, y as f64, size.width, size.height, true)
        }
        (Position::Logical(pos), Size::Logical(size)) => (pos.x, pos.y, size.width, size.height, false),
        (Position::Logical(pos), Size::Physical(PhysicalSize { width, height })) => {
            (pos.x, pos.y, width as f64, height as f64, false)
        }
    };
    if !physical {
        return Some((x + w / 2.0, y + h / 2.0));
    }
    screens.iter().find_map(|screen| {
        let scale = screen.scale.max(0.1);
        let cx = x / scale + (w / scale) / 2.0;
        let cy = y / scale + (h / scale) / 2.0;
        contains_screen(screen, cx, cy).then_some((cx, cy))
    })
}

fn screen_boxes(app: &AppHandle) -> Vec<ScreenBox> {
    #[cfg(target_os = "macos")]
    if let Some(boxes) = macos_screens() {
        if !boxes.is_empty() {
            return boxes;
        }
    }
    monitor_boxes(app)
}

fn monitor_boxes(app: &AppHandle) -> Vec<ScreenBox> {
    app.available_monitors()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|monitor| {
            let scale = monitor.scale_factor();
            if scale <= 0.0 {
                return None;
            }
            Some(ScreenBox {
                x: monitor.position().x as f64 / scale,
                y: monitor.position().y as f64 / scale,
                width: monitor.size().width as f64 / scale,
                height: monitor.size().height as f64 / scale,
                menu: 0.0,
                scale,
            })
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn display_pixels_high() -> f64 {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGMainDisplayID() -> u32;
        fn CGDisplayPixelsHigh(display: u32) -> u32;
    }
    unsafe { CGDisplayPixelsHigh(CGMainDisplayID()) as f64 }
}

#[cfg(target_os = "macos")]
fn macos_screens() -> Option<Vec<ScreenBox>> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSScreen;

    let marker = MainThreadMarker::new()?;
    let screens = NSScreen::screens(marker);
    let count = screens.count();
    if count == 0 {
        return None;
    }
    let flip = display_pixels_high();
    let mut boxes = Vec::with_capacity(count);
    for index in 0..count {
        let screen = screens.objectAtIndex(index);
        let frame = screen.frame();
        let visible = screen.visibleFrame();
        let top = frame.origin.y + frame.size.height;
        let visible_top = visible.origin.y + visible.size.height;
        boxes.push(ScreenBox {
            x: frame.origin.x,
            y: flip - top,
            width: frame.size.width,
            height: frame.size.height,
            menu: (top - visible_top).max(0.0),
            scale: screen.backingScaleFactor().max(1.0),
        });
    }
    Some(boxes)
}

#[cfg(target_os = "macos")]
fn focused_screen(screens: &[ScreenBox]) -> Option<ScreenBox> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSScreen;

    let marker = MainThreadMarker::new()?;
    let main = NSScreen::mainScreen(marker)?;
    let frame = main.frame();
    let x = frame.origin.x;
    let y = display_pixels_high() - (frame.origin.y + frame.size.height);
    screens.iter().copied().find(|screen| (screen.x - x).abs() < 2.0 && (screen.y - y).abs() < 2.0)
}

#[cfg(not(target_os = "macos"))]
fn focused_screen(_screens: &[ScreenBox]) -> Option<ScreenBox> {
    None
}

#[cfg(target_os = "macos")]
fn cursor_screen(_app: &AppHandle, screens: &[ScreenBox]) -> Option<ScreenBox> {
    use objc2_app_kit::NSEvent;

    let point = NSEvent::mouseLocation();
    let x = point.x;
    let y = display_pixels_high() - point.y;
    screens.iter().copied().find(|screen| contains_screen(screen, x, y))
}

#[cfg(not(target_os = "macos"))]
fn cursor_screen(app: &AppHandle, screens: &[ScreenBox]) -> Option<ScreenBox> {
    let point = app.cursor_position().ok()?;
    let (x, y) = screens.iter().find_map(|screen| {
        let scale = screen.scale.max(0.1);
        let x = point.x / scale;
        let y = point.y / scale;
        contains_screen(screen, x, y).then_some((x, y))
    })?;
    screens.iter().copied().find(|screen| contains_screen(screen, x, y))
}

fn move_window(win: &WebviewWindow, x: f64, y: f64) {
    #[cfg(target_os = "macos")]
    {
        use objc2::rc::Retained;
        use objc2_app_kit::NSWindow;
        use objc2_foundation::NSPoint;

        if let Ok(ptr) = win.ns_window() {
            if !ptr.is_null() {
                if let Some(ns) = unsafe { Retained::retain(ptr.cast::<NSWindow>()) } {
                    let frame = ns.frame();
                    let origin_y = display_pixels_high() - y - frame.size.height;
                    ns.setFrameOrigin(NSPoint::new(x, origin_y));
                    return;
                }
            }
        }
    }
    let _ = win.set_position(LogicalPosition::new(x, y));
}

fn toggle_panel(app: &AppHandle, rect: Option<Rect>) {
    let Some(win) = app.get_webview_window("panel") else {
        return;
    };
    HIDE_TICK.fetch_add(1, Ordering::Relaxed);
    if PANEL_OPEN.load(Ordering::Relaxed) && win.is_visible().unwrap_or(false) {
        hide_panel(app);
        return;
    }
    place_panel(app, &win, rect);
    let _ = win.set_visible_on_all_workspaces(true);
    pin_over_fullscreen(&win);
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    PANEL_SHOWN_MS.store(now, Ordering::Relaxed);
    PANEL_FOCUSED.store(false, Ordering::Relaxed);
    PANEL_OPEN.store(true, Ordering::Relaxed);
    let _ = win.show();
    pin_over_fullscreen(&win);
    let _ = win.set_focus();
    pin_over_fullscreen(&win);
    restore_owned(app);
    pin_panel_again(app);
}

fn pin_panel_again(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(250));
        let later = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(win) = later.get_webview_window("panel") {
                if win.is_visible().unwrap_or(false) {
                    pin_over_fullscreen(&win);
                }
            }
            for label in ["chat", "gemini", "settings"] {
                if label == "gemini" && GEMINI_BUBBLE.load(Ordering::Relaxed) {
                    continue;
                }
                let Some(win) = later.get_webview_window(label) else {
                    continue;
                };
                if !win.is_visible().unwrap_or(false) {
                    continue;
                }
                if label == "chat" {
                    style_companion(&win);
                } else {
                    style_card(&win);
                }
            }
        });
    });
}

pub fn hide_panel(app: &AppHandle) {
    PANEL_OPEN.store(false, Ordering::Relaxed);
    HIDE_TICK.fetch_add(1, Ordering::Relaxed);
    hide_owned(app, "chat");
    if !GEMINI_BUBBLE.load(Ordering::Relaxed) {
        hide_owned(app, "gemini");
    }
    hide_owned(app, "settings");
    let Some(win) = app.get_webview_window("panel") else {
        return;
    };
    let _ = win.hide();
}

pub fn schedule_hide(app: &AppHandle) {
    let generation = HIDE_TICK.fetch_add(1, Ordering::Relaxed) + 1;
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(180));
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            if HIDE_TICK.load(Ordering::Relaxed) != generation || !should_hide_on_blur() {
                return;
            }
            if surface_focused(&handle) {
                return;
            }
            hide_panel(&handle);
        });
    });
}

fn surface_focused(app: &AppHandle) -> bool {
    ["panel", "chat", "gemini", "settings"].iter().any(|label| {
        app.get_webview_window(label)
            .and_then(|win| win.is_focused().ok())
            .unwrap_or(false)
    })
}

fn pin_over_fullscreen(win: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    {
        use objc2::rc::Retained;
        use objc2_app_kit::{NSPopUpMenuWindowLevel, NSWindow, NSWindowCollectionBehavior};

        let Ok(ptr) = win.ns_window() else {
            return;
        };
        if ptr.is_null() {
            return;
        }
        let Some(ns) = (unsafe { Retained::retain(ptr.cast::<NSWindow>()) }) else {
            return;
        };
        // CanJoinAllSpaces and MoveToActiveSpace together raise an ObjC exception
        // that aborts the process. CanJoinAllApplications is what enters another
        // app's fullscreen space.
        let behavior = NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::CanJoinAllApplications;
        ns.setCollectionBehavior(behavior);
        ns.setLevel(NSPopUpMenuWindowLevel);
        ns.setHidesOnDeactivate(false);
        ns.setMovable(false);
        ns.setMovableByWindowBackground(false);
        ns.orderFrontRegardless();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = win;
    }
}

const CHAT_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.6 Safari/605.1.15";
// Google Chat publica un theme-color claro y macOS lo usa para pintar la barra.
// Lo dejamos en el color del sistema para que la barra no se quede blanca.
const CHAT_CHROME: &str = r#"(function(){var lock=false;function paint(){if(lock)return;lock=true;var dark=matchMedia('(prefers-color-scheme: dark)').matches;var color=dark?'#1c1c1e':'#f5f5f7';var head=document.head||document.documentElement;var nodes=document.querySelectorAll('meta[name="theme-color"]');if(!nodes.length){var meta=document.createElement('meta');meta.setAttribute('name','theme-color');meta.setAttribute('content',color);head.appendChild(meta);}else{nodes.forEach(function(meta){if(meta.getAttribute('content')!==color)meta.setAttribute('content',color);});}document.documentElement.style.colorScheme=dark?'dark':'light';lock=false;}paint();new MutationObserver(paint).observe(document.documentElement,{subtree:true,childList:true,attributes:true,attributeFilter:['content']});matchMedia('(prefers-color-scheme: dark)').addEventListener('change',paint);})();"#;

pub fn show_chat_window(app: &AppHandle) -> Result<(), String> {
    CHAT_OPEN.store(true, Ordering::Relaxed);
    open_owned(
        app,
        "chat",
        "Chat",
        tauri::WebviewUrl::External("https://chat.google.com/".parse().expect("url de chat")),
        520.0,
        720.0,
        360.0,
        480.0,
        Some(CHAT_AGENT),
    )
}

pub fn hide_chat_window(app: &AppHandle) -> Result<(), String> {
    CHAT_OPEN.store(false, Ordering::Relaxed);
    hide_owned(app, "chat");
    let _ = tell(app, "chat-closed");
    Ok(())
}

pub fn show_gemini_window(app: &AppHandle) -> Result<(), String> {
    let was_bubble = GEMINI_BUBBLE.swap(false, Ordering::Relaxed);
    GEMINI_OPEN.store(true, Ordering::Relaxed);
    open_owned(
        app,
        "gemini",
        "Gemini",
        tauri::WebviewUrl::App("gemini.html".into()),
        380.0,
        640.0,
        360.0,
        480.0,
        None,
    )?;
    if let Some(win) = app.get_webview_window("gemini") {
        let _ = win.set_decorations(false);
        let _ = win.set_resizable(true);
        if was_bubble {
            let _ = win.set_size(LogicalSize::new(380.0, 640.0));
            let (x, y) = companion_origin(app, "gemini", 380.0, 640.0);
            let _ = win.set_position(LogicalPosition::new(x, y));
        }
        style_card(&win);
    }
    mark_bubble_chat(app, false);
    Ok(())
}

pub fn show_settings_window(app: &AppHandle) -> Result<(), String> {
    SETTINGS_OPEN.store(true, Ordering::Relaxed);
    open_owned(
        app,
        "settings",
        "Preferencias",
        tauri::WebviewUrl::App("settings.html".into()),
        640.0,
        720.0,
        520.0,
        420.0,
        None,
    )?;
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.set_decorations(false);
        style_card(&win);
    }
    emit_label(app, "settings", "ritmo-surface", "settings-opened");
    Ok(())
}

pub fn hide_settings_window(app: &AppHandle) -> Result<(), String> {
    SETTINGS_OPEN.store(false, Ordering::Relaxed);
    hide_owned(app, "settings");
    let _ = tell(app, "settings-closed");
    Ok(())
}

pub fn hide_gemini_window(app: &AppHandle) -> Result<(), String> {
    GEMINI_BUBBLE.store(false, Ordering::Relaxed);
    GEMINI_OPEN.store(false, Ordering::Relaxed);
    hide_owned(app, "gemini");
    let _ = tell(app, "gemini-closed");
    mark_bubble_chat(app, false);
    Ok(())
}

pub fn gemini_bubble_open() -> bool {
    GEMINI_BUBBLE.load(Ordering::Relaxed)
}

/// Shows the round avatar that floats above other apps.
pub fn show_bubble(app: &AppHandle, saved_x: Option<f64>, saved_y: Option<f64>) -> Result<(), String> {
    let (x, y) = match (saved_x, saved_y) {
        (Some(x), Some(y)) => clamp_avatar(app, x, y),
        _ => default_bubble_origin(app),
    };
    remember_bubble_point(x, y);
    hide_owned(app, "bubble");
    #[cfg(target_os = "macos")]
    {
        let _ = COMPANION_APP.set(app.clone());
        let app = app.clone();
        let _ = app.clone().run_on_main_thread(move || {
            crate::bubble_panel::show(x, y);
            let _ = app;
        });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, x, y);
    Ok(())
}

pub fn hide_bubble(app: &AppHandle) {
    hide_owned(app, "bubble");
    #[cfg(target_os = "macos")]
    crate::bubble_panel::hide();
    if GEMINI_BUBBLE.load(Ordering::Relaxed) {
        let _ = hide_gemini_window(app);
    }
}

pub fn bubble_origin(app: &AppHandle) -> (f64, f64) {
    let _ = app;
    #[cfg(target_os = "macos")]
    if let Some(origin) = crate::bubble_panel::origin() {
        return origin;
    }
    (
        f64::from_bits(BUBBLE_X.load(Ordering::Relaxed)),
        f64::from_bits(BUBBLE_Y.load(Ordering::Relaxed)),
    )
}

pub fn shared_app() -> Option<AppHandle> {
    #[cfg(target_os = "macos")]
    {
        return COMPANION_APP.get().cloned();
    }
    #[cfg(not(target_os = "macos"))]
    None
}

pub fn queue_save_bubble(x: f64, y: f64) {
    remember_bubble_point(x, y);
    let Some(app) = shared_app() else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        if let Some(state) = app.try_state::<crate::application::AppState>() {
            let _ = state.inner.remember_bubble_at(x, y).await;
        }
    });
}

pub fn place_bubble(app: &AppHandle, x: f64, y: f64) {
    let (x, y) = clamp_avatar(app, x, y);
    remember_bubble_point(x, y);
    #[cfg(target_os = "macos")]
    crate::bubble_panel::move_to(x, y);
    if GEMINI_BUBBLE.load(Ordering::Relaxed) {
        place_bubble_chat(app, x, y);
    }
}

pub fn bubble_chat_open(app: &AppHandle) -> bool {
    GEMINI_BUBBLE.load(Ordering::Relaxed)
        && app
            .get_webview_window("gemini")
            .and_then(|win| win.is_visible().ok())
            .unwrap_or(false)
}

pub fn toggle_bubble_chat(app: &AppHandle) -> Result<(), String> {
    if bubble_chat_open(app) {
        return hide_gemini_window(app);
    }
    GEMINI_BUBBLE.store(true, Ordering::Relaxed);
    GEMINI_OPEN.store(true, Ordering::Relaxed);
    let x = f64::from_bits(BUBBLE_X.load(Ordering::Relaxed));
    let y = f64::from_bits(BUBBLE_Y.load(Ordering::Relaxed));
    let (chat_x, chat_y) = bubble_chat_origin(app, x, y);
    if app.get_webview_window("gemini").is_none() {
        let win = tauri::WebviewWindowBuilder::new(app, "gemini", tauri::WebviewUrl::App("gemini.html".into()))
            .title("Gemini")
            .inner_size(BUBBLE_CHAT_W, BUBBLE_CHAT_H)
            .min_inner_size(320.0, 420.0)
            .position(chat_x, chat_y)
            .resizable(false)
            .maximizable(false)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .skip_taskbar(true)
            .focused(true)
            .visible(true)
            .build()
            .map_err(|err| err.to_string())?;
        let _ = win.set_visible_on_all_workspaces(true);
    }
    present_bubble_chat(app, chat_x, chat_y)?;
    Ok(())
}

fn present_bubble_chat(app: &AppHandle, x: f64, y: f64) -> Result<(), String> {
    let Some(win) = app.get_webview_window("gemini") else {
        return Ok(());
    };
    let _ = win.set_decorations(false);
    let _ = win.set_resizable(false);
    let _ = win.set_size(LogicalSize::new(BUBBLE_CHAT_W, BUBBLE_CHAT_H));
    let _ = win.set_position(LogicalPosition::new(x, y));
    let _ = win.show();
    style_chat_head(&win);
    let _ = win.set_focus();
    mark_bubble_chat(app, true);
    lift_bubble_again(app);
    Ok(())
}

fn place_bubble_chat(app: &AppHandle, avatar_x: f64, avatar_y: f64) {
    let (x, y) = bubble_chat_origin(app, avatar_x, avatar_y);
    if let Some(win) = app.get_webview_window("gemini") {
        let _ = win.set_position(LogicalPosition::new(x, y));
    }
}

fn remember_bubble_point(x: f64, y: f64) {
    BUBBLE_X.store(x.to_bits(), Ordering::Relaxed);
    BUBBLE_Y.store(y.to_bits(), Ordering::Relaxed);
}

fn mark_bubble_chat(app: &AppHandle, open: bool) {
    emit_label(app, "gemini", "ritmo-chrome", if open { "bubble" } else { "window" });
    emit_label(app, "bubble", "ritmo-bubble", if open { "open" } else { "closed" });
    #[cfg(target_os = "macos")]
    crate::bubble_panel::redraw();
}

fn emit_label(app: &AppHandle, label: &str, event: &str, payload: &str) {
    use tauri::Emitter;
    if app.get_webview_window(label).is_some() {
        let _ = app.emit_to(label, event, payload);
    }
}

pub fn close_owned(app: &AppHandle, label: &str) {
    match label {
        "chat" => {
            CHAT_OPEN.store(false, Ordering::Relaxed);
            hide_owned(app, "chat");
            let _ = tell(app, "chat-closed");
        }
        "gemini" => {
            let _ = hide_gemini_window(app);
        }
        "settings" => {
            SETTINGS_OPEN.store(false, Ordering::Relaxed);
            hide_owned(app, "settings");
            let _ = tell(app, "settings-closed");
        }
        _ => {}
    }
}

fn open_owned(
    app: &AppHandle,
    label: &str,
    title: &str,
    url: tauri::WebviewUrl,
    width: f64,
    height: f64,
    min_width: f64,
    min_height: f64,
    user_agent: Option<&str>,
) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(label) {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
        style_companion(&win);
        return Ok(());
    }
    let (x, y) = companion_origin(app, label, width, height);
    let mut builder = tauri::WebviewWindowBuilder::new(app, label, url)
        .title(title)
        .inner_size(width, height)
        .min_inner_size(min_width, min_height)
        .position(x, y)
        .resizable(true)
        .maximizable(true)
        .decorations(label != "gemini" && label != "settings")
        .transparent(label == "gemini" || label == "settings")
        .shadow(label != "gemini" && label != "settings")
        .skip_taskbar(true)
        .focused(true)
        .visible(true)
        .on_page_load(|window, payload| {
            if payload.event() == tauri::webview::PageLoadEvent::Finished {
                let window = window.clone();
                let _ = window.clone().run_on_main_thread(move || {
                    if window.label() == "gemini" || window.label() == "settings" {
                        style_card(&window);
                    } else {
                        style_companion(&window);
                    }
                });
            }
        });
    if let Some(agent) = user_agent {
        builder = builder.user_agent(agent).initialization_script(CHAT_CHROME);
    }
    let win = builder.build().map_err(|err| err.to_string())?;
    let _ = win.set_visible_on_all_workspaces(true);
    style_companion(&win);
    Ok(())
}

fn hide_owned(app: &AppHandle, label: &str) {
    if let Some(win) = app.get_webview_window(label) {
        let _ = win.hide();
    }
}

fn restore_owned(app: &AppHandle) {
    if CHAT_OPEN.load(Ordering::Relaxed) {
        place_owned(app, "chat");
    }
    if GEMINI_OPEN.load(Ordering::Relaxed) && !GEMINI_BUBBLE.load(Ordering::Relaxed) {
        place_owned(app, "gemini");
    }
    if SETTINGS_OPEN.load(Ordering::Relaxed) {
        place_owned(app, "settings");
    }
}

fn place_owned(app: &AppHandle, label: &str) {
    let Some(win) = app.get_webview_window(label) else {
        return;
    };
    let scale = win.scale_factor().unwrap_or(1.0).max(0.1);
    let size = win.outer_size().unwrap_or(PhysicalSize::new(360, 480));
    let width = size.width as f64 / scale;
    let height = size.height as f64 / scale;
    let (x, y) = companion_origin(app, label, width, height);
    move_window(&win, x, y);
    let _ = win.show();
    if label == "chat" {
        style_companion(&win);
    } else {
        style_card(&win);
    }
}

fn companion_origin(app: &AppHandle, label: &str, width: f64, height: f64) -> (f64, f64) {
    let _ = height;
    let Some(panel) = app.get_webview_window("panel") else {
        return (80.0, 48.0);
    };
    let scale = panel.scale_factor().unwrap_or(1.0);
    let pos = panel.outer_position().unwrap_or(PhysicalPosition::new(80, 40));
    let panel_x = pos.x as f64 / scale;
    let panel_y = pos.y as f64 / scale;
    let mut x = panel_x - 12.0 - width;
    let mut y = panel_y;
    let beside: &[&str] = match label {
        "settings" => &["gemini", "chat"],
        "gemini" => &["chat"],
        _ => &[],
    };
    for name in beside {
        let Some(other) = app.get_webview_window(name) else {
            continue;
        };
        if !other.is_visible().unwrap_or(false) {
            continue;
        }
        if let Ok(other_pos) = other.outer_position() {
            let other_scale = other.scale_factor().unwrap_or(scale);
            x = other_pos.x as f64 / other_scale - 12.0 - width;
            y = other_pos.y as f64 / other_scale;
        }
        break;
    }
    if let Ok(Some(monitor)) = panel.current_monitor() {
        let origin_x = monitor.position().x as f64 / scale;
        let origin_y = monitor.position().y as f64 / scale;
        x = x.max(origin_x + 8.0);
        y = y.max(origin_y + 28.0);
    }
    (x, y)
}

fn default_bubble_origin(app: &AppHandle) -> (f64, f64) {
    let Some((origin_x, origin_y, width, height)) = monitor_frame(app, 0.0, 0.0) else {
        return (24.0, 80.0);
    };
    (
        origin_x + width - AVATAR_SIZE - 20.0,
        origin_y + height - AVATAR_SIZE - 96.0,
    )
}

fn clamp_avatar(app: &AppHandle, x: f64, y: f64) -> (f64, f64) {
    let Some((origin_x, origin_y, width, height)) = monitor_frame(app, x, y) else {
        return (x, y);
    };
    let margin = 8.0;
    (
        x.clamp(origin_x + margin, origin_x + width - AVATAR_SIZE - margin),
        y.clamp(origin_y + margin, origin_y + height - AVATAR_SIZE - margin),
    )
}

fn bubble_chat_origin(app: &AppHandle, avatar_x: f64, avatar_y: f64) -> (f64, f64) {
    let gap = 12.0;
    let mut x = avatar_x - gap - BUBBLE_CHAT_W;
    let mut y = avatar_y + AVATAR_SIZE - BUBBLE_CHAT_H;
    let Some((origin_x, origin_y, width, height)) = monitor_frame(app, avatar_x, avatar_y) else {
        return (x, y);
    };
    if x < origin_x + 8.0 {
        x = avatar_x + AVATAR_SIZE + gap;
    }
    if x + BUBBLE_CHAT_W > origin_x + width - 8.0 {
        x = (origin_x + width - BUBBLE_CHAT_W - 8.0).max(origin_x + 8.0);
    }
    y = y.clamp(origin_y + 28.0, (origin_y + height - BUBBLE_CHAT_H - 8.0).max(origin_y + 28.0));
    (x, y)
}

fn monitor_frame(app: &AppHandle, x: f64, y: f64) -> Option<(f64, f64, f64, f64)> {
    let monitors = app.available_monitors().ok()?;
    let mut best: Option<(f64, (f64, f64, f64, f64))> = None;
    for monitor in monitors {
        let scale = monitor.scale_factor();
        if scale <= 0.0 {
            continue;
        }
        let origin_x = monitor.position().x as f64 / scale;
        let origin_y = monitor.position().y as f64 / scale;
        let width = monitor.size().width as f64 / scale;
        let height = monitor.size().height as f64 / scale;
        let inside = x >= origin_x && x < origin_x + width && y >= origin_y && y < origin_y + height;
        let distance = (x - (origin_x + width / 2.0)).abs() + (y - (origin_y + height / 2.0)).abs();
        let score = if inside { 0.0 } else { distance + 10_000.0 };
        if best.as_ref().map(|(current, _)| score < *current).unwrap_or(true) {
            best = Some((score, (origin_x, origin_y, width, height)));
        }
    }
    best.map(|(_, frame)| frame)
}

fn lift_bubble(app: &AppHandle) {
    if GEMINI_BUBBLE.load(Ordering::Relaxed) {
        if let Some(win) = app.get_webview_window("gemini") {
            if win.is_visible().unwrap_or(false) {
                style_chat_head(&win);
            }
        }
    }
    #[cfg(target_os = "macos")]
    crate::bubble_panel::lift();
}

fn lift_bubble_again(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(250));
        let later = app.clone();
        let _ = app.run_on_main_thread(move || lift_bubble(&later));
    });
}

fn style_avatar(win: &WebviewWindow) {
    // No llamar a set_always_on_top: en macOS se aplica después y baja
    // la ventana a un nivel que otras apps tapan.
    pin_above_apps(win, false);
}

fn style_chat_head(win: &WebviewWindow) {
    pin_above_apps(win, false);
    // El filete lo pinta la página, igual que la agenda. Una máscara nativa lo recorta.
    round_window(win, 0.0);
}

fn style_card(win: &WebviewWindow) {
    style_companion(win);
    #[cfg(target_os = "macos")]
    {
        use objc2::rc::Retained;
        use objc2_app_kit::{NSColor, NSWindow};

        if let Ok(ptr) = win.ns_window() {
            if !ptr.is_null() {
                if let Some(ns) = unsafe { Retained::retain(ptr.cast::<NSWindow>()) } {
                    ns.setOpaque(false);
                    ns.setHasShadow(false);
                    ns.setBackgroundColor(Some(&NSColor::clearColor()));
                }
            }
        }
    }
    round_window(win, 0.0);
}

fn style_companion(win: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    {
        use objc2::rc::Retained;
        use objc2_app_kit::{NSPopUpMenuWindowLevel, NSWindow, NSWindowCollectionBehavior};

        let Ok(ptr) = win.ns_window() else {
            return;
        };
        if ptr.is_null() {
            return;
        }
        let Some(ns) = (unsafe { Retained::retain(ptr.cast::<NSWindow>()) }) else {
            return;
        };
        let behavior = NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::CanJoinAllApplications;
        ns.setCollectionBehavior(behavior);
        ns.setLevel(NSPopUpMenuWindowLevel);
        ns.setHidesOnDeactivate(false);
        ns.orderFrontRegardless();
        ns.setMovable(true);
        ns.setHasShadow(true);
        ns.setTitlebarAppearsTransparent(false);
        ns.setOpaque(true);
        apply_system_chrome(&ns);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = win;
    }
}

#[cfg(target_os = "macos")]
static COMPANION_APP: OnceLock<AppHandle> = OnceLock::new();

#[cfg(target_os = "macos")]
fn system_is_dark() -> bool {
    use objc2_foundation::{NSString, NSUserDefaults};

    let style = NSUserDefaults::standardUserDefaults().stringForKey(&NSString::from_str("AppleInterfaceStyle"));
    style.is_some_and(|value| value.to_string() == "Dark")
}

#[cfg(target_os = "macos")]
fn apply_system_chrome(ns: &objc2_app_kit::NSWindow) {
    use objc2_app_kit::{NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua};

    let name = if system_is_dark() {
        unsafe { &*NSAppearanceNameDarkAqua }
    } else {
        unsafe { &*NSAppearanceNameAqua }
    };
    if let Some(appearance) = NSAppearance::appearanceNamed(name) {
        ns.setAppearance(Some(&appearance));
    }
    // El color de la página no debe reemplazar el de la barra.
    ns.setBackgroundColor(Some(&objc2_app_kit::NSColor::windowBackgroundColor()));
}

#[cfg(target_os = "macos")]
fn restyle_companions(app: &AppHandle) {
    for label in ["chat", "gemini", "settings"] {
        if let Some(win) = app.get_webview_window(label) {
            if label == "gemini" && GEMINI_BUBBLE.load(Ordering::Relaxed) {
                style_chat_head(&win);
            } else if label == "gemini" || label == "settings" {
                style_card(&win);
            } else {
                style_companion(&win);
            }
        }
    }
    if let Some(win) = app.get_webview_window("bubble") {
        style_avatar(&win);
    }
}

#[cfg(target_os = "macos")]
fn pin_above_apps(win: &WebviewWindow, round: bool) {
    use objc2::rc::Retained;
    use objc2_app_kit::{NSColor, NSScreenSaverWindowLevel, NSWindow, NSWindowCollectionBehavior};

    let Ok(ptr) = win.ns_window() else {
        return;
    };
    if ptr.is_null() {
        return;
    }
    let Some(ns) = (unsafe { Retained::retain(ptr.cast::<NSWindow>()) }) else {
        return;
    };
    // Stationary lo mantiene visible al cambiar de espacio. CanJoinAllApplications
    // lo mete en el espacio de otra app a pantalla completa. No combinar con
    // MoveToActiveSpace: junto a CanJoinAllSpaces aborta el proceso.
    let behavior = NSWindowCollectionBehavior::CanJoinAllSpaces
        | NSWindowCollectionBehavior::FullScreenAuxiliary
        | NSWindowCollectionBehavior::CanJoinAllApplications
        | NSWindowCollectionBehavior::Stationary
        | NSWindowCollectionBehavior::IgnoresCycle;
    ns.setCollectionBehavior(behavior);
    ns.setLevel(NSScreenSaverWindowLevel);
    ns.setHidesOnDeactivate(false);
    ns.setCanHide(false);
    ns.setMovable(false);
    ns.setMovableByWindowBackground(false);
    ns.setOpaque(false);
    ns.setHasShadow(round);
    ns.setBackgroundColor(Some(&NSColor::clearColor()));
    ns.orderFrontRegardless();
}

#[cfg(not(target_os = "macos"))]
fn pin_above_apps(win: &WebviewWindow, _round: bool) {
    let _ = win.set_always_on_top(true);
}

fn round_window(win: &WebviewWindow, radius: f64) {
    #[cfg(target_os = "macos")]
    {
        use objc2::rc::Retained;
        use objc2_app_kit::NSWindow;

        let Ok(ptr) = win.ns_window() else {
            return;
        };
        if ptr.is_null() {
            return;
        }
        let Some(ns) = (unsafe { Retained::retain(ptr.cast::<NSWindow>()) }) else {
            return;
        };
        let Some(view) = ns.contentView() else {
            return;
        };
        view.setWantsLayer(true);
        if let Some(layer) = view.layer() {
            layer.setCornerRadius(radius);
            layer.setMasksToBounds(radius > 0.0);
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (win, radius);
    }
}

#[cfg(target_os = "macos")]
fn watch_system_theme() {
    use objc2::rc::{Allocated, Retained};
    use objc2::runtime::{AnyObject, NSObject};
    use objc2::{define_class, msg_send, AnyThread};
    use objc2_foundation::{
        NSDistributedNotificationCenter, NSNotificationSuspensionBehavior, NSString,
    };

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "RitmoChromeObserver"]
        struct ChromeObserver;

        impl ChromeObserver {
            #[unsafe(method_id(init))]
            fn init(this: Allocated<Self>) -> Retained<Self> {
                let this = this.set_ivars(());
                unsafe { msg_send![super(this), init] }
            }

            #[unsafe(method(themeChanged:))]
            fn theme_changed(&self, _notification: Option<&AnyObject>) {
                let Some(app) = COMPANION_APP.get() else {
                    return;
                };
                let app = app.clone();
                let _ = app.clone().run_on_main_thread(move || restyle_companions(&app));
            }
        }
    );

    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::Relaxed) {
        return;
    }
    let observer: Retained<ChromeObserver> = unsafe { msg_send![ChromeObserver::alloc(), init] };
    let name = NSString::from_str("AppleInterfaceThemeChangedNotification");
    unsafe {
        NSDistributedNotificationCenter::defaultCenter()
            .addObserver_selector_name_object_suspensionBehavior(
                &observer,
                objc2::sel!(themeChanged:),
                Some(&name),
                None,
                NSNotificationSuspensionBehavior::DeliverImmediately,
            );
    }
    std::mem::forget(observer);
}

#[cfg(target_os = "macos")]
fn watch_bubble_front() {
    use objc2::rc::{Allocated, Retained};
    use objc2::runtime::{AnyObject, NSObject};
    use objc2::{define_class, msg_send, AnyThread};
    use objc2_app_kit::{
        NSApplicationDidResignActiveNotification, NSWorkspace, NSWorkspaceActiveSpaceDidChangeNotification,
    };
    use objc2_foundation::NSNotificationCenter;

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "RitmoBubbleObserver"]
        struct BubbleObserver;

        impl BubbleObserver {
            #[unsafe(method_id(init))]
            fn init(this: Allocated<Self>) -> Retained<Self> {
                let this = this.set_ivars(());
                unsafe { msg_send![super(this), init] }
            }

            #[unsafe(method(appResigned:))]
            fn app_resigned(&self, _notification: Option<&AnyObject>) {
                lift_from_observer();
            }

            #[unsafe(method(spaceChanged:))]
            fn space_changed(&self, _notification: Option<&AnyObject>) {
                lift_from_observer();
            }
        }
    );

    fn lift_from_observer() {
        let Some(app) = COMPANION_APP.get() else {
            return;
        };
        let app = app.clone();
        let _ = app.clone().run_on_main_thread(move || lift_bubble(&app));
    }

    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::Relaxed) {
        return;
    }
    let observer: Retained<BubbleObserver> = unsafe { msg_send![BubbleObserver::alloc(), init] };
    unsafe {
        NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
            &observer,
            objc2::sel!(appResigned:),
            Some(NSApplicationDidResignActiveNotification),
            None,
        );
        NSWorkspace::sharedWorkspace()
            .notificationCenter()
            .addObserver_selector_name_object(
                &observer,
                objc2::sel!(spaceChanged:),
                Some(NSWorkspaceActiveSpaceDidChangeNotification),
                None,
            );
    }
    std::mem::forget(observer);
}

fn tell(app: &AppHandle, message: &str) -> tauri::Result<()> {
    use tauri::Emitter;
    app.emit_to("panel", "ritmo-surface", message)
}
