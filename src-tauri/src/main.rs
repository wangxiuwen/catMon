#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clean;
mod clocks;
mod config;
mod metrics;
mod system_info;
mod tray;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

use neko_sprite::draw::Rgba;
use neko_sprite::sprites::Character;
use tauri::Manager;

use metrics::Snapshot;

pub struct AppState {
    pub config: Mutex<config::Config>,
    pub chars: Mutex<Vec<Character>>,
    pub latest: Mutex<Option<Snapshot>>,
    /// 当前动画速度 0..=100（由采样线程按选定指标平滑得出）
    pub speed: AtomicU32,
    /// 主面板 / 悬浮球窗口是否可见（采样线程据此决定是否推事件；
    /// 不用 is_visible()——后台线程查窗口可见性不可靠，正是“监控没数据”的根因）
    pub main_visible: AtomicBool,
    pub ball_visible: AtomicBool,
    pub data_dir: PathBuf,
    /// 托盘墨色（随系统任务栏深浅，定期重检）
    pub ink: Mutex<Rgba>,
    /// 系统静态配置（启动时后台采集一次）
    pub sysinfo: Mutex<Option<system_info::SystemInfo>>,
}

#[tauri::command]
fn get_snapshot(state: tauri::State<AppState>) -> Option<Snapshot> {
    state.latest.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

#[tauri::command]
fn get_config(state: tauri::State<AppState>) -> config::Config {
    state.config.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// 返回 (id, label, 是否当前角色)
#[tauri::command]
fn list_characters(state: tauri::State<AppState>) -> Vec<(String, String, bool)> {
    let cur = state.config.lock().unwrap_or_else(|p| p.into_inner()).character.clone();
    state
        .chars
        .lock()
        .unwrap()
        .iter()
        .map(|c| (c.id.clone(), c.label.clone(), c.id == cur))
        .collect()
}

#[tauri::command]
fn get_system_info(state: tauri::State<AppState>) -> Option<system_info::SystemInfo> {
    state.sysinfo.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

#[tauri::command]
async fn clean_scan() -> clean::CleanScan {
    clean::scan()
}

#[tauri::command]
async fn clean_execute(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    ids: Vec<String>,
) -> Result<clean::CleanReport, String> {
    let dir = state.data_dir.join("clean-backups");
    let r = clean::execute(&ids, &dir, &|i, total, id| {
        use tauri::Emitter;
        let _ = app.emit(
            "clean-progress",
            serde_json::json!({ "i": i, "total": total, "id": id }),
        );
    });
    use tauri::Emitter;
    let _ = app.emit("clean-progress", serde_json::json!({ "done": true }));
    if r.removed_files > 0 || r.removed_reg_items > 0 || r.freed_bytes > 0 {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        {
            let mut cfg = state.config.lock().unwrap_or_else(|p| p.into_inner());
            cfg.last_clean = Some(config::LastClean {
                ts,
                freed: r.freed_bytes,
            });
            config::save(&state.data_dir, &cfg);
        }
    }
    Ok(r)
}

#[tauri::command]
async fn startup_scan() -> Vec<clean::StartupItem> {
    clean::startup_scan()
}

#[tauri::command]
fn startup_remove(state: tauri::State<AppState>, id: String) -> clean::CleanReport {
    clean::startup_remove(&id, &state.data_dir.join("clean-backups"))
}

#[tauri::command]
async fn large_files_scan() -> Vec<clean::BigFile> {
    clean::large_files_scan()
}

#[tauri::command]
async fn flush_dns() -> bool {
    clean::flush_dns()
}

/// 悬浮球点击：开/关主面板
#[tauri::command]
fn toggle_panel(app: tauri::AppHandle) {
    tray::toggle_window(&app);
}

/// 悬浮球右键：隐藏悬浮球（托盘菜单可重新打开）
#[tauri::command]
fn hide_ball(app: tauri::AppHandle) {
    tray::set_float_ball(&app, false);
}

/// WebView2 运行时检测（MS 文档的注册表口径：pv 非空且非 0.0.0.0 即已安装）
#[cfg(windows)]
fn webview2_missing() -> bool {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
    use winreg::RegKey;
    const CLIENT: &str = r"Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    let candidates = [
        (HKEY_LOCAL_MACHINE, format!(r"SOFTWARE\WOW6432Node\{CLIENT}")),
        (HKEY_LOCAL_MACHINE, format!(r"SOFTWARE\{CLIENT}")),
        (HKEY_CURRENT_USER, format!(r"Software\{CLIENT}")),
    ];
    for (hive, path) in candidates {
        if let Ok(k) = RegKey::predef(hive).open_subkey_with_flags(&path, KEY_READ) {
            if let Ok(pv) = k.get_value::<String, _>("pv") {
                if !pv.is_empty() && pv != "0.0.0.0" {
                    return false;
                }
            }
        }
    }
    true
}

#[cfg(windows)]
fn fatal_messagebox(text: &str) {
    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(hwnd: usize, text: *const u16, caption: *const u16, utype: u32) -> i32;
    }
    let wide: Vec<u16> = text
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let caption: Vec<u16> = "CatMon\0".encode_utf16().collect();
    unsafe {
        MessageBoxW(0, wide.as_ptr(), caption.as_ptr(), 0x10); // MB_ICONERROR
    }
}

/// 单实例保护：命名互斥体已存在说明有实例在跑，直接退出
#[cfg(windows)]
fn ensure_single_instance() {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    extern "system" {
        fn CreateMutexW(attrs: *mut c_void, initial_owner: i32, name: *const u16) -> *mut c_void;
        fn GetLastError() -> u32;
    }
    let name: Vec<u16> = "Local\\CatMon.SingleInstance\0"
        .encode_utf16()
        .collect();
    unsafe {
        CreateMutexW(std::ptr::null_mut(), 0, name.as_ptr());
        if GetLastError() == 183 {
            // ERROR_ALREADY_EXISTS
            fatal_messagebox("CatMon 已经在运行了（请看系统托盘）。");
            std::process::exit(0);
        }
        // 句柄故意持有到进程结束，锁住互斥体
        std::mem::forget(name);
    }
}

fn main() {
    if std::env::args().any(|a| a == "--check-webview2") {
        #[cfg(windows)]
        println!("webview2 missing = {}", webview2_missing());
        return;
    }
    if std::env::args().any(|a| a == "--selftest") {
        metrics::selftest();
        return;
    }
    #[cfg(windows)]
    if webview2_missing() {
        fatal_messagebox(
            "CatMon 需要 Microsoft Edge WebView2 运行时才能运行。\n\n\
             请到 https://developer.microsoft.com/microsoft-edge/webview2/\n\
             下载「Evergreen Standalone Installer」安装后重试。",
        );
        return;
    }
    if std::env::args().any(|a| a == "--info") {
        println!("{}", serde_json::to_string_pretty(&system_info::collect()).unwrap());
        return;
    }
    if std::env::args().any(|a| a == "--clean-scan") {
        println!("{}", serde_json::to_string_pretty(&clean::scan()).unwrap());
        return;
    }
    if let Some(pos) = std::env::args().position(|a| a == "--clean-exec") {
        let ids: Vec<String> = std::env::args()
            .skip(pos + 1)
            .flat_map(|a| a.split(',').map(str::to_string).collect::<Vec<String>>())
            .filter(|s| !s.is_empty())
            .collect();
        let dir = std::env::temp_dir().join("catmon-clean-backups");
        println!(
            "{}",
            serde_json::to_string_pretty(&clean::execute(&ids, &dir, &|_, _, _| {})).unwrap()
        );
        return;
    }
    if std::env::args().any(|a| a == "--startup-scan") {
        println!("{}", serde_json::to_string_pretty(&clean::startup_scan()).unwrap());
        return;
    }
    if std::env::args().any(|a| a == "--bigfiles") {
        println!("{}", serde_json::to_string_pretty(&clean::large_files_scan()).unwrap());
        return;
    }
    // 单实例保护在所有调试旗标之后：旗标模式（--selftest 等）允许与 GUI 并存
    #[cfg(windows)]
    ensure_single_instance();
    tauri::Builder::default()
        .setup(|app| {
            tray::setup(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            get_config,
            list_characters,
            get_system_info,
            clean_scan,
            clean_execute,
            startup_scan,
            startup_remove,
            large_files_scan,
            flush_dns,
            toggle_panel,
            hide_ball
        ])
        .build(tauri::generate_context!())
        .expect("catmon: 初始化失败")
        .run(|app, event| {
            match event {
                // 点 X 只是隐藏到托盘
                tauri::RunEvent::WindowEvent {
                    label,
                    event: tauri::WindowEvent::CloseRequested { api, .. },
                    ..
                } => {
                    if label == "main" {
                        tray::save_window_rect(app);
                        app.state::<AppState>()
                            .main_visible
                            .store(false, Ordering::Relaxed);
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.hide();
                        }
                        api.prevent_close();
                    }
                }
                // 退出/系统关机时也保存窗口位置
                tauri::RunEvent::ExitRequested { .. } => {
                    tray::save_window_rect(app);
                }
                _ => {}
            }
        });
}
