use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, PhysicalPosition, PhysicalSize, Rect, WebviewWindow,
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
static SETTINGS_OPEN: AtomicBool = AtomicBool::new(false);
static HIDE_TICK: AtomicU64 = AtomicU64::new(0);
static WELCOME_OPEN: AtomicBool = AtomicBool::new(false);

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

    let _ = app.run_on_main_thread(move || {
        if let Some(marker) = MainThreadMarker::new() {
            let ns_app = NSApplication::sharedApplication(marker);
            let _ = ns_app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        }
    });
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
        let shortcut = Shortcut::from_str(spec).map_err(|_| {
            "Esa combinación no se puede usar como atajo.".to_string()
        })?;
        let shortcuts = app.global_shortcut();
        let _ = shortcuts.unregister_all();
        shortcuts.register(shortcut).map_err(|_| {
            "Esa combinación ya la usa el sistema u otra app.".to_string()
        })?;
    }
    #[cfg(not(desktop))]
    let _ = (app, spec);
    Ok(())
}

fn place(win: &WebviewWindow, rect: Rect) {
    let scale = win.scale_factor().unwrap_or(1.0);
    let position: PhysicalPosition<f64> = rect.position.to_physical(scale);
    let size: PhysicalSize<f64> = rect.size.to_physical(scale);
    let outer = win.outer_size().unwrap_or(PhysicalSize::new(AGENDA_W as u32, PANEL_H as u32));
    let (width, height) = (outer.width as f64, outer.height as f64);
    let mut x = position.x + size.width / 2.0 - (width - PANEL_ANCHOR * scale);
    let mut y = position.y + size.height + MENU_GAP * scale;
    if let Ok(Some(monitor)) = win.current_monitor() {
        let (origin_x, origin_y) = (monitor.position().x as f64, monitor.position().y as f64);
        let (monitor_w, monitor_h) = (monitor.size().width as f64, monitor.size().height as f64);
        if position.y > origin_y + monitor_h / 2.0 {
            y = position.y - height - MENU_GAP * scale;
        }
        x = x.max(origin_x).min(origin_x + monitor_w - width);
    }
    let _ = win.set_position(PhysicalPosition::new(x, y));
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
    if let Some(rect) = rect {
        place(&win, rect);
    }
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
}

pub fn hide_panel(app: &AppHandle) {
    PANEL_OPEN.store(false, Ordering::Relaxed);
    HIDE_TICK.fetch_add(1, Ordering::Relaxed);
    hide_owned(app, "chat");
    hide_owned(app, "gemini");
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
        use objc2_app_kit::{NSFloatingWindowLevel, NSWindow, NSWindowCollectionBehavior};

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
        ns.setLevel(NSFloatingWindowLevel);
        ns.setHidesOnDeactivate(false);
        ns.setMovable(false);
        ns.setMovableByWindowBackground(false);
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
    )
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
    )
}

pub fn hide_settings_window(app: &AppHandle) -> Result<(), String> {
    SETTINGS_OPEN.store(false, Ordering::Relaxed);
    hide_owned(app, "settings");
    let _ = tell(app, "settings-closed");
    Ok(())
}

pub fn hide_gemini_window(app: &AppHandle) -> Result<(), String> {
    GEMINI_OPEN.store(false, Ordering::Relaxed);
    hide_owned(app, "gemini");
    let _ = tell(app, "gemini-closed");
    Ok(())
}

pub fn close_owned(app: &AppHandle, label: &str) {
    match label {
        "chat" => {
            CHAT_OPEN.store(false, Ordering::Relaxed);
            hide_owned(app, "chat");
            let _ = tell(app, "chat-closed");
        }
        "gemini" => {
            GEMINI_OPEN.store(false, Ordering::Relaxed);
            hide_owned(app, "gemini");
            let _ = tell(app, "gemini-closed");
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
        .decorations(true)
        .transparent(false)
        .shadow(true)
        .skip_taskbar(true)
        .focused(true)
        .visible(true)
        .on_page_load(|window, payload| {
            if payload.event() == tauri::webview::PageLoadEvent::Finished {
                let window = window.clone();
                let _ = window.clone().run_on_main_thread(move || style_companion(&window));
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
        if let Some(win) = app.get_webview_window("chat") {
            let _ = win.show();
        }
    }
    if GEMINI_OPEN.load(Ordering::Relaxed) {
        if let Some(win) = app.get_webview_window("gemini") {
            let _ = win.show();
        }
    }
    if SETTINGS_OPEN.load(Ordering::Relaxed) {
        if let Some(win) = app.get_webview_window("settings") {
            let _ = win.show();
        }
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

fn style_companion(win: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    {
        use objc2::rc::Retained;
        use objc2_app_kit::{NSFloatingWindowLevel, NSWindow, NSWindowCollectionBehavior};

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
        ns.setLevel(NSFloatingWindowLevel);
        ns.setHidesOnDeactivate(false);
        ns.setMovable(true);
        ns.setTitlebarAppearsTransparent(false);
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
            style_companion(&win);
        }
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

fn tell(app: &AppHandle, message: &str) -> tauri::Result<()> {
    use tauri::Emitter;
    app.emit_to("panel", "ritmo-surface", message)
}
