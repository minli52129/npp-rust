//! NppRust —— 用 Rust 编写的类 Notepad++ 多标签文本/代码编辑器。
//!
//! 功能:多标签、语法高亮、行号、查找/替换(正则)、转到行、行操作、
//! 大小写转换、编码转换、换行符转换、自动缩进、缩放、明暗主题、
//! 最近文件、会话恢复、外部文件修改监视、拖放打开等。

mod editor;
mod files;
mod highlight;
mod search;
mod settings;
mod tab;
mod textops;

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Instant;

use eframe::egui;
use egui::{
    Align, Align2, Button, Color32, FontId, Id, Key, RichText, ScrollArea, TextEdit, TextStyle, Ui,
    Vec2, ViewportCommand,
};
use egui::epaint::text::{FontData, FontInsert, FontPriority, InsertFontFamily};

use editor::{EditorOpts, EditorUi};
use files::{Eol, FileEncoding};
use highlight::Lang;
use search::{FindParams, Matcher};
use settings::Settings;
use tab::Tab;
use textops::{CaseOp, LineOp};

/// 查找条件缓存键:(内容, 区分大小写, 全字匹配, 正则)
type MatchKey = (String, bool, bool, bool);
type MatcherCache = Option<(MatchKey, Result<Matcher, String>)>;

const APP_NAME: &str = "NppRust";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> eframe::Result {
    let settings = settings::load();
    let cli_files: Vec<PathBuf> = std::env::args()
        .skip(1)
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .collect();

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1150.0, 760.0])
        .with_min_inner_size([480.0, 300.0])
        .with_icon(load_icon());
    if let Some([w, h]) = settings.window_size {
        viewport = viewport.with_inner_size([w.max(480.0), h.max(300.0)]);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        &format!("{APP_NAME} - Rust 记事本"),
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_theme(if settings.dark {
                egui::ThemePreference::Dark
            } else {
                egui::ThemePreference::Light
            });
            install_fonts(&cc.egui_ctx);
            Ok(Box::new(NotepadApp::new(settings, cli_files)))
        }),
    )
}

// ---------------------------------------------------------------------------
// 图标:手绘一个蓝底白 "N" 的 32x32 图标
// ---------------------------------------------------------------------------

fn load_icon() -> egui::IconData {
    const W: usize = 32;
    const H: usize = 32;
    let mut rgba = vec![0u8; W * H * 4];
    for i in 0..W * H {
        rgba[i * 4] = 0x2E;
        rgba[i * 4 + 1] = 0x64;
        rgba[i * 4 + 2] = 0xC8;
        rgba[i * 4 + 3] = 255;
    }
    const N: [[u8; 5]; 7] = [
        [1, 0, 0, 0, 1],
        [1, 1, 0, 0, 1],
        [1, 1, 0, 0, 1],
        [1, 0, 1, 0, 1],
        [1, 0, 1, 1, 1],
        [1, 0, 0, 1, 1],
        [1, 0, 0, 0, 1],
    ];
    let scale = 3usize;
    let ox = (W - 5 * scale) / 2;
    let oy = (H - 7 * scale) / 2;
    for (ry, row) in N.iter().enumerate() {
        for (rx, &v) in row.iter().enumerate() {
            if v == 1 {
                for dy in 0..scale {
                    for dx in 0..scale {
                        let x = ox + rx * scale + dx;
                        let y = oy + ry * scale + dy;
                        let i = (y * W + x) * 4;
                        rgba[i] = 255;
                        rgba[i + 1] = 255;
                        rgba[i + 2] = 255;
                        rgba[i + 3] = 255;
                    }
                }
            }
        }
    }
    egui::IconData {
        width: W as u32,
        height: H as u32,
        rgba,
    }
}

// ---------------------------------------------------------------------------
// 中文字体:尝试加载系统字体作为后备,保证中文界面与内容可显示
// ---------------------------------------------------------------------------

fn install_fonts(ctx: &egui::Context) {
    for path in cjk_font_candidates() {
        if let Ok(bytes) = std::fs::read(&path) {
            ctx.add_font(FontInsert {
                name: "cjk-fallback".into(),
                data: FontData::from_owned(bytes),
                families: vec![
                    InsertFontFamily {
                        family: egui::FontFamily::Proportional,
                        priority: FontPriority::Lowest,
                    },
                    InsertFontFamily {
                        family: egui::FontFamily::Monospace,
                        priority: FontPriority::Lowest,
                    },
                ],
            });
            return;
        }
    }
}

fn cjk_font_candidates() -> Vec<PathBuf> {
    if cfg!(target_os = "windows") {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        let f = |name: &str| PathBuf::from(&windir).join("Fonts").join(name);
        return vec![
            f("msyh.ttc"),
            f("msyh.ttf"),
            f("simsun.ttc"),
            f("simhei.ttf"),
            f("Deng.ttf"),
        ];
    }
    if cfg!(target_os = "macos") {
        let sys = PathBuf::from("/System/Library/Fonts");
        return vec![
            sys.join("PingFang.ttc"),
            sys.join("STHeiti Light.ttc"),
            sys.join("Hiragino Sans GB.ttc"),
        ];
    }
    let mut v = vec![
        PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
        PathBuf::from("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc"),
        PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJKsc-Regular.otf"),
        PathBuf::from("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc"),
        PathBuf::from("/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc"),
        PathBuf::from("/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf"),
        PathBuf::from("/usr/share/fonts/wqy-microhei/wqy-microhei.ttc"),
    ];
    // 兜底:浅层扫描 /usr/share/fonts 找常见中文字体
    if let Ok(entries) = std::fs::read_dir("/usr/share/fonts") {
        for e in entries.flatten() {
            if let Ok(sub) = std::fs::read_dir(e.path()) {
                for s in sub.flatten() {
                    let name = s.file_name().to_string_lossy().to_lowercase();
                    if (name.contains("cjk")
                        || name.contains("wqy")
                        || name.contains("droidsansfallback")
                        || name.contains("pingfang")
                        || name.contains("sourcehansans")
                        || name.contains("simsun")
                        || name.contains("msyh"))
                        && (name.ends_with(".ttf") || name.ends_with(".ttc") || name.ends_with(".otf"))
                    {
                        v.push(s.path());
                    }
                }
            }
        }
    }
    v
}

// ---------------------------------------------------------------------------
// 数据结构
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum PendingClose {
    None,
    Exit,
    CloseAll,
    CloseTab(u64),
}

struct Toast {
    msg: String,
    born: Instant,
    ttl: f32,
}

struct SearchHit {
    tab_uid: u64,
    title: String,
    line: usize,
    col: usize,
    preview: String,
    char_range: (usize, usize),
}

struct NotepadApp {
    tabs: Vec<Tab>,
    active: usize,
    next_uid: u64,
    new_count: usize,

    // 查找/替换
    find: FindParams,
    replace_text: String,
    find_win: bool,
    replace_row: bool,
    find_focus_pending: bool,
    find_error: Option<String>,
    matcher_cache: MatcherCache,
    /// 当前文档的匹配区间缓存(字节偏移)
    match_ranges: Vec<Range<usize>>,
    match_text: String,
    match_key: Option<MatchKey>,
    current_match: Option<Range<usize>>,

    // 结果面板
    results: Vec<SearchHit>,
    results_truncated: bool,
    results_open: bool,

    // 转到行
    goto_win: bool,
    goto_text: String,
    goto_focus_pending: bool,

    // 其他窗口
    about_open: bool,
    help_open: bool,

    // 关闭确认
    pending_close: PendingClose,
    allow_close: bool,

    // 设置
    settings: Settings,

    // 编辑器缓存
    editor_ui: EditorUi,

    // 内部状态
    last_title: String,
    was_focused: Option<bool>,
    toasts: Vec<Toast>,
    last_settings_save: Option<Instant>,
}

impl NotepadApp {
    fn new(settings: Settings, cli_files: Vec<PathBuf>) -> Self {
        let mut app = NotepadApp {
            tabs: Vec::new(),
            active: 0,
            next_uid: 1,
            new_count: 0,
            find: FindParams::default(),
            replace_text: String::new(),
            find_win: false,
            replace_row: false,
            find_focus_pending: false,
            find_error: None,
            matcher_cache: None,
            match_ranges: Vec::new(),
            match_text: String::new(),
            match_key: None,
            current_match: None,
            results: Vec::new(),
            results_truncated: false,
            results_open: false,
            goto_win: false,
            goto_text: String::new(),
            goto_focus_pending: false,
            about_open: false,
            help_open: false,
            pending_close: PendingClose::None,
            allow_close: false,
            settings: settings.clone(),
            editor_ui: EditorUi::default(),
            last_title: String::new(),
            was_focused: None,
            toasts: Vec::new(),
            last_settings_save: None,
        };
        // 恢复会话
        if settings.restore_session && cli_files.is_empty() {
            let paths: Vec<PathBuf> = settings
                .last_tabs
                .iter()
                .map(PathBuf::from)
                .filter(|p| p.exists())
                .collect();
            for p in paths {
                if let Ok(t) = Tab::from_path(&p, app.next_uid) {
                    app.next_uid += 1;
                    app.tabs.push(t);
                }
            }
            app.active = settings.last_active.min(app.tabs.len().saturating_sub(1));
            app.new_count = app.tabs.len();
        }
        // 命令行指定的文件
        for p in cli_files {
            let key = canon(&p);
            if app
                .tabs
                .iter()
                .any(|t| t.path.as_ref().map(|tp| canon(tp)) == Some(key.clone()))
            {
                continue;
            }
            if let Ok(t) = Tab::from_path(&p, app.next_uid) {
                app.next_uid += 1;
                app.tabs.push(t);
                app.settings.push_recent(&p.display().to_string());
            }
        }
        if !app.tabs.is_empty() {
            app.active = 0;
            app.new_count = app.tabs.len();
        }
        app
    }

    fn toast(&mut self, msg: impl Into<String>) {
        self.toasts.push(Toast {
            msg: msg.into(),
            born: Instant::now(),
            ttl: 3.5,
        });
    }

    fn save_settings(&self) {
        settings::save(&self.settings);
    }

    fn sync_session(&mut self) {
        self.settings.last_tabs = self
            .tabs
            .iter()
            .filter_map(|t| t.path.as_ref().map(|p| p.display().to_string()))
            .collect();
        self.settings.last_active = self.active;
    }

    fn active_idx(&self) -> Option<usize> {
        if self.tabs.is_empty() {
            None
        } else {
            Some(self.active.min(self.tabs.len() - 1))
        }
    }

    fn fix_active(&mut self) {
        if self.tabs.is_empty() {
            self.active = 0;
        } else if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        }
    }

    // -----------------------------------------------------------------------
    // 文件操作
    // -----------------------------------------------------------------------

    fn new_tab(&mut self) {
        self.new_count += 1;
        self.tabs.push(Tab::new_untitled(self.new_count, self.next_uid));
        self.next_uid += 1;
        self.active = self.tabs.len() - 1;
        self.sync_session();
        self.save_settings();
    }

    fn open_dialog(&mut self) {
        let mut dlg = rfd::FileDialog::new()
            .set_title("打开文件")
            .add_filter(
                "常用文本/代码",
                &[
                    "txt", "md", "log", "csv", "json", "rs", "py", "js", "ts", "c", "h", "cpp",
                    "hpp", "java", "cs", "go", "xml", "html", "css", "yaml", "yml", "toml", "ini",
                    "sh", "bat", "sql", "php", "rb", "kt", "swift",
                ],
            )
            .add_filter("所有文件", &[] as &[&str]);
        if let Some(d) = &self.settings.last_dir {
            dlg = dlg.set_directory(d);
        }
        if let Some(paths) = dlg.pick_files() {
            for p in paths {
                self.open_path(p);
            }
        }
    }

    fn open_path(&mut self, p: PathBuf) {
        let key = canon(&p);
        if let Some(i) = self
            .tabs
            .iter()
            .position(|t| t.path.as_ref().map(|p| canon(p)).as_deref() == Some(key.as_path()))
        {
            self.active = i;
            self.toast(format!("已在标签页中打开:{}", t_title(&self.tabs[i])));
            return;
        }
        match Tab::from_path(&p, self.next_uid) {
            Ok(t) => {
                self.next_uid += 1;
                let title = t.title.clone();
                self.tabs.push(t);
                self.active = self.tabs.len() - 1;
                self.settings.push_recent(&p.display().to_string());
                self.settings.set_last_dir_from(&p);
                self.sync_session();
                self.save_settings();
                let _ = title;
            }
            Err(e) => self.toast(e),
        }
    }

    /// 保存指定标签;已保存的直接写盘,未保存的弹「另存为」。
    /// 返回是否成功保存(用户取消另存为返回 false)。
    fn save_tab_index(&mut self, idx: usize) -> bool {
        let Some(tab) = self.tabs.get(idx) else { return false };
        if let Some(path) = tab.path.clone() {
            self.do_save_to(idx, path);
            true
        } else {
            self.save_as_dialog_index(idx)
        }
    }

    fn save_as_dialog_index(&mut self, idx: usize) -> bool {
        let Some(tab) = self.tabs.get(idx) else { return false };
        let mut dlg = rfd::FileDialog::new()
            .set_title("另存为")
            .set_file_name(&tab.title)
            .add_filter(
                "常用文本/代码",
                &[
                    "txt", "md", "log", "csv", "json", "rs", "py", "js", "ts", "c", "cpp", "java",
                    "go", "xml", "html", "yaml", "toml", "ini", "sh", "sql",
                ],
            );
        if let Some(p) = &tab.path {
            if let Some(d) = p.parent() {
                dlg = dlg.set_directory(d);
            }
        } else if let Some(d) = &self.settings.last_dir {
            dlg = dlg.set_directory(d);
        }
        match dlg.save_file() {
            Some(p) => {
                self.do_save_to(idx, p);
                true
            }
            None => false,
        }
    }

    fn do_save_to(&mut self, idx: usize, path: PathBuf) {
        let (text, eol, enc, title_old) = {
            let tab = &mut self.tabs[idx];
            (tab.text.clone(), tab.eol, tab.encoding, tab.title.clone())
        };
        match files::save(&path, &text, eol, enc) {
            Ok(had_errors) => {
                let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
                let title = path
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or(title_old);
                let tab = &mut self.tabs[idx];
                tab.path = Some(path.clone());
                tab.title = title;
                tab.dirty = false;
                tab.mtime = mtime;
                self.settings.push_recent(&path.display().to_string());
                self.settings.set_last_dir_from(&path);
                self.sync_session();
                self.save_settings();
                if had_errors {
                    self.toast(format!(
                        "已保存,但部分字符无法用 {} 表示,已替换为 �",
                        enc.label()
                    ));
                } else {
                    self.toast(format!("已保存:{}", path.display()));
                }
            }
            Err(e) => self.toast(e),
        }
    }

    fn save_active(&mut self) {
        if let Some(idx) = self.active_idx() {
            if self.tabs[idx].readonly {
                self.toast("文件处于只读模式,无法保存");
                return;
            }
            self.save_tab_index(idx);
        }
    }

    fn save_all(&mut self) {
        let dirty: Vec<usize> = (0..self.tabs.len())
            .filter(|&i| self.tabs[i].dirty && !self.tabs[i].readonly)
            .collect();
        for idx in dirty {
            self.save_tab_index(idx);
        }
        self.toast("已全部保存");
    }

    /// 保存所有未保存项;用户取消另存为时返回 false。
    fn save_all_dirty(&mut self) -> bool {
        let dirty: Vec<usize> = (0..self.tabs.len()).filter(|&i| self.tabs[i].dirty).collect();
        for idx in dirty {
            if !self.save_tab_index(idx) {
                return false;
            }
        }
        true
    }

    fn remove_tab(&mut self, uid: u64) {
        if let Some(i) = self.tabs.iter().position(|t| t.uid == uid) {
            self.tabs.remove(i);
            self.fix_active();
            self.current_match = None;
            self.sync_session();
            self.save_settings();
        }
    }

    fn request_close_tab(&mut self, idx: usize) {
        if let Some(tab) = self.tabs.get(idx) {
            if tab.dirty {
                self.pending_close = PendingClose::CloseTab(tab.uid);
            } else {
                let uid = tab.uid;
                self.remove_tab(uid);
            }
        }
    }

    fn close_all_tabs(&mut self) {
        if self.tabs.iter().any(|t| t.dirty) {
            self.pending_close = PendingClose::CloseAll;
        } else {
            let uids: Vec<u64> = self.tabs.iter().map(|t| t.uid).collect();
            for u in uids {
                self.remove_tab(u);
            }
        }
    }

    fn close_other_tabs(&mut self, keep_uid: u64) {
        let uids: Vec<u64> = self
            .tabs
            .iter()
            .filter(|t| t.uid != keep_uid && !t.dirty)
            .map(|t| t.uid)
            .collect();
        for u in uids {
            self.remove_tab(u);
        }
        // 有脏标签时保持打开(简化处理)
        if self.tabs.iter().any(|t| t.uid != keep_uid && t.dirty) {
            self.toast("存在未保存的标签,已保留");
        }
    }

    fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + 1) % self.tabs.len();
        }
    }

    fn prev_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
        }
    }

    fn reveal_in_explorer(&mut self) {
        let Some(idx) = self.active_idx() else { return };
        let Some(path) = self.tabs[idx].path.clone() else {
            self.toast("文件尚未保存,无法定位");
            return;
        };
        #[cfg(target_os = "windows")]
        {
            let _ = std::process::Command::new("explorer")
                .arg("/select,")
                .arg(&path)
                .spawn();
        }
        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("open").arg("-R").arg(&path).spawn();
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
        }
    }

    // -----------------------------------------------------------------------
    // 查找/替换
    // -----------------------------------------------------------------------

    fn ensure_matcher(&mut self) {
        let key = self.find.key();
        let same = match (&self.matcher_cache, &key) {
            (Some((k, _)), Some(kk)) => k == kk,
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        self.matcher_cache = match key {
            Some(k) => Some((k, Matcher::build(&self.find))),
            None => None,
        };
        self.find_error = match &self.matcher_cache {
            Some((_, Err(e))) => Some(e.clone()),
            _ => None,
        };
        self.current_match = None;
    }

    /// 重建当前文档的匹配区间缓存(仅在文本或查找条件变化时)。
    fn ensure_matches(&mut self) {
        let text_matches_active = self
            .active_idx()
            .map(|i| self.tabs[i].text.as_str())
            .unwrap_or("");
        let key = if self.find_win || self.results_open {
            self.find.key()
        } else {
            None
        };
        if self.match_key == key && self.match_text == text_matches_active {
            return;
        }
        self.match_key = key.clone();
        self.match_text = text_matches_active.to_string();
        self.current_match = None;
        self.match_ranges = Vec::new();
        if let Some((_, Ok(m))) = &self.matcher_cache {
            if key.is_some() && !text_matches_active.is_empty() {
                let (ranges, _) = m.find_all(text_matches_active, 5000);
                self.match_ranges = sanitize_ranges(text_matches_active, &ranges);
            }
        }
    }

    fn do_find(&mut self, dir: i32) {
        self.ensure_matcher();
        let Some(idx) = self.active_idx() else { return };
        let Some((_, Ok(m))) = &self.matcher_cache else {
            self.toast("请先输入查找内容");
            return;
        };
        let text = self.tabs[idx].text.clone();
        let cursor_byte = textops::char_to_byte(&text, self.tabs[idx].cursor_char);
        let found = if dir > 0 {
            m.find_next(&text, cursor_byte)
        } else {
            m.find_prev(&text, cursor_byte)
        };
        let found = found.or_else(|| {
            if self.find.loop_around {
                if dir > 0 {
                    m.find_next(&text, 0)
                } else {
                    m.find_prev(&text, text.len())
                }
            } else {
                None
            }
        });
        match found {
            Some(r) => {
                let (cs, ce) = (
                    textops::byte_to_char(&text, r.start),
                    textops::byte_to_char(&text, r.end),
                );
                self.tabs[idx].jump_to = Some((cs, ce));
                self.current_match = Some(r);
            }
            None => {
                self.toast(format!("未找到 \"{}\"", self.find.query));
            }
        }
    }

    fn do_count(&mut self) {
        self.ensure_matcher();
        let Some((_, Ok(m))) = &self.matcher_cache else { return };
        let Some(idx) = self.active_idx() else { return };
        let n = m.count(&self.tabs[idx].text);
        self.toast(format!("共找到 {} 处匹配", n));
    }

    fn find_in_all_files(&mut self) {
        self.ensure_matcher();
        let Some((_, Ok(m))) = &self.matcher_cache else { return };
        let cap = 5000usize;
        let mut hits = Vec::new();
        let mut truncated = false;
        for tab in &self.tabs {
            if hits.len() >= cap {
                truncated = true;
                break;
            }
            let (ranges, trunc) = m.find_all(&tab.text, cap - hits.len());
            truncated |= trunc;
            for r in ranges {
                let cs = textops::byte_to_char(&tab.text, r.start);
                let (line, col) = textops::line_col(&tab.text, cs);
                let line_start = tab.text[..r.start].rfind('\n').map_or(0, |p| p + 1);
                let line_end = tab.text[r.start..]
                    .find('\n')
                    .map_or(tab.text.len(), |p| r.start + p);
                let preview: String = tab.text[line_start..line_end]
                    .trim_start()
                    .chars()
                    .take(140)
                    .collect();
                hits.push(SearchHit {
                    tab_uid: tab.uid,
                    title: tab.title.clone(),
                    line,
                    col,
                    preview,
                    char_range: (
                        cs,
                        textops::byte_to_char(&tab.text, r.end),
                    ),
                });
            }
        }
        self.results_truncated = truncated;
        let total = hits.len();
        self.results = hits;
        self.results_open = true;
        self.toast(format!("在所有打开的文件中共找到 {} 处匹配", total));
    }

    fn do_replace_one(&mut self) {
        self.ensure_matcher();
        let Some(idx) = self.active_idx() else { return };
        if self.tabs[idx].readonly {
            self.toast("文件处于只读模式");
            return;
        }
        let Some((_, Ok(m))) = &self.matcher_cache else { return };
        let text = self.tabs[idx].text.clone();
        let replaced;
        if let Some((cs, ce)) = self.tabs[idx].sel_char {
            if cs < ce && ce <= text.chars().count() {
                let sb = textops::char_to_byte(&text, cs);
                let eb = textops::char_to_byte(&text, ce);
                let new_frag = if self.find.regex {
                    m.replace_captures(&text[sb..eb], &self.replace_text)
                } else {
                    self.replace_text.clone()
                };
                let mut new_text = text[..sb].to_string();
                new_text.push_str(&new_frag);
                new_text.push_str(&text[eb..]);
                self.tabs[idx].text = new_text;
                self.tabs[idx].dirty = true;
                self.tabs[idx].cursor_char = cs + new_frag.chars().count();
                self.tabs[idx].sel_char = None;
                self.current_match = None;
                replaced = true;
            } else {
                replaced = false;
            }
        } else {
            replaced = false;
        }
        if !replaced {
            // 未选中内容:先定位到下一个匹配
            self.do_find(1);
        }
    }

    fn do_replace_all(&mut self) {
        self.ensure_matcher();
        let Some(idx) = self.active_idx() else { return };
        if self.tabs[idx].readonly {
            self.toast("文件处于只读模式");
            return;
        }
        let Some((_, Ok(m))) = &self.matcher_cache else { return };
        let text = self.tabs[idx].text.clone();
        let (new_text, n) = m.replace_all(&text, &self.replace_text, !self.find.regex);
        self.tabs[idx].text = new_text;
        self.tabs[idx].dirty = true;
        self.current_match = None;
        self.tabs[idx].jump_to = Some((0, 0));
        self.toast(format!("已替换 {} 处", n));
    }

    // -----------------------------------------------------------------------
    // 文本变换
    // -----------------------------------------------------------------------

    fn apply_line_op(&mut self, op: LineOp) {
        let Some(idx) = self.active_idx() else { return };
        if self.tabs[idx].readonly {
            self.toast("文件处于只读模式");
            return;
        }
        let sel_byte = self.tabs[idx]
            .sel_char
            .map(|(a, b)| {
                let (sa, sb) = (
                    textops::char_to_byte(&self.tabs[idx].text, a),
                    textops::char_to_byte(&self.tabs[idx].text, b),
                );
                sa..sb
            })
            .filter(|r| r.start < r.end);
        let cursor = self.tabs[idx].cursor_char;
        let (new_text, cursor) = textops::apply_line_op(&self.tabs[idx].text, sel_byte, cursor, op);
        self.tabs[idx].text = new_text;
        self.tabs[idx].dirty = true;
        self.tabs[idx].jump_to = Some((cursor, cursor));
        self.current_match = None;
    }

    fn apply_case_op(&mut self, op: CaseOp) {
        let Some(idx) = self.active_idx() else { return };
        if self.tabs[idx].readonly {
            self.toast("文件处于只读模式");
            return;
        }
        let sel_byte = self.tabs[idx]
            .sel_char
            .map(|(a, b)| {
                textops::char_to_byte(&self.tabs[idx].text, a)
                    ..textops::char_to_byte(&self.tabs[idx].text, b)
            })
            .filter(|r| r.start < r.end);
        let (new_text, cursor) = textops::apply_case(&self.tabs[idx].text, sel_byte, op);
        self.tabs[idx].text = new_text;
        self.tabs[idx].dirty = true;
        self.tabs[idx].jump_to = Some((cursor, cursor));
        self.current_match = None;
    }

    fn open_goto(&mut self) {
        if self.active_idx().is_none() {
            return;
        }
        self.goto_win = true;
        self.goto_text.clear();
        self.goto_focus_pending = true;
    }

    fn do_goto(&mut self) {
        let Ok(n) = self.goto_text.trim().parse::<usize>() else {
            self.toast("请输入有效的行号");
            return;
        };
        let Some(idx) = self.active_idx() else { return };
        let lines = self.tabs[idx].line_count();
        let target = n.clamp(1, lines);
        if let Some((s, e)) = textops::line_char_range(&self.tabs[idx].text, target) {
            self.tabs[idx].jump_to = Some((s, e));
        }
        self.goto_win = false;
    }

    // -----------------------------------------------------------------------
    // 全局快捷键
    // -----------------------------------------------------------------------

    fn global_keys(&mut self, ui: &mut Ui) {
        let m = ui.input(|i| i.modifiers);
        let pressed = |key: Key| ui.input(|i| i.key_pressed(key));
        let ctrl = m.ctrl || m.command;
        let no_mod = |ui: &Ui| ui.input(|i| !i.modifiers.any());
        let _ = no_mod;

        // 顺序:组合键从长到短,避免 Ctrl+Shift+F 被 Ctrl+F 截胡
        if pressed(Key::F) && ctrl && m.shift {
            self.find_in_all_files();
        } else if pressed(Key::F) && ctrl {
            self.find_win = true;
            self.replace_row = false;
            self.find_focus_pending = true;
        }
        if pressed(Key::H) && ctrl {
            self.find_win = true;
            self.replace_row = true;
            self.find_focus_pending = true;
        }
        if pressed(Key::F3) && m.shift {
            self.do_find(-1);
        } else if pressed(Key::F3) {
            self.do_find(1);
        }
        if pressed(Key::G) && ctrl {
            self.open_goto();
        }

        if pressed(Key::N) && ctrl && !m.shift {
            self.new_tab();
        }
        if pressed(Key::O) && ctrl {
            self.open_dialog();
        }
        if pressed(Key::S) && ctrl && m.alt {
            let _ = self.save_as_dialog_index(self.active_idx().unwrap_or(0));
        } else if pressed(Key::S) && ctrl && m.shift {
            self.save_all();
        } else if pressed(Key::S) && ctrl {
            self.save_active();
        }
        if pressed(Key::W) && ctrl && m.shift {
            self.close_all_tabs();
        } else if pressed(Key::W) && ctrl {
            if let Some(idx) = self.active_idx() {
                self.request_close_tab(idx);
            }
        }
        if pressed(Key::Tab) && ctrl && m.shift {
            self.prev_tab();
        } else if pressed(Key::Tab) && ctrl {
            self.next_tab();
        }

        // 行操作
        if self.active_idx().is_some() {
            if pressed(Key::D) && ctrl && !m.shift {
                self.apply_line_op(LineOp::Duplicate);
            }
            if pressed(Key::L) && ctrl && !m.shift {
                self.apply_line_op(LineOp::Delete);
            }
            if pressed(Key::ArrowUp) && m.alt && !ctrl {
                self.apply_line_op(LineOp::MoveUp);
            }
            if pressed(Key::ArrowDown) && m.alt && !ctrl {
                self.apply_line_op(LineOp::MoveDown);
            }
        }

        // 缩放
        if pressed(Key::Equals) && ctrl {
            self.settings.font_size = (self.settings.font_size + 1.0).clamp(8.0, 72.0);
        }
        if pressed(Key::Minus) && ctrl {
            self.settings.font_size = (self.settings.font_size - 1.0).clamp(8.0, 72.0);
        }
        if pressed(Key::Num0) && ctrl {
            self.settings.font_size = 14.0;
        }
        let zd = ui.input(|i| i.zoom_delta());
        if zd != 1.0 {
            self.settings.font_size = (self.settings.font_size * zd).clamp(8.0, 72.0);
        }

        // 全屏
        if pressed(Key::F11) {
            let full = ui.input(|i| i.viewport().fullscreen).unwrap_or(false);
            ui.ctx().send_viewport_cmd(ViewportCommand::Fullscreen(!full));
        }

        // 自动换行 Alt+Z
        if pressed(Key::Z) && m.alt && !ctrl {
            self.settings.wrap = !self.settings.wrap;
        }
    }

    // -----------------------------------------------------------------------
    // 外部修改监视 / 拖放
    // -----------------------------------------------------------------------

    fn check_external_changes(&mut self, ui: &mut Ui) {
        let focused = ui.input(|i| i.viewport().focused);
        if focused == Some(true) && self.was_focused != Some(true) {
            let tabs_meta: Vec<(u64, Option<std::time::SystemTime>)> = self
                .tabs
                .iter()
                .filter_map(|t| t.path.clone().map(|p| (t.uid, std::fs::metadata(&p).and_then(|m| m.modified()).ok())))
                .collect();
            for (uid, mtime) in tabs_meta {
                let Some(idx) = self.tabs.iter().position(|t| t.uid == uid) else { continue };
                let Some(disk) = mtime else { continue };
                if self.tabs[idx].mtime != Some(disk) {
                    if self.tabs[idx].dirty {
                        self.tabs[idx].mtime = Some(disk);
                        self.toast(format!("\"{}\" 已被外部修改(当前有未保存更改)", self.tabs[idx].title));
                    } else if let Some(path) = self.tabs[idx].path.clone() {
                        match files::load(&path) {
                            Ok(lf) => {
                                let title = self.tabs[idx].title.clone();
                                self.tabs[idx].text = lf.text;
                                self.tabs[idx].encoding = lf.encoding;
                                self.tabs[idx].eol = lf.eol;
                                self.tabs[idx].mtime = lf.mtime;
                                self.tabs[idx].dirty = false;
                                self.toast(format!("\"{}\" 已被外部修改,已自动重新加载", title));
                            }
                            Err(e) => self.toast(e),
                        }
                    }
                }
            }
        }
        self.was_focused = focused;
    }

    fn handle_dropped_files(&mut self, ui: &mut Ui) {
        let dropped: Vec<_> = ui.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            let p = f.path().to_path_buf();
            if p.is_file() {
                self.open_path(p);
            }
        }
        let hovering = ui.input(|i| !i.raw.hovered_files.is_empty());
        if hovering {
            egui::Area::new(Id::new("drop_overlay"))
                .order(egui::Order::Foreground)
                .fixed_pos(egui::Pos2::ZERO)
                .show(ui.ctx(), |ui| {
                    let screen = ui
                        .input(|i| i.raw.screen_rect.unwrap_or(egui::Rect::ZERO));
                    let painter = ui.painter();
                    painter.rect_filled(
                        screen,
                        0.0,
                        Color32::from_rgba_unmultiplied(120, 180, 255, 40),
                    );
                    painter.rect_filled(
                        screen.shrink(24.0),
                        6.0,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 0),
                    );
                    painter.text(
                        screen.center(),
                        Align2::CENTER_CENTER,
                        "松开以打开文件",
                        FontId::proportional(26.0),
                        Color32::WHITE,
                    );
                });
        }
    }

    // -----------------------------------------------------------------------
    // UI:主循环
    // -----------------------------------------------------------------------

    fn main_ui(&mut self, ui: &mut Ui) {
        // 每帧应用:编辑器字体缩放 + 面板底色与编辑器底色一致
        let size = self.settings.font_size;
        ui.ctx().all_styles_mut(|s| {
            s.text_styles
                .insert(TextStyle::Monospace, FontId::monospace(size));
            s.visuals.panel_fill = s.visuals.extreme_bg_color;
        });

        self.global_keys(ui);
        self.handle_close_request(ui);
        self.check_external_changes(ui);
        self.handle_dropped_files(ui);

        egui::Panel::top("menu_bar").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::top("tab_bar").show(ui, |ui| self.tab_bar(ui));

        if self.results_open {
            egui::Panel::bottom("search_results")
                .resizable(true)
                .default_size(170.0)
                .show(ui, |ui| self.results_panel(ui));
        }
        if self.settings.show_status_bar {
            egui::Panel::bottom("status_bar").show(ui, |ui| self.status_bar(ui));
        }

        egui::CentralPanel::default().show(ui, |ui| self.editor_area(ui));

        self.find_window(ui);
        self.goto_window(ui);
        self.confirm_close_window(ui);
        if self.about_open {
            self.about_window(ui);
        }
        if self.help_open {
            self.help_window(ui);
        }

        self.draw_toasts(ui);
        self.update_title(ui);
    }

    fn handle_close_request(&mut self, ui: &mut Ui) {
        let closing = ui.input(|i| i.viewport().close_requested());
        if closing && !self.allow_close && self.tabs.iter().any(|t| t.dirty) {
            ui.send_viewport_cmd(ViewportCommand::CancelClose);
            self.pending_close = PendingClose::Exit;
        }
        // 无脏文件:放行,由 eframe 正常退出(on_exit 保存设置)
    }

    // -----------------------------------------------------------------------
    // UI:菜单栏
    // -----------------------------------------------------------------------

    fn menu_bar(&mut self, ui: &mut Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            let has_tab = self.active_idx().is_some();

            // ---------------- 文件 ----------------
            ui.menu_button("文件", |ui| {
                if item(ui, "新建", "Ctrl+N", true).clicked() {
                    ui.close();
                    self.new_tab();
                }
                if item(ui, "打开...", "Ctrl+O", true).clicked() {
                    ui.close();
                    self.open_dialog();
                }
                ui.menu_button("打开最近的文件", |ui| {
                    let mut to_open: Option<PathBuf> = None;
                    if self.settings.recent.is_empty() {
                        ui.add_enabled(false, Button::new("(空)"));
                    }
                    for r in self.settings.recent.clone() {
                        if ui.add(Button::new(r.clone())).clicked() {
                            to_open = Some(PathBuf::from(r));
                        }
                    }
                    ui.separator();
                    if ui.add(Button::new("清空最近列表")).clicked() {
                        self.settings.recent.clear();
                    }
                    if let Some(p) = to_open {
                        ui.close();
                        self.open_path(p);
                    }
                });
                ui.separator();
                if item(ui, "保存", "Ctrl+S", has_tab).clicked() {
                    ui.close();
                    self.save_active();
                }
                if item(ui, "另存为...", "Ctrl+Alt+S", has_tab).clicked() {
                    ui.close();
                    if let Some(idx) = self.active_idx() {
                        self.save_as_dialog_index(idx);
                    }
                }
                if item(ui, "全部保存", "Ctrl+Shift+S", has_tab).clicked() {
                    ui.close();
                    self.save_all();
                }
                ui.separator();
                if item(ui, "关闭", "Ctrl+W", has_tab).clicked() {
                    ui.close();
                    if let Some(idx) = self.active_idx() {
                        self.request_close_tab(idx);
                    }
                }
                if item(ui, "关闭全部", "Ctrl+Shift+W", has_tab).clicked() {
                    ui.close();
                    self.close_all_tabs();
                }
                if item(ui, "关闭其他标签", "", has_tab).clicked() {
                    ui.close();
                    if let Some(idx) = self.active_idx() {
                        let uid = self.tabs[idx].uid;
                        self.close_other_tabs(uid);
                    }
                }
                ui.separator();
                ui.checkbox(&mut self.settings.restore_session, "启动时恢复上次会话");
                ui.separator();
                if item(ui, "在资源管理器中显示", "", has_tab).clicked() {
                    ui.close();
                    self.reveal_in_explorer();
                }
                ui.separator();
                if item(ui, "退出", "", true).clicked() {
                    ui.close();
                    if self.tabs.iter().any(|t| t.dirty) {
                        self.pending_close = PendingClose::Exit;
                    } else {
                        self.allow_close = true;
                        ui.send_viewport_cmd(ViewportCommand::Close);
                    }
                }
            });

            // ---------------- 编辑 ----------------
            ui.menu_button("编辑", |ui| {
                let Some(idx) = self.active_idx() else {
                    ui.add_enabled(false, Button::new("(无打开的文件)"));
                    return;
                };
                let readonly = self.tabs[idx].readonly;
                if ui.checkbox(&mut self.tabs[idx].readonly, "只读模式").changed() {
                    ui.close();
                }
                ui.separator();

                ui.menu_button("行操作", |ui| {
                    if item(ui, "复制当前行", "Ctrl+D", !readonly).clicked() {
                        ui.close();
                        self.apply_line_op(LineOp::Duplicate);
                    }
                    if item(ui, "删除当前行", "Ctrl+L", !readonly).clicked() {
                        ui.close();
                        self.apply_line_op(LineOp::Delete);
                    }
                    if item(ui, "上移一行", "Alt+Up", !readonly).clicked() {
                        ui.close();
                        self.apply_line_op(LineOp::MoveUp);
                    }
                    if item(ui, "下移一行", "Alt+Down", !readonly).clicked() {
                        ui.close();
                        self.apply_line_op(LineOp::MoveDown);
                    }
                    ui.separator();
                    if item(ui, "升序排序", "", !readonly).clicked() {
                        ui.close();
                        self.apply_line_op(LineOp::SortAsc);
                    }
                    if item(ui, "降序排序", "", !readonly).clicked() {
                        ui.close();
                        self.apply_line_op(LineOp::SortDesc);
                    }
                    if item(ui, "反转行序", "", !readonly).clicked() {
                        ui.close();
                        self.apply_line_op(LineOp::Reverse);
                    }
                    if item(ui, "删除重复行", "", !readonly).clicked() {
                        ui.close();
                        self.apply_line_op(LineOp::RemoveDup);
                    }
                    ui.separator();
                    if item(ui, "删除空行", "", !readonly).clicked() {
                        ui.close();
                        self.apply_line_op(LineOp::RemoveEmpty);
                    }
                    if item(ui, "删除空白行(含空白字符)", "", !readonly).clicked() {
                        ui.close();
                        self.apply_line_op(LineOp::RemoveBlank);
                    }
                    ui.separator();
                    if item(ui, "去除行首空白", "", !readonly).clicked() {
                        ui.close();
                        self.apply_line_op(LineOp::TrimLeading);
                    }
                    if item(ui, "去除行尾空白", "", !readonly).clicked() {
                        ui.close();
                        self.apply_line_op(LineOp::TrimTrailing);
                    }
                    if item(ui, "合并为一行", "", !readonly).clicked() {
                        ui.close();
                        self.apply_line_op(LineOp::Join);
                    }
                });

                ui.menu_button("大小写转换", |ui| {
                    if item(ui, "全部大写", "", !readonly).clicked() {
                        ui.close();
                        self.apply_case_op(CaseOp::Upper);
                    }
                    if item(ui, "全部小写", "", !readonly).clicked() {
                        ui.close();
                        self.apply_case_op(CaseOp::Lower);
                    }
                    if item(ui, "大小写互换", "", !readonly).clicked() {
                        ui.close();
                        self.apply_case_op(CaseOp::Swap);
                    }
                    if item(ui, "首字母大写", "", !readonly).clicked() {
                        ui.close();
                        self.apply_case_op(CaseOp::Title);
                    }
                });

                // 行尾符
                ui.menu_button("行尾符转换", |ui| {
                    for eol in Eol::ALL {
                        if ui
                            .radio(self.tabs[idx].eol == eol, eol.label())
                            .clicked()
                        {
                            self.tabs[idx].eol = eol;
                            ui.close();
                        }
                    }
                    ui.separator();
                    ui.label(RichText::new("(保存文件时按此换行符写出)").size(11.5).weak());
                });

                // 编码
                ui.menu_button("编码", |ui| {
                    ui.label(
                        RichText::new(format!("当前编码:{}", self.tabs[idx].encoding.label()))
                            .size(11.5)
                            .weak(),
                    );
                    ui.separator();
                    ui.menu_button("转换为(保存时生效)", |ui| {
                        for enc in FileEncoding::ALL {
                            if ui
                                .radio(self.tabs[idx].encoding == enc, enc.label())
                                .clicked()
                            {
                                self.tabs[idx].encoding = enc;
                                ui.close();
                            }
                        }
                    });
                    if self.tabs[idx].path.is_some() {
                        ui.menu_button("以编码重新加载", |ui| {
                            let mut action: Option<FileEncoding> = None;
                            for enc in FileEncoding::ALL {
                                if ui.add(Button::new(enc.label())).clicked() {
                                    action = Some(enc);
                                }
                            }
                            if let Some(enc) = action {
                                let path = self.tabs[idx].path.clone().unwrap();
                                match files::reload_with(&path, enc) {
                                    Ok((eol, text)) => {
                                        self.tabs[idx].text = text;
                                        self.tabs[idx].eol = eol;
                                        self.tabs[idx].encoding = enc;
                                        self.tabs[idx].dirty = false;
                                        ui.close();
                                        self.current_match = None;
                                    }
                                    Err(e) => self.toast(e),
                                }
                            }
                        });
                    }
                });

                ui.separator();
                if item(ui, "插入日期时间", "F5", !readonly).clicked() {
                    ui.close();
                    // 光标在编辑器内时由编辑器处理;这里作用于光标位置
                    let tab = &mut self.tabs[idx];
                    let stamp = chrono::Local::now()
                        .format("%Y-%m-%d %H:%M:%S")
                        .to_string();
                    let at = tab.cursor_char.min(tab.text.chars().count());
                    let b = textops::char_to_byte(&tab.text, at);
                    tab.text.insert_str(b, &stamp);
                    tab.dirty = true;
                    tab.jump_to = Some((at + stamp.chars().count(), at + stamp.chars().count()));
                }
            });

            // ---------------- 搜索 ----------------
            ui.menu_button("搜索", |ui| {
                if item(ui, "查找...", "Ctrl+F", true).clicked() {
                    ui.close();
                    self.find_win = true;
                    self.replace_row = false;
                    self.find_focus_pending = true;
                }
                if item(ui, "查找下一个", "F3", true).clicked() {
                    ui.close();
                    self.do_find(1);
                }
                if item(ui, "查找上一个", "Shift+F3", true).clicked() {
                    ui.close();
                    self.do_find(-1);
                }
                if item(ui, "替换...", "Ctrl+H", true).clicked() {
                    ui.close();
                    self.find_win = true;
                    self.replace_row = true;
                    self.find_focus_pending = true;
                }
                ui.separator();
                if item(ui, "计数当前文件", "", has_tab).clicked() {
                    ui.close();
                    self.do_count();
                }
                if item(ui, "在所有打开的文件中查找", "Ctrl+Shift+F", has_tab).clicked() {
                    ui.close();
                    self.find_in_all_files();
                }
                ui.separator();
                if item(ui, "转到行...", "Ctrl+G", has_tab).clicked() {
                    ui.close();
                    self.open_goto();
                }
                ui.separator();
                ui.checkbox(&mut self.results_open, "显示查找结果面板");
            });

            // ---------------- 格式 ----------------
            ui.menu_button("格式", |ui| {
                ui.checkbox(&mut self.settings.wrap, "自动换行");
                ui.checkbox(&mut self.settings.highlight, "语法着色");
                ui.separator();
                ui.menu_button("语言", |ui| {
                    if let Some(idx) = self.active_idx() {
                        let mut set: Option<Lang> = None;
                        for lang in Lang::ALL {
                            if ui
                                .radio(self.tabs[idx].lang == lang, lang.label())
                                .clicked()
                            {
                                set = Some(lang);
                            }
                        }
                        if let Some(lang) = set {
                            self.tabs[idx].lang = lang;
                            ui.close();
                        }
                    } else {
                        ui.add_enabled(false, Button::new("(无打开的文件)"));
                    }
                });
            });

            // ---------------- 查看 ----------------
            ui.menu_button("查看", |ui| {
                let mut dark = self.settings.dark;
                if ui.checkbox(&mut dark, "暗色主题").changed() {
                    self.settings.dark = dark;
                    ui.ctx().set_theme(if dark {
                        egui::ThemePreference::Dark
                    } else {
                        egui::ThemePreference::Light
                    });
                }
                ui.separator();
                if item(ui, "放大", "Ctrl+ = / Ctrl+滚轮", true).clicked() {
                    ui.close();
                    self.settings.font_size = (self.settings.font_size + 1.0).clamp(8.0, 72.0);
                }
                if item(ui, "缩小", "Ctrl+ -", true).clicked() {
                    ui.close();
                    self.settings.font_size = (self.settings.font_size - 1.0).clamp(8.0, 72.0);
                }
                if item(ui, "还原缩放", "Ctrl+0", true).clicked() {
                    ui.close();
                    self.settings.font_size = 14.0;
                }
                ui.separator();
                ui.checkbox(&mut self.settings.show_status_bar, "显示状态栏");
                ui.separator();
                if item(ui, "全屏", "F11", true).clicked() {
                    ui.close();
                    let full = ui.input(|i| i.viewport().fullscreen).unwrap_or(false);
                    ui.ctx().send_viewport_cmd(ViewportCommand::Fullscreen(!full));
                }
            });

            // ---------------- 帮助 ----------------
            ui.menu_button("帮助", |ui| {
                if item(ui, "快捷键说明", "", true).clicked() {
                    ui.close();
                    self.help_open = true;
                }
                if item(ui, "关于 NppRust", "", true).clicked() {
                    ui.close();
                    self.about_open = true;
                }
            });
        });

        // 设置变化即时持久化
        self.save_settings_throttled();
    }

    fn save_settings_throttled(&mut self) {
        // 简单节流:每 10 秒最多写一次设置文件
        let now = Instant::now();
        let should = match self.last_settings_save {
            Some(t) => now.duration_since(t).as_secs_f32() > 10.0,
            None => true,
        };
        if should {
            self.last_settings_save = Some(now);
            self.sync_session();
            settings::save(&self.settings);
        }
    }

    // -----------------------------------------------------------------------
    // UI:标签栏
    // -----------------------------------------------------------------------

    fn tab_bar(&mut self, ui: &mut Ui) {
        let mut activate: Option<usize> = None;
        let mut close: Option<usize> = None;
        let mut actions: Vec<(usize, TabAction)> = Vec::new();

        ui.horizontal(|ui| {
            if ui.add(Button::new(" ＋ ")).clicked() {
                self.new_tab();
            }
            ScrollArea::horizontal()
                .id_salt("tabs_scroll")
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for i in 0..self.tabs.len() {
                            let sel = i == self.active;
                            let (label, dirty, path_str) = {
                                let t = &self.tabs[i];
                                (
                                    format!("{}{}", if t.dirty { "● " } else { "" }, t.title),
                                    t.dirty,
                                    t.display_path(),
                                )
                            };
                            let r = ui.add(
                                Button::new(RichText::new(&label).size(12.5))
                                    .selected(sel)
                                    .min_size(Vec2::new(0.0, 24.0)),
                            );
                            if r.clicked() {
                                activate = Some(i);
                            }
                            if r.middle_clicked() {
                                close = Some(i);
                            }
                            let _ = dirty;
                            r.context_menu(|menu| {
                                if menu.button("保存").clicked() {
                                    menu.close();
                                    actions.push((i, TabAction::Save));
                                }
                                if menu.button("另存为...").clicked() {
                                    menu.close();
                                    actions.push((i, TabAction::SaveAs));
                                }
                                if menu.button("关闭").clicked() {
                                    menu.close();
                                    close = Some(i);
                                }
                                if menu.button("关闭其他标签").clicked() {
                                    menu.close();
                                    actions.push((i, TabAction::CloseOthers));
                                }
                                if menu.button("复制文件路径").clicked() {
                                    menu.close();
                                    menu.copy_text(path_str.clone());
                                }
                            });
                            if ui
                                .add(
                                    Button::new(RichText::new("✕").size(10.0))
                                        .small()
                                        .min_size(Vec2::new(18.0, 24.0)),
                                )
                                .clicked()
                            {
                                close = Some(i);
                            }
                        }
                    });
                });
        });

        if let Some(i) = activate {
            if i < self.tabs.len() {
                self.active = i;
            }
        }
        if let Some(i) = close {
            self.request_close_tab(i);
        }
        for (i, action) in actions {
            match action {
                TabAction::Save => {
                    self.save_tab_index(i);
                }
                TabAction::SaveAs => {
                    self.save_as_dialog_index(i);
                }
                TabAction::CloseOthers => {
                    let uid = self.tabs.get(i).map(|t| t.uid);
                    if let Some(uid) = uid {
                        self.close_other_tabs(uid);
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // UI:查找结果面板
    // -----------------------------------------------------------------------

    fn results_panel(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("查找结果").strong());
            ui.label(
                RichText::new(format!(
                    "(共 {} 条{})",
                    self.results.len(),
                    if self.results_truncated { ",已截断" } else { "" }
                ))
                .size(11.5)
                .weak(),
            );
            if ui.button("清空并关闭").clicked() {
                self.results.clear();
                self.results_open = false;
            }
        });
        ui.separator();
        ScrollArea::vertical().show(ui, |ui| {
            let mut jump: Option<(u64, (usize, usize))> = None;
            for hit in &self.results {
                let text = format!("{}  行 {}:{}", hit.title, hit.line, hit.col);
                let resp = ui.selectable_label(false, RichText::new(&text).size(12.0).monospace());
                if resp.clicked() {
                    jump = Some((hit.tab_uid, hit.char_range));
                }
                resp.on_hover_text(&hit.preview);
            }
            if let Some((uid, (s, e))) = jump {
                if let Some(i) = self.tabs.iter().position(|t| t.uid == uid) {
                    self.active = i;
                    self.tabs[i].jump_to = Some((s, e));
                }
            }
        });
    }

    // -----------------------------------------------------------------------
    // UI:状态栏
    // -----------------------------------------------------------------------

    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let t = |s: String| RichText::new(s);
            if let Some(idx) = self.active_idx() {
                let tab = &self.tabs[idx];
                let (line, col) = textops::line_col(&tab.text, tab.cursor_char);
                let chars = tab.text.chars().count();
                let bytes = tab.text.len();
                let lines = tab.line_count();

                ui.label(t(tab.lang.label().to_string()).size(11.5));
                ui.separator();
                ui.label(t(format!("Ln {}, Col {}", line, col)).size(11.5));
                if tab.sel_chars > 0 {
                    ui.label(t(format!("选中 {} 字符", tab.sel_chars)).size(11.5));
                }
                ui.separator();
                ui.label(
                    t(format!("长度 {} 字符 / {}", chars, human_size(bytes))).size(11.5),
                );
                ui.separator();
                ui.label(t(format!("{} 行", lines)).size(11.5));

                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    let mode = if tab.readonly { "只读" } else { "INS" };
                    ui.label(t(mode.to_string()).size(11.5));
                    ui.label(t(format!("缩放 {}%", (self.settings.font_size / 14.0 * 100.0).round() as i32)).size(11.5));
                    ui.label(t(tab.encoding.label().to_string()).size(11.5));
                    ui.label(t(tab.eol.short().to_string()).size(11.5));
                });
            } else {
                ui.label(t("就绪".to_string()).size(11.5).weak());
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        t(format!(
                            "缩放 {}%",
                            (self.settings.font_size / 14.0 * 100.0).round() as i32
                        ))
                        .size(11.5),
                    );
                });
            }
        });
    }

    // -----------------------------------------------------------------------
    // UI:编辑区
    // -----------------------------------------------------------------------

    fn editor_area(&mut self, ui: &mut Ui) {
        if self.tabs.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.label(RichText::new(APP_NAME).size(46.0).weak());
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("拖入文件打开,或 Ctrl+O 打开 / Ctrl+N 新建")
                            .size(14.0)
                            .weak(),
                    );
                });
            });
            return;
        }

        self.ensure_matcher();
        self.ensure_matches();

        let Self {
            tabs,
            active,
            settings,
            editor_ui,
            match_ranges,
            current_match,
            ..
        } = self;
        let idx = (*active).min(tabs.len() - 1);
        let tab = &mut tabs[idx];

        let out = editor::show_editor(
            ui,
            tab,
            EditorOpts {
                wrap: settings.wrap,
                font_size: settings.font_size,
                dark: settings.dark,
                highlight: settings.highlight,
                matches: &match_ranges[..],
                current_match: current_match.clone(),
                cache: editor_ui,
            },
        );

        if out.text_changed {
            tab.dirty = true;
            *current_match = None;
        }
    }

    // -----------------------------------------------------------------------
    // UI:窗口(查找/替换、转到行、关闭确认、关于、帮助)
    // -----------------------------------------------------------------------

    fn find_window(&mut self, ui: &mut Ui) {
        if !self.find_win {
            return;
        }
        let mut open = true;
        let title = if self.replace_row { "替换" } else { "查找" };
        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                egui::Grid::new("find_grid")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("查找内容:");
                        let resp = ui.add(
                            TextEdit::singleline(&mut self.find.query)
                                .id(Id::new("find_query"))
                                .desired_width(300.0)
                                .hint_text("支持正则表达式(勾选后)"),
                        );
                        if self.find_focus_pending {
                            if resp.has_focus() {
                                self.find_focus_pending = false;
                            } else {
                                resp.request_focus();
                            }
                        }
                        ui.end_row();
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            self.do_find(1);
                        }
                        if self.replace_row {
                            ui.label("替换为:");
                            ui.add(
                                TextEdit::singleline(&mut self.replace_text).desired_width(300.0),
                            );
                            ui.end_row();
                        }
                    });

                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.find.case_sensitive, "区分大小写");
                    ui.checkbox(&mut self.find.whole_word, "全字匹配");
                    ui.checkbox(&mut self.find.regex, "正则表达式");
                    ui.checkbox(&mut self.find.loop_around, "循环查找");
                });

                if let Some(err) = &self.find_error {
                    ui.label(RichText::new(err).color(Color32::from_rgb(0xE0, 0x60, 0x60)).size(12.0));
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("查找下一个").clicked() {
                        self.do_find(1);
                    }
                    if ui.button("查找上一个").clicked() {
                        self.do_find(-1);
                    }
                    if self.replace_row {
                        if ui.button("替换").clicked() {
                            self.do_replace_one();
                        }
                        if ui.button("全部替换").clicked() {
                            self.do_replace_all();
                        }
                    }
                    if ui.button("计数").clicked() {
                        self.do_count();
                    }
                    if ui.button("在所有文件中查找").clicked() {
                        self.find_in_all_files();
                    }
                });
            });
        if !open {
            self.find_win = false;
        }
    }

    fn goto_window(&mut self, ui: &mut Ui) {
        if !self.goto_win {
            return;
        }
        let mut open = true;
        egui::Window::new("转到行")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                let field_id = Id::new("goto_input");
                let total = self
                    .active_idx()
                    .map(|i| self.tabs[i].line_count())
                    .unwrap_or(0);
                ui.label(format!("行号(1 - {}):", total));
                let resp = ui.add(
                    TextEdit::singleline(&mut self.goto_text)
                        .id(field_id)
                        .desired_width(160.0),
                );
                // 聚焦输入框(重试直到成功;对话框打开时自动执行)
                if self.goto_focus_pending {
                    if resp.has_focus() || ui.memory(|m| m.has_focus(field_id)) {
                        self.goto_focus_pending = false;
                    } else {
                        resp.request_focus();
                    }
                }
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                ui.horizontal(|ui| {
                    if ui.button("转到").clicked() || enter {
                        self.do_goto();
                    }
                    if ui.button("取消").clicked() {
                        self.goto_win = false;
                    }
                });
            });
        if !open {
            self.goto_win = false;
        }
    }

    fn confirm_close_window(&mut self, ui: &mut Ui) {
        if self.pending_close == PendingClose::None {
            return;
        }
        let (title, desc): (&str, String) = match self.pending_close {
            PendingClose::Exit => ("退出 NppRust", "以下文件有未保存的更改:".to_string()),
            PendingClose::CloseAll => ("关闭全部", "以下文件有未保存的更改:".to_string()),
            PendingClose::CloseTab(uid) => (
                "关闭文件",
                format!(
                    "\"{}\" 有未保存的更改,是否保存?",
                    self.tabs
                        .iter()
                        .find(|t| t.uid == uid)
                        .map(|t| t.title.as_str())
                        .unwrap_or("")
                ),
            ),
            PendingClose::None => unreachable!(),
        };
        let mut open = true;
        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                if !matches!(self.pending_close, PendingClose::CloseTab(_)) {
                    ui.label(desc);
                    let dirty: Vec<String> = self
                        .tabs
                        .iter()
                        .filter(|t| t.dirty)
                        .map(|t| format!("  ● {}", t.display_path()))
                        .collect();
                    let shown = dirty.iter().take(8);
                    for d in shown {
                        ui.label(RichText::new(d).size(12.0));
                    }
                    if dirty.len() > 8 {
                        ui.label(RichText::new(format!("  ...等 {} 个文件", dirty.len())).size(12.0));
                    }
                } else {
                    ui.label(desc);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    match self.pending_close {
                        PendingClose::CloseTab(uid) => {
                            if ui.button("保存并关闭").clicked() {
                                if let Some(i) = self.tabs.iter().position(|t| t.uid == uid) {
                                    if self.save_tab_index(i) {
                                        self.remove_tab(uid);
                                        self.pending_close = PendingClose::None;
                                    }
                                }
                            }
                            if ui.button("不保存关闭").clicked() {
                                self.remove_tab(uid);
                                self.pending_close = PendingClose::None;
                            }
                        }
                        PendingClose::Exit | PendingClose::CloseAll => {
                            let exit = self.pending_close == PendingClose::Exit;
                            let action = if exit {
                                "全部保存并退出"
                            } else {
                                "全部保存并关闭"
                            };
                            if ui.button(action).clicked() && self.save_all_dirty() {
                                if exit {
                                    self.allow_close = true;
                                    self.pending_close = PendingClose::None;
                                    ui.send_viewport_cmd(ViewportCommand::Close);
                                } else {
                                    let uids: Vec<u64> =
                                        self.tabs.iter().map(|t| t.uid).collect();
                                    for u in uids {
                                        self.remove_tab(u);
                                    }
                                    self.pending_close = PendingClose::None;
                                }
                            }
                            if ui.button("放弃更改").clicked() {
                                if exit {
                                    self.allow_close = true;
                                    ui.send_viewport_cmd(ViewportCommand::Close);
                                } else {
                                    let uids: Vec<u64> = self.tabs.iter().map(|t| t.uid).collect();
                                    for u in uids {
                                        self.remove_tab(u);
                                    }
                                }
                                self.pending_close = PendingClose::None;
                            }
                        }
                        PendingClose::None => {}
                    }
                    if ui.button("取消").clicked() {
                        self.pending_close = PendingClose::None;
                    }
                });
            });
        if !open {
            self.pending_close = PendingClose::None;
        }
    }

    fn about_window(&mut self, ui: &mut Ui) {
        let mut open = true;
        egui::Window::new("关于")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                ui.label(RichText::new(APP_NAME).size(26.0).strong());
                ui.label(format!("版本 {}", APP_VERSION));
                ui.separator();
                ui.label("用 Rust + egui 编写的类 Notepad++ 多标签文本/代码编辑器");
                ui.label(RichText::new("特性:语法高亮、行号、查找替换(正则)、行操作、").size(12.0).weak());
                ui.label(RichText::new("多编码(UTF-8/UTF-16/GBK/Big5)、明暗主题、会话恢复等").size(12.0).weak());
            });
        if !open {
            self.about_open = false;
        }
    }

    fn help_window(&mut self, ui: &mut Ui) {
        let mut open = true;
        egui::Window::new("快捷键说明")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                const LINES: &[&str] = &[
                    "Ctrl+N        新建标签",
                    "Ctrl+O        打开文件",
                    "Ctrl+S        保存",
                    "Ctrl+Alt+S    另存为",
                    "Ctrl+Shift+S  全部保存",
                    "Ctrl+W        关闭标签",
                    "Ctrl+Shift+W  关闭全部",
                    "Ctrl+Tab      下一个标签",
                    "Ctrl+Shift+Tab 上一个标签",
                    "Ctrl+F        查找",
                    "Ctrl+H        替换",
                    "F3 / Shift+F3 查找下一个/上一个",
                    "Ctrl+Shift+F  在所有打开的文件中查找",
                    "Ctrl+G        转到行",
                    "Ctrl+D        复制当前行",
                    "Ctrl+L        删除当前行",
                    "Alt+Up/Down  上移/下移一行",
                    "F5            插入日期时间",
                    "Ctrl+滚轮     缩放(Ctrl+0 还原)",
                    "Alt+Z         自动换行开关",
                    "F11           全屏",
                    "Tab / Shift+Tab  缩进/反缩进",
                ];
                for l in LINES {
                    ui.label(RichText::new(*l).monospace().size(12.0));
                }
            });
        if !open {
            self.help_open = false;
        }
    }

    // -----------------------------------------------------------------------
    // UI:浮动提示 / 标题
    // -----------------------------------------------------------------------

    fn draw_toasts(&mut self, ui: &mut Ui) {
        let now = Instant::now();
        self.toasts
            .retain(|t| now.duration_since(t.born).as_secs_f32() < t.ttl);
        if self.toasts.is_empty() {
            return;
        }
        egui::Area::new(Id::new("toasts"))
            .anchor(Align2::RIGHT_BOTTOM, [-14.0, -36.0])
            .show(ui.ctx(), |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                let msgs: Vec<String> = self
                    .toasts
                    .iter()
                    .rev()
                    .take(5)
                    .map(|t| t.msg.clone())
                    .collect();
                for msg in msgs {
                    egui::Frame::new()
                        .fill(ui.style().visuals.window_fill)
                        .stroke(ui.style().visuals.window_stroke)
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            ui.label(RichText::new(msg).size(12.5));
                        });
                }
            });
    }

    fn update_title(&mut self, ui: &mut Ui) {
        let title = match self.active_idx() {
            Some(idx) => {
                let tab = &self.tabs[idx];
                format!(
                    "{}{} - {}",
                    if tab.dirty { "● " } else { "" },
                    tab.display_path(),
                    APP_NAME
                )
            }
            None => format!("{} - Rust 记事本", APP_NAME),
        };
        if title != self.last_title {
            self.last_title = title.clone();
            ui.ctx().send_viewport_cmd(ViewportCommand::Title(title));
        }
    }
}

// ---------------------------------------------------------------------------
// 辅助
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum TabAction {
    Save,
    SaveAs,
    CloseOthers,
}

fn item(ui: &mut Ui, label: &str, shortcut: &str, enabled: bool) -> egui::Response {
    let btn = Button::new(label);
    let btn = if shortcut.is_empty() {
        btn
    } else {
        btn.shortcut_text(shortcut)
    };
    ui.add_enabled(enabled, btn)
}

fn t_title(tab: &Tab) -> &str {
    &tab.title
}

fn canon(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

fn sanitize_ranges(text: &str, ranges: &[Range<usize>]) -> Vec<Range<usize>> {
    ranges
        .iter()
        .filter_map(|r| {
            let s = char_boundary_down(text, r.start.min(text.len()));
            let e = char_boundary_up(text, r.end.min(text.len()));
            if e > s {
                Some(s..e)
            } else {
                None
            }
        })
        .collect()
}

fn char_boundary_down(text: &str, mut i: usize) -> usize {
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn char_boundary_up(text: &str, mut i: usize) -> usize {
    while i < text.len() && !text.is_char_boundary(i) {
        i += 1;
    }
    i
}

fn human_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

impl eframe::App for NotepadApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.main_ui(ui);
    }

    fn on_exit(&mut self) {
        self.sync_session();
        settings::save(&self.settings);
    }
}
