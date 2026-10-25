use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// 当前角色 id（内置 cat/ghost/rocket/digits，或 sprites/ 下的自定义目录名）
    pub character: String,
    /// 动画速度跟随的指标：cpu | mem | net
    pub metric: String,
    pub paused: bool,
    /// 首次启动自动展示一次监控面板（消费后置 false）
    pub first_run: bool,
    /// 开机自启（Windows: HKCU Run 键）
    pub autostart: bool,
    /// 桌面悬浮球（默认关闭）
    pub float_ball: bool,
    /// 主面板上次的位置与尺寸（物理像素）
    pub win: Option<WinRect>,
    /// 悬浮球上次位置（物理像素）
    pub ball: Option<WinRect>,
    /// 上次清理时间与释放量
    pub last_clean: Option<LastClean>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct LastClean {
    /// Unix 秒
    pub ts: u64,
    pub freed: u64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct WinRect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            character: "cat".into(),
            metric: "cpu".into(),
            paused: false,
            first_run: true,
            autostart: false,
            float_ball: false,
            win: None,
            ball: None,
            last_clean: None,
        }
    }
}

fn config_path(dir: &Path) -> PathBuf {
    dir.join("config.json")
}

pub fn load(dir: &Path) -> Config {
    fs::read(config_path(dir))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save(dir: &Path, cfg: &Config) {
    let _ = fs::create_dir_all(dir);
    let Ok(b) = serde_json::to_vec_pretty(cfg) else { return };
    // 原子写：先落临时文件再替换，防写到一半损坏 config
    let tmp = dir.join("config.json.tmp");
    if fs::write(&tmp, &b).is_ok() {
        if fs::rename(&tmp, config_path(dir)).is_ok() {
            return;
        }
        let _ = fs::remove_file(&tmp);
    }
    let _ = fs::write(config_path(dir), b);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load_roundtrip_and_atomic() {
        let dir = std::env::temp_dir().join("catmon-config-test");
        let _ = fs::remove_dir_all(&dir);
        let cfg = Config {
            character: "rocket".into(),
            metric: "net".into(),
            paused: true,
            first_run: false,
            autostart: true,
            float_ball: true,
            win: Some(WinRect { x: 1, y: 2, w: 3, h: 4 }),
            ball: Some(WinRect { x: 5, y: 6, w: 84, h: 84 }),
            last_clean: None,
        };
        save(&dir, &cfg);
        let loaded = load(&dir);
        assert_eq!(loaded.character, "rocket");
        assert_eq!(loaded.metric, "net");
        assert!(loaded.win.unwrap().x == 1);
        // 覆盖写不残留临时文件
        save(&dir, &Config::default());
        assert_eq!(load(&dir).character, "cat");
        assert!(!dir.join("config.json.tmp").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
