use anyhow::Result;
use tauri::menu::Menu;
use tauri::menu::MenuItem;
use tauri::tray::MouseButton;
use tauri::tray::MouseButtonState;
use tauri::tray::TrayIconBuilder;
use tauri::tray::TrayIconEvent;
use tauri::AppHandle;
use tauri::Manager;
use tauri::WindowEvent;

pub fn setup_tray(app: &AppHandle) -> Result<()> {
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&quit_item])?;

    let _tray = TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            match event.id.0.as_str() {
                "quit" => {
                    println!("Quit menu item clicked");
                    app.exit(0);
                }
                _ => {
                    println!("Unhandled menu item: {:?}", event.id);
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            match event {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => {
                    println!("Tray icon left-clicked");
                    let app = tray.app_handle();
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                _ => (),
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
                println!("Main window close requested, hiding instead of closing.");
                api.prevent_close(); // Prevents the app from quitting
                let main_window = app_handle.get_webview_window("main").unwrap();
                let _ = main_window.hide();
            }
        });
    }

    Ok(())
}
