use tauri::{AppHandle, Manager};

use crate::{
    desktop_integration::DesktopIntegration,
    window::{
        current_logical_height, finish_native_panel_resize,
        fit_panel_to_content as fit_native_panel_to_content, hide_main_window,
        lock_native_panel_resize_axis, panel_resize_edge, prepare_native_panel_resize,
        PanelHeightMode, PanelResizeEdge, PanelResizeSession, MAIN_WINDOW,
    },
};

#[tauri::command]
pub fn dismiss_main_window(app: AppHandle) {
    let integration = app.state::<DesktopIntegration>();
    if integration.exits_on_close() {
        if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
            finish_native_panel_resize(&window);
        }
        app.exit(0);
    } else if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        hide_main_window(&window);
    }
}

#[tauri::command]
pub fn get_panel_resize_edge(app: AppHandle) -> Result<PanelResizeEdge, String> {
    let window = app
        .get_webview_window(MAIN_WINDOW)
        .ok_or("UsageDeck window is unavailable.")?;
    panel_resize_edge(&window)
}

#[tauri::command]
pub fn get_panel_height_mode(app: AppHandle) -> PanelHeightMode {
    app.state::<std::sync::Arc<PanelResizeSession>>().mode()
}

#[tauri::command]
pub fn fit_panel_to_content(app: AppHandle, height: u32) -> Result<bool, String> {
    let window = app
        .get_webview_window(MAIN_WINDOW)
        .ok_or("UsageDeck window is unavailable.")?;
    fit_native_panel_to_content(&window, height.max(1))
}

// Like the other persistence-bearing commands, panel height changes write
// through SQLite; run them on the blocking pool so a refresh-batch write
// holding the storage mutex cannot freeze the panel on the main thread.
#[tauri::command]
pub async fn set_panel_height_automatic(app: AppHandle) -> Result<(), String> {
    let session = app
        .state::<std::sync::Arc<PanelResizeSession>>()
        .inner()
        .clone();
    tauri::async_runtime::spawn_blocking(move || session.set_automatic())
        .await
        .map_err(|_| "UsageDeck panel height mode could not be saved.".to_owned())?
}

#[tauri::command]
pub async fn set_panel_height_manual(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(MAIN_WINDOW)
        .ok_or("UsageDeck window is unavailable.")?;
    let session = app
        .state::<std::sync::Arc<PanelResizeSession>>()
        .inner()
        .clone();
    let height = current_logical_height(&window.as_ref().window())
        .ok_or("UsageDeck content size is unavailable.")?;
    tauri::async_runtime::spawn_blocking(move || session.set_manual(height))
        .await
        .map_err(|_| "UsageDeck panel height could not be saved.".to_owned())?
}

#[tauri::command]
pub fn begin_panel_resize(app: AppHandle) -> Result<PanelResizeEdge, String> {
    let window = app
        .get_webview_window(MAIN_WINDOW)
        .ok_or("UsageDeck window is unavailable.")?;
    prepare_native_panel_resize(&window)
}

#[tauri::command]
pub fn lock_panel_resize_axis(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(MAIN_WINDOW)
        .ok_or("UsageDeck window is unavailable.")?;
    lock_native_panel_resize_axis(&window)
}

#[tauri::command]
pub fn current_panel_width(app: AppHandle) -> Result<f64, String> {
    let window = app
        .get_webview_window(MAIN_WINDOW)
        .ok_or("UsageDeck window is unavailable.")?;
    Ok(crate::window::panel_logical_width(&window))
}

#[tauri::command]
pub fn set_panel_width(app: AppHandle, width: f64) -> Result<(), String> {
    let window = app
        .get_webview_window(MAIN_WINDOW)
        .ok_or("UsageDeck window is unavailable.")?;
    crate::window::apply_panel_width(&window, width)
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        finish_native_panel_resize(&window);
    }
    app.exit(0);
}
