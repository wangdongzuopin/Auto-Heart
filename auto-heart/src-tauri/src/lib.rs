mod commands;
mod conversation;
mod database;
mod heartbeat;
mod model_router;
mod notifier;
mod settings;

use database::init_database;
use heartbeat::{
    new_file_watcher_generation, start_deep_heartbeat, start_file_watcher,
    start_middle_heartbeat, start_operation_log_heartbeat, start_shallow_heartbeat,
};
use settings::{load_settings, SettingsHandle, auto_detect_watch_paths};

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use chrono::Local;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder,
};

#[tauri::command]
async fn open_main_window(app: AppHandle) -> Result<(), String> {
    eprintln!("[open_main_window] called");
    if let Some(window) = app.get_webview_window("main") {
        eprintln!("[open_main_window] main window already exists, focusing...");
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    eprintln!("[open_main_window] building window...");
    WebviewWindowBuilder::new(
        &app,
        "main",
        WebviewUrl::App("index.html?view=main".into()),
    )
    .title("Auto-Heart")
    .inner_size(480.0, 680.0)
    .center()
    .decorations(true)
    .resizable(true)
    .build()
    .map_err(|e| e.to_string())?;

    eprintln!("[open_main_window] window created successfully");
    Ok(())
}

#[tauri::command]
async fn open_main_window_with_view(app: AppHandle, view: String) -> Result<(), String> {
    eprintln!("[open_main_window_with_view] called with view: {}", view);

    // 如果窗口已存在，只发送导航事件
    if let Some(window) = app.get_webview_window("main") {
        eprintln!("[open_main_window_with_view] main window exists, sending navigate event");
        window.emit("navigate_to", view).map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    // 否则创建新窗口
    eprintln!("[open_main_window_with_view] building new window with view: {}", view);
    WebviewWindowBuilder::new(
        &app,
        "main",
        WebviewUrl::App(format!("index.html?view={}", view).into()),
    )
    .title("Auto-Heart")
    .inner_size(480.0, 680.0)
    .center()
    .decorations(true)
    .resizable(true)
    .build()
    .map_err(|e| e.to_string())?;

    eprintln!("[open_main_window_with_view] window created successfully");
    Ok(())
}

/// 由 Rust 直接关主窗口，避免子 WebView 里 `plugin:window|close` 权限/IPC 异常导致关不掉
#[tauri::command]
fn close_main_window(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("main") {
        w.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show_item = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出 Auto-Heart", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

    TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("Auto-Heart · 活跃中")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                let _ = open_main_window(app.clone());
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = open_main_window(tray.app_handle().clone());
            }
        })
        .build(app)?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 第二个实例启动时，聚焦已有的 Orb 窗口
            if let Some(win) = app.get_webview_window("orb") {
                let _ = win.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // ── 系统托盘 ──
            setup_tray(app.handle())?;

            // ── 数据目录 ──
            // settings.json 始终存在默认目录（锚点）
            let app_data_dir: PathBuf = app.path().app_data_dir().expect("无法获取 app_data_dir");
            std::fs::create_dir_all(&app_data_dir).ok();

            // 读取 data_dir 设置，决定实际数据存放位置
            let app_settings = load_settings(&app_data_dir);
            let today_str = Local::now().format("%Y-%m-%d").to_string();
            let today_dir: PathBuf = if app_settings.data_dir.is_empty() {
                app_data_dir.clone()
            } else {
                PathBuf::from(&app_settings.data_dir).join(&today_str)
            };
            std::fs::create_dir_all(&today_dir).ok();

            // ── 数据库 ──
            let db = init_database(today_dir).expect("数据库初始化失败");
            app.manage(db.clone());

            // ── 设置 ──
            let watch_paths: Vec<PathBuf> = if app_settings.watch_paths.is_empty() {
                auto_detect_watch_paths()
                    .iter()
                    .map(PathBuf::from)
                    .collect()
            } else {
                app_settings.watch_paths.iter().map(PathBuf::from).collect()
            };
            let settings_handle: SettingsHandle = Arc::new(Mutex::new(app_settings.clone()));
            app.manage(settings_handle.clone());
            let file_watcher_generation = new_file_watcher_generation();
            app.manage(file_watcher_generation.clone());

            // ── 启动三层心跳 ──
            start_shallow_heartbeat(app.handle().clone(), db.clone(), settings_handle.clone());
            start_middle_heartbeat(app.handle().clone(), db.clone(), settings_handle.clone());
            start_deep_heartbeat(app.handle().clone(), db.clone(), settings_handle.clone());
            start_operation_log_heartbeat(app.handle().clone(), db.clone(), settings_handle.clone());

            // ── 文件监听（有配置时启动）──
            let all_paths = watch_paths;
            if !all_paths.is_empty() {
                start_file_watcher(db.clone(), all_paths, file_watcher_generation, app.handle().clone());
            }

            // ── Orb 窗口初始定位：屏幕右下角 ──
            if let Some(orb_win) = app.get_webview_window("orb") {
                if let Ok(Some(monitor)) = orb_win.primary_monitor() {
                    let mon_size = monitor.size();
                    let scale = monitor.scale_factor();
                    let orb_physical = (160.0 * scale) as i32;
                    let margin_x = (20.0 * scale) as i32;
                    let margin_y = (60.0 * scale) as i32; // 留出任务栏空间
                    let x = mon_size.width as i32 - orb_physical - margin_x;
                    let y = mon_size.height as i32 - orb_physical - margin_y;
                    let _ = orb_win.set_position(tauri::PhysicalPosition::new(x, y));
                }
            }

            eprintln!(
                "[Auto-Heart] 初始化完成 · 三层心跳已启动 · 数据库就绪 · 数据目录: {:?}",
                app_data_dir
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_main_window,
            open_main_window_with_view,
            close_main_window,
            // 消息队列
            commands::get_message_queue,
            commands::dismiss_message,
            commands::ack_message,
            // 语义地图
            commands::get_semantic_modules,
            commands::get_decision_log,
            commands::get_tech_debt,
            // 今日任务 + 意图
            commands::get_today_tasks,
            commands::get_today_intent,
            commands::get_today_activity_summary,
            commands::clear_today_activity_snapshots,
            commands::get_today_file_changes,
            commands::clear_today_file_changes,
            commands::get_tracking_health,
            commands::add_intent,
            commands::add_today_task,
            commands::update_today_task_status,
            commands::update_today_task,
            commands::delete_today_task,
            commands::move_today_task,
            commands::parse_today_intent_now,
            // 设置
            commands::load_settings_cmd,
            commands::save_settings,
            commands::save_settings_to_home,
            // 日报
            commands::get_today_report,
            commands::get_daily_report,
            commands::update_report_content,
            commands::send_daily_report,
            // 对话 + 日志查询
            commands::query_operation_logs,
            commands::search_file_changes,
            commands::get_trend_stats,
            // 窗口状态
            commands::save_window_state,
            commands::load_window_state,
            // 对话
            commands::get_conversations,
            commands::get_conversation,
            commands::create_conversation,
            commands::delete_conversation,
            commands::send_message,
        ])
        .run(tauri::generate_context!())
        .expect("Auto-Heart 运行失败");
}
