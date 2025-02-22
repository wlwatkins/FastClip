use anyhow::Result;
use log::debug;
use tauri::menu::Menu;
use tauri::menu::MenuItem;
use tauri::tray::MouseButton;
use tauri::tray::TrayIconBuilder;
use tauri::tray::TrayIconEvent;
use tauri::AppHandle;
use tauri::Manager;
use tauri::WindowEvent;
use tauri_plugin_notification::NotificationExt;

pub fn setup_tray(app: &AppHandle) -> Result<()> {
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&quit_item])?;

    let _tray = TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            match event.id.0.as_str() {
                "quit" => {
                    app.exit(0);
                }
                _ => {
                    debug!("Unhandled menu item: {:?}", event.id);
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            let app = tray.app_handle();
            match event {
                // Left Double Click: Toggle Window Visibility
                TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } => {
                    if let Some(window) = app.get_webview_window("main") {
                        if window.is_visible().unwrap_or(false) {
                            let _ = window.hide();
                        } else {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                }
        
                _ => {}
            }
        })
        .icon(app.default_window_icon().unwrap().clone())
        .build(app)?;

    // Listen for window close event
    if let Some(window) = app.get_webview_window("main") {
        let app_handle = app.clone();
        window.on_window_event(move |event| {
            if let WindowEvent::CloseRequested {
                api, ..
            } = event
            {
                api.prevent_close(); // Prevents the app from quitting
                let main_window = app_handle.get_webview_window("main").unwrap();
                let _ = main_window.hide();
                let _ = app_handle.notification()
                .builder()
                .title("FastClip")
                .body("FastClip is still running in your system tray. Double click to show window.")
                .show();
            }
        });
    }

    Ok(())
}
