use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use neko_sprite::draw::{rgba, Rgba};
use neko_sprite::sprites::{self, digits, Character, Kind};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Emitter, Manager, Wry};

use crate::config::{self, Config};
use crate::metrics::Sampler;
use crate::AppState;

const ICON: u32 = neko_sprite::draw::BASE;

/// 开机自启：HKCU Run 键写/清（用户级，无需管理员）
#[cfg(windows)]
fn set_autostart(enable: bool) -> std::io::Result<()> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE};
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let k = hkcu.open_subkey_with_flags(
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
        KEY_QUERY_VALUE | KEY_SET_VALUE,
    )?;
    if enable {
        let exe = std::env::current_exe()?;
        k.set_value("CatMon", &format!("\"{}\"", exe.display()))?;
    } else {
        let _ = k.delete_value("CatMon");
    }
    Ok(())
}

/// 菜单项句柄，用于勾选状态互斥更新（菜单事件在主线程回调）
pub struct MenuHandles {
    pub chars: Vec<(String, CheckMenuItem<Wry>)>,
    pub metrics: Vec<(String, CheckMenuItem<Wry>)>,
    pub pause: CheckMenuItem<Wry>,
    pub floatball: CheckMenuItem<Wry>,
    #[cfg(windows)]
    pub autostart: CheckMenuItem<Wry>,
}

/// 托盘墨色跟随系统主题：浅色任务栏用深墨，深色任务栏用浅墨
#[cfg(windows)]
fn tray_ink() -> Rgba {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let light = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize")
        .and_then(|k| k.get_value::<u32, _>("SystemUsesLightTheme"))
        .unwrap_or(0);
    if light == 1 {
        rgba(26, 26, 26, 255)
    } else {
        rgba(235, 235, 235, 255)
    }
}

#[cfg(not(windows))]
fn tray_ink() -> Rgba {
    rgba(235, 235, 235, 255)
}

/// 内置角色 + %APPDATA%/dev.catmon.app/sprites/<目录名>/xx.png 自定义角色
pub fn discover_characters(extra_dir: Option<&Path>, ink: Rgba) -> Vec<Character> {
    let mut chars = sprites::builtin(ink);
    let Some(dir) = extra_dir else { return chars };
    let Ok(rd) = std::fs::read_dir(dir) else { return chars };
    let mut dirs: Vec<_> = rd.flatten().filter(|e| e.path().is_dir()).collect();
    dirs.sort_by_key(|e| e.file_name());
    for entry in dirs {
        let id = entry.file_name().to_string_lossy().to_string();
        let mut frames = Vec::new();
        if let Ok(files) = std::fs::read_dir(entry.path()) {
            let mut names: Vec<_> = files
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().map(|x| x == "png").unwrap_or(false))
                .collect();
            names.sort();
            for f in names {
                // 防呆：超大图片直接跳过（解码会放大数倍吃内存）
                if std::fs::metadata(&f)
                    .map(|m| m.len() > 20 * 1024 * 1024)
                    .unwrap_or(true)
                {
                    continue;
                }
                let img = image::ImageReader::open(&f).ok().and_then(|r| r.decode().ok());
                if let Some(img) = img {
                    let img = img
                        .resize_exact(ICON, ICON, image::imageops::FilterType::Nearest)
                        .to_rgba8();
                    frames.push(img.into_raw());
                }
            }
        }
        if !frames.is_empty() {
            chars.push(Character {
                id: id.clone(),
                label: format!("{id}（自定义）"),
                kind: Kind::Animated,
                frames,
            });
        }
    }
    chars
}

pub fn setup(app: &mut App) -> tauri::Result<()> {
    // 纯托盘应用：macOS 不进 Dock
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);

    let handle = app.handle().clone();
    let data_dir = handle
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    let cfg = config::load(&data_dir);
    let ink = tray_ink();

    let chars = discover_characters(Some(&data_dir.join("sprites")), ink);
    let active = chars
        .iter()
        .find(|c| c.id == cfg.character)
        .cloned()
        .unwrap_or_else(|| chars[0].clone());
    let active_id = active.id.clone();
    let metric = cfg.metric.clone();
    let paused = cfg.paused;
    let autostart = cfg.autostart;
    let float_ball = cfg.float_ball;
    let win_rect = cfg.win;
    let ball_rect = cfg.ball;
    let last_clean = cfg.last_clean;

    // 自启开启状态下刷新 Run 键（exe 路径可能变化）
    #[cfg(windows)]
    if autostart {
        let _ = set_autostart(true);
    }

    app.manage(AppState {
        config: std::sync::Mutex::new(Config {
            character: active_id.clone(),
            metric: metric.clone(),
            paused,
            autostart,
            float_ball,
            win: win_rect,
            ball: ball_rect,
            last_clean,
        }),
        chars: std::sync::Mutex::new(chars),
        latest: std::sync::Mutex::new(None),
        speed: std::sync::atomic::AtomicU32::new(0),
        main_visible: std::sync::atomic::AtomicBool::new(false),
        ball_visible: std::sync::atomic::AtomicBool::new(false),
        data_dir,
        ink: std::sync::Mutex::new(ink),
        sysinfo: std::sync::Mutex::new(None),
    });

    // ---- 托盘菜单 ----
    let show = MenuItem::with_id(&handle, "show", "显示 / 隐藏监控面板", true, None::<&str>)?;
    let sep0 = PredefinedMenuItem::separator(&handle)?;
    let mut char_items = Vec::new();
    {
        let st = app.state::<AppState>();
        for c in st.chars.lock().unwrap_or_else(|p| p.into_inner()).iter() {
            char_items.push((
                c.id.clone(),
                CheckMenuItem::with_id(
                    &handle,
                    format!("char:{}", c.id),
                    format!("角色：{}", c.label),
                    true,
                    c.id == active_id,
                    None::<&str>,
                )?,
            ));
        }
    }
    let sep1 = PredefinedMenuItem::separator(&handle)?;
    let m_cpu = CheckMenuItem::with_id(&handle, "metric:cpu", "速度跟随：CPU", true, metric == "cpu", None::<&str>)?;
    let m_mem = CheckMenuItem::with_id(&handle, "metric:mem", "速度跟随：内存", true, metric == "mem", None::<&str>)?;
    let m_net = CheckMenuItem::with_id(&handle, "metric:net", "速度跟随：网络", true, metric == "net", None::<&str>)?;
    let sep2 = PredefinedMenuItem::separator(&handle)?;
    let pause = CheckMenuItem::with_id(&handle, "pause", "暂停动画", true, paused, None::<&str>)?;
    let floatball_item = CheckMenuItem::with_id(&handle, "floatball", "显示悬浮球", true, float_ball, None::<&str>)?;
    #[cfg(windows)]
    let autostart_item = CheckMenuItem::with_id(&handle, "autostart", "开机自启", true, autostart, None::<&str>)?;
    let sep3 = PredefinedMenuItem::separator(&handle)?;
    let quit = MenuItem::with_id(&handle, "quit", "退出", true, None::<&str>)?;

    let mut items: Vec<&dyn IsMenuItem<Wry>> = vec![&show, &sep0];
    for (_, it) in &char_items {
        items.push(it);
    }
    items.push(&sep1);
    items.push(&m_cpu);
    items.push(&m_mem);
    items.push(&m_net);
    items.push(&sep2);
    items.push(&pause);
    items.push(&floatball_item);
    #[cfg(windows)]
    items.push(&autostart_item);
    items.push(&sep3);
    items.push(&quit);
    let menu = Menu::with_items(&handle, &items)?;

    app.manage(MenuHandles {
        chars: char_items,
        metrics: vec![("cpu".into(), m_cpu), ("mem".into(), m_mem), ("net".into(), m_net)],
        pause,
        floatball: floatball_item,
        #[cfg(windows)]
        autostart: autostart_item,
    });

    let builder = TrayIconBuilder::with_id("main")
        .icon(Image::new_owned(active.frames[0].clone(), ICON, ICON))
        .tooltip("CatMon 启动中…")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(handle_menu)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_window(tray.app_handle());
            }
        });
    // macOS：模板图标，由系统按菜单栏深浅自动反色
    #[cfg(target_os = "macos")]
    let builder = builder.icon_as_template(true);
    let _tray = builder.build(app)?;

    eprintln!("[catmon] tray ready, character={active_id}, metric={metric}");

    // 悬浮球按配置默认关闭，开启过则随启动恢复（位置还原到现有显示器内才生效）
    if float_ball {
        if let Some(w) = app.get_webview_window("ball") {
            if let Some(r) = cfg.ball {
                let on_screen = w.available_monitors().map(|ms| {
                    ms.iter().any(|m| {
                        let (mx, my) = (m.position().x, m.position().y);
                        let (mw, mh) = (m.size().width as i32, m.size().height as i32);
                        r.x + r.w as i32 > mx && r.y + r.h as i32 > my && r.x < mx + mw && r.y < my + mh
                    })
                }).unwrap_or(false);
                if on_screen {
                    let _ = w.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
                        r.x, r.y,
                    )));
                }
            }
            let _ = w.show();
        }
        app.state::<AppState>()
            .ball_visible
            .store(true, Ordering::Relaxed);
        push_latest(&handle);
    }

    // 恢复上次的窗口尺寸与位置（不在任何现有显示器上则保持居中）
    if let (Some(w), Some(r)) = (app.get_webview_window("main"), win_rect) {
        let _ = w.set_size(tauri::Size::Physical(tauri::PhysicalSize::new(r.w, r.h)));
        let on_screen = w.available_monitors().map(|ms| {
            ms.iter().any(|m| {
                let (mx, my) = (m.position().x, m.position().y);
                let (mw, mh) = (m.size().width as i32, m.size().height as i32);
                r.x + r.w as i32 > mx && r.y + r.h as i32 > my && r.x < mx + mw && r.y < my + mh
            })
        }).unwrap_or(false);
        if on_screen {
            let _ = w.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
                r.x, r.y,
            )));
        }
    }

    let h = handle.clone();
    std::thread::spawn(move || sampler_loop(h));
    let h = handle.clone();
    std::thread::spawn(move || animator_loop(h));
    // 系统配置采集较慢（Windows 要查 CIM），后台跑，完成后推给面板
    {
        let h = handle.clone();
        std::thread::spawn(move || {
            let info = crate::system_info::collect();
            {
                let st = h.state::<AppState>();
                *st.sysinfo.lock().unwrap_or_else(|p| p.into_inner()) = Some(info.clone());
            }
            let _ = h.emit("sysinfo", &info);
            eprintln!("[catmon] system info collected ({} groups)", info.groups.len());
        });
    }
    Ok(())
}

fn handle_menu(app: &AppHandle, event: tauri::menu::MenuEvent) {
    let id: &str = event.id().as_ref();
    let st = app.state::<AppState>();
    match id {
        "show" => toggle_window(app),
        "quit" => app.exit(0),
        "pause" => {
            let mut cfg = st.config.lock().unwrap_or_else(|p| p.into_inner());
            cfg.paused = !cfg.paused;
            config::save(&st.data_dir, &cfg);
            app.state::<MenuHandles>().pause.set_checked(cfg.paused).ok();
            let _ = app.emit("config", &*cfg);
        }
        #[cfg(windows)]
        "autostart" => {
            let mut cfg = st.config.lock().unwrap_or_else(|p| p.into_inner());
            cfg.autostart = !cfg.autostart;
            if set_autostart(cfg.autostart).is_err() {
                cfg.autostart = !cfg.autostart; // 写失败回滚
                eprintln!("[catmon] 写入开机自启失败，已回滚");
            }
            config::save(&st.data_dir, &cfg);
            app.state::<MenuHandles>().autostart.set_checked(cfg.autostart).ok();
            let _ = app.emit("config", &*cfg);
        }
        "floatball" => {
            let on = {
                let mut cfg = st.config.lock().unwrap_or_else(|p| p.into_inner());
                cfg.float_ball = !cfg.float_ball;
                cfg.float_ball
            };
            set_float_ball(app, on);
        }
        s if s.starts_with("char:") => {
            let cid = &s["char:".len()..];
            {
                let chars = st.chars.lock().unwrap_or_else(|p| p.into_inner());
                if !chars.iter().any(|c| c.id == cid) {
                    return;
                }
            }
            {
                let mut cfg = st.config.lock().unwrap_or_else(|p| p.into_inner());
                cfg.character = cid.to_string();
                config::save(&st.data_dir, &cfg);
                let _ = app.emit("config", &*cfg);
            }
            for (hid, item) in &app.state::<MenuHandles>().chars {
                item.set_checked(hid == cid).ok();
            }
        }
        s if s.starts_with("metric:") => {
            let m = &s["metric:".len()..];
            {
                let mut cfg = st.config.lock().unwrap_or_else(|p| p.into_inner());
                cfg.metric = m.to_string();
                config::save(&st.data_dir, &cfg);
                let _ = app.emit("config", &*cfg);
            }
            for (hid, item) in &app.state::<MenuHandles>().metrics {
                item.set_checked(hid == m).ok();
            }
        }
        _ => {}
    }
}

/// 关闭/隐藏前记录主面板的位置与尺寸
pub fn save_window_rect(app: &AppHandle) {
    let Some(w) = app.get_webview_window("main") else { return };
    let (Ok(pos), Ok(size)) = (w.outer_position(), w.outer_size()) else {
        return;
    };
    let st = app.state::<AppState>();
    let mut cfg = st.config.lock().unwrap_or_else(|p| p.into_inner());
    cfg.win = Some(config::WinRect {
        x: pos.x,
        y: pos.y,
        w: size.width,
        h: size.height,
    });
    config::save(&st.data_dir, &cfg);
}

/// 立即把最新快照推给前端（打开面板/悬浮球时调用，免等下一个采样周期）
pub fn push_latest(app: &AppHandle) {
    if let Some(snap) = app.state::<AppState>().latest.lock().unwrap_or_else(|p| p.into_inner()).clone() {
        let _ = app.emit("metrics", snap);
    }
}

/// 悬浮球显示/隐藏（同步配置、托盘勾选与可见性标志）
pub fn set_float_ball(app: &AppHandle, on: bool) {
    let st = app.state::<AppState>();
    {
        let mut cfg = st.config.lock().unwrap_or_else(|p| p.into_inner());
        cfg.float_ball = on;
        config::save(&st.data_dir, &cfg);
    }
    if let Some(w) = app.get_webview_window("ball") {
        if on {
            // 恢复上次位置（不在任何现有显示器上则保持居中）
            let r = st.config.lock().unwrap_or_else(|p| p.into_inner()).ball;
            if let Some(r) = r {
                let on_screen = w.available_monitors().map(|ms| {
                    ms.iter().any(|m| {
                        let (mx, my) = (m.position().x, m.position().y);
                        let (mw, mh) = (m.size().width as i32, m.size().height as i32);
                        r.x + r.w as i32 > mx && r.y + r.h as i32 > my && r.x < mx + mw && r.y < my + mh
                    })
                }).unwrap_or(false);
                if on_screen {
                    let _ = w.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
                        r.x, r.y,
                    )));
                }
            }
            let _ = w.show();
        } else {
            save_ball_rect(app);
            let _ = w.hide();
        }
    }
    st.ball_visible.store(on, Ordering::Relaxed);
    if on {
        push_latest(app);
    }
    app.state::<MenuHandles>().floatball.set_checked(on).ok();
    let cfg = st.config.lock().unwrap_or_else(|p| p.into_inner()).clone();
    let _ = app.emit("config", &cfg);
}

/// 记录悬浮球当前位置
pub fn save_ball_rect(app: &AppHandle) {
    let Some(w) = app.get_webview_window("ball") else { return };
    let (Ok(pos), Ok(size)) = (w.outer_position(), w.outer_size()) else {
        return;
    };
    let st = app.state::<AppState>();
    let mut cfg = st.config.lock().unwrap_or_else(|p| p.into_inner());
    cfg.ball = Some(config::WinRect {
        x: pos.x,
        y: pos.y,
        w: size.width,
        h: size.height,
    });
    config::save(&st.data_dir, &cfg);
}

pub fn toggle_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let visible = w.is_visible().unwrap_or(false);
        let focused = w.is_focused().unwrap_or(false);
        if visible && focused {
            save_window_rect(app);
            let _ = w.hide();
            app.state::<AppState>()
                .main_visible
                .store(false, Ordering::Relaxed);
        } else {
            let _ = w.show();
            let _ = w.unminimize();
            let _ = w.set_focus();
            app.state::<AppState>()
                .main_visible
                .store(true, Ordering::Relaxed);
            push_latest(app);
        }
    }
}

fn pct(a: u64, b: u64) -> f32 {
    if b == 0 {
        0.0
    } else {
        a as f32 / b as f32 * 100.0
    }
}

fn fmt_rate(bps: f64) -> String {
    let b = bps.max(0.0);
    if b >= 1048576.0 {
        format!("{:.1}MB/s", b / 1048576.0)
    } else if b >= 1024.0 {
        format!("{:.1}KB/s", b / 1024.0)
    } else {
        format!("{:.0}B/s", b)
    }
}

/// 网络速率 → 0..100：1KB/s 起、1GB/s 满（对数）
fn net_pct(bps: f64) -> f32 {
    if bps <= 1024.0 {
        return 0.0;
    }
    (((bps.log10() - 3.0) / 6.0) * 100.0).clamp(0.0, 100.0) as f32
}

/// 每秒采样：更新快照、推事件给面板、算动画速度、刷托盘提示
fn sampler_loop(app: AppHandle) {
    let mut sampler = Sampler::new();
    let mut ema = 0f32;
    let mut n = 0u64;
    loop {
        let snap = sampler.tick();
        {
            let st = app.state::<AppState>();
            let cfg = st.config.lock().unwrap_or_else(|p| p.into_inner()).clone();
            let raw = match cfg.metric.as_str() {
                "mem" => pct(snap.mem_used, snap.mem_total),
                "net" => net_pct(snap.rx + snap.tx),
                _ => snap.cpu.clamp(0.0, 100.0),
            };
            ema = ema * 0.75 + raw * 0.25;
            st.speed
                .store(ema.round().clamp(0.0, 100.0) as u32, Ordering::Relaxed);
            *st.latest.lock().unwrap_or_else(|p| p.into_inner()) = Some(snap.clone());
        }
        // 面板/悬浮球至少一个可见才推事件；重新可见时 toggle 处会立即补发最新一帧
        let ui_visible = {
            let st = app.state::<AppState>();
            st.main_visible.load(Ordering::Relaxed) || st.ball_visible.load(Ordering::Relaxed)
        };
        if ui_visible {
            let _ = app.emit("metrics", snap.clone());
        }
        n += 1;

        // 每 5 分钟重检任务栏深浅，墨色变了就重建角色帧
        if n.is_multiple_of(300) {
            let st = app.state::<AppState>();
            let new_ink = tray_ink();
            let changed = *st.ink.lock().unwrap_or_else(|p| p.into_inner()) != new_ink;
            if changed {
                let dir = st.data_dir.clone();
                *st.chars.lock().unwrap_or_else(|p| p.into_inner()) = discover_characters(Some(&dir.join("sprites")), new_ink);
                *st.ink.lock().unwrap_or_else(|p| p.into_inner()) = new_ink;
                eprintln!("[catmon] 主题变化，托盘墨色已重建");
            }
        }

        // 每 30 分钟重采系统配置（驱动/磁盘变化能跟上），后台线程不卡交互
        if n.is_multiple_of(1800) {
            let h = app.clone();
            std::thread::spawn(move || {
                let info = crate::system_info::collect();
                {
                    let st = h.state::<AppState>();
                    *st.sysinfo.lock().unwrap_or_else(|p| p.into_inner()) = Some(info.clone());
                }
                let _ = h.emit("sysinfo", &info);
                eprintln!("[catmon] system info refreshed ({} groups)", info.groups.len());
            });
        }

        if n.is_multiple_of(5) {
            let label = {
                let st = app.state::<AppState>();
                let cfg = st.config.lock().unwrap_or_else(|p| p.into_inner());
                let chars = st.chars.lock().unwrap_or_else(|p| p.into_inner());
                chars
                    .iter()
                    .find(|c| c.id == cfg.character)
                    .map(|c| c.label.clone())
                    .unwrap_or_default()
            };
            let role = if label.is_empty() {
                String::new()
            } else {
                format!("{label} · ")
            };
            let tt = format!(
                "CatMon · {role}CPU {:.0}% · 内存 {:.0}% · ↓{} ↑{}",
                snap.cpu,
                pct(snap.mem_used, snap.mem_total),
                fmt_rate(snap.rx),
                fmt_rate(snap.tx)
            );
            let h = app.clone();
            let _ = app.run_on_main_thread(move || {
                if let Some(t) = h.tray_by_id("main") {
                    let _ = t.set_tooltip(Some(tt));
                }
            });
        }

        // 数字角色没有动画帧，随每次采样重绘；显示值 = 当前选定的跟随指标
        let (is_static, metric) = {
            let st = app.state::<AppState>();
            let cfg = st.config.lock().unwrap_or_else(|p| p.into_inner());
            let chars = st.chars.lock().unwrap_or_else(|p| p.into_inner());
            let s = chars
                .iter()
                .find(|c| c.id == cfg.character)
                .map(|c| c.kind == Kind::Static)
                .unwrap_or(false);
            (s, cfg.metric.clone())
        };
        let ink = *app.state::<AppState>().ink.lock().unwrap_or_else(|p| p.into_inner());
        if is_static {
            let val = match metric.as_str() {
                "mem" => pct(snap.mem_used, snap.mem_total),
                "net" => net_pct(snap.rx + snap.tx),
                _ => snap.cpu.clamp(0.0, 100.0),
            };
            let img = Image::new_owned(digits(val, ink), ICON, ICON);
            let h = app.clone();
            let _ = app.run_on_main_thread(move || {
                if let Some(t) = h.tray_by_id("main") {
                    let _ = t.set_icon(Some(img));
                }
            });
        }
        std::thread::sleep(Duration::from_millis(1000));
    }
}

/// 动画线程：40ms 心跳，按 speed 把帧间隔从 900ms（闲）压到 ~50ms（满载）
fn animator_loop(app: AppHandle) {
    let mut last_switch = Instant::now();
    let mut frame = 0usize;
    loop {
        std::thread::sleep(Duration::from_millis(40));
        // 心跳锁内只读元数据，不克隆帧数据（约 25 次/秒，克隆是纯浪费）
        let (paused, kind, frames_len, char_id) = {
            let st = app.state::<AppState>();
            let cfg = st.config.lock().unwrap_or_else(|p| p.into_inner());
            let chars = st.chars.lock().unwrap_or_else(|p| p.into_inner());
            match chars.iter().find(|c| c.id == cfg.character) {
                Some(c) => (cfg.paused, c.kind, c.frames.len(), c.id.clone()),
                None => continue,
            }
        };
        if paused || kind == Kind::Static || frames_len == 0 {
            continue;
        }
        let speed = app.state::<AppState>().speed.load(Ordering::Relaxed) as f32;
        let interval = 900.0 * 0.055f32.powf(speed / 100.0);
        if last_switch.elapsed().as_secs_f64() * 1000.0 >= interval as f64 {
            last_switch = Instant::now();
            frame = (frame + 1) % frames_len;
            // 真正切帧才克隆这 4KB；len 现取现模（主题重建可能换了帧数）
            let bytes = {
                let st = app.state::<AppState>();
                let chars = st.chars.lock().unwrap_or_else(|p| p.into_inner());
                chars.iter().find(|c| c.id == char_id).and_then(|c| {
                    c.frames.get(frame % c.frames.len()).cloned()
                })
            };
            let Some(bytes) = bytes else { continue };
            let img = Image::new_owned(bytes, ICON, ICON);
            let h = app.clone();
            let _ = app.run_on_main_thread(move || {
                if let Some(t) = h.tray_by_id("main") {
                    let _ = t.set_icon(Some(img));
                }
            });
        }
    }
}
