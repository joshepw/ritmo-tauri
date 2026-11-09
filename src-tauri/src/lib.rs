mod application;
mod commands;
mod domain;
mod infrastructure;
mod shell;

use tauri::{Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let state = application::AppState::bootstrap(app.handle());
            app.manage(state.clone());
            let shortcut = state.panel_shortcut();
            shell::install(app, &shortcut)?;
            if !state.setup_complete() {
                shell::open_welcome(app.handle())?;
            }
            state.spawn();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::snapshot,
            commands::sign_in,
            commands::sign_out,
            commands::set_preferences,
            commands::set_calendar_visible,
            commands::add_task,
            commands::toggle_task,
            commands::add_note,
            commands::pomodoro_action,
            commands::open_meeting,
            commands::assistant_summary,
            commands::assistant_ask,
            commands::set_gemini_key,
            commands::clear_gemini_key,
            commands::set_factorial_key,
            commands::clear_factorial_key,
            commands::set_clickup_key,
            commands::clear_clickup_key,
            commands::show_chat_window,
            commands::hide_chat_window,
            commands::show_gemini_window,
            commands::hide_gemini_window,
            commands::show_settings_window,
            commands::hide_settings_window,
            commands::preview_notification,
            commands::finish_setup,
            commands::request_notifications,
            commands::notifications_allowed,
            commands::video_apps,
            commands::check_update,
            commands::install_update,
            commands::quit
        ])
        .on_window_event(|window, event| {
            if window.label() == "welcome" {
                if let WindowEvent::Destroyed = event {
                    shell::release_welcome(window.app_handle());
                }
                return;
            }
            let label = window.label();
            if label != "panel" && label != "chat" && label != "gemini" && label != "settings" {
                return;
            }
            match event {
                WindowEvent::Focused(true) => shell::note_panel_focused(),
                WindowEvent::Focused(false) if shell::should_hide_on_blur() => {
                    shell::schedule_hide(window.app_handle());
                }
                WindowEvent::CloseRequested { api, .. } if label != "panel" => {
                    api.prevent_close();
                    shell::close_owned(window.app_handle(), label);
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!())
        .expect("error al iniciar Ritmo")
        .run(|app, event| {
            if let tauri::RunEvent::Ready = event {
                #[cfg(target_os = "macos")]
                shell::enter_menu_bar(app);
            }
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app.try_state::<application::AppState>() {
                    let inner = state.inner.clone();
                    tauri::async_runtime::block_on(async {
                        let _ = inner.persist().await;
                    });
                }
            }
        });
}
