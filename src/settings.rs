//! 设置持久化(JSON):主题、换行、缩放、最近文件、会话等。
//! 存放位置:Linux `~/.config/npp-rust/settings.json`,
//! Windows `%APPDATA%\npp-rust\settings.json`,macOS `~/Library/Application Support/npp-rust/`。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub wrap: bool,
    pub dark: bool,
    pub font_size: f32,
    pub highlight: bool,
    pub show_status_bar: bool,
    /// 启动时恢复上次打开的文件
    pub restore_session: bool,
    /// 上次打开的文件列表(会话)
    pub last_tabs: Vec<String>,
    pub last_active: usize,
    /// 最近打开的文件(最多 15 条)
    pub recent: Vec<String>,
    /// 最近使用的目录(打开/保存对话框起始位置)
    pub last_dir: Option<String>,
    /// 窗口尺寸
    pub window_size: Option<[f32; 2]>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            wrap: false,
            dark: true,
            font_size: 14.0,
            highlight: true,
            show_status_bar: true,
            restore_session: false,
            last_tabs: Vec::new(),
            last_active: 0,
            recent: Vec::new(),
            last_dir: None,
            window_size: None,
        }
    }
}

impl Settings {
    pub fn push_recent(&mut self, path: &str) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_string());
        self.recent.truncate(15);
    }

    pub fn set_last_dir_from(&mut self, path: &std::path::Path) {
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.display().to_string());
        }
    }
}

pub fn config_path() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)?
    } else if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME").map(PathBuf::from)?;
        home.join("Library/Application Support")
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?
    };
    Some(base.join("npp-rust").join("settings.json"))
}

pub fn load() -> Settings {
    let Some(p) = config_path() else {
        return Settings::default();
    };
    match std::fs::read_to_string(&p) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

pub fn save(settings: &Settings) {
    let Some(p) = config_path() else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(&p, json);
    }
}
