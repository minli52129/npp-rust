//! 标签页(文档)数据模型。

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::files::{self, Eol, FileEncoding};
use crate::highlight::Lang;

#[derive(Clone)]
pub struct Tab {
    /// 关联的磁盘文件;`None` 表示未保存的新建文档
    pub path: Option<PathBuf>,
    /// 标签栏显示的标题(文件名或「新建 N」)
    pub title: String,
    /// 文本内容(内部统一 `\n` 换行)
    pub text: String,
    pub dirty: bool,
    pub encoding: FileEncoding,
    pub eol: Eol,
    pub lang: Lang,
    pub readonly: bool,
    pub mtime: Option<SystemTime>,
    /// 主光标位置(字符索引,每帧由编辑器回写)
    pub cursor_char: usize,
    /// 选区长度(字符数)
    pub sel_chars: usize,
    /// 当前选区(字符区间,每帧由编辑器回写;行操作/大小写转换用)
    pub sel_char: Option<(usize, usize)>,
    /// 待跳转目标(字符区间),由查找/结果面板/转到行设置,编辑器消费
    pub jump_to: Option<(usize, usize)>,
    /// 供 egui 控件用的唯一 id
    pub uid: u64,
}

impl Tab {
    pub fn new_untitled(n: usize, uid: u64) -> Self {
        Tab {
            path: None,
            title: format!("新建 {}", n),
            text: String::new(),
            dirty: false,
            encoding: FileEncoding::Utf8,
            eol: Eol::Lf,
            lang: Lang::Plain,
            readonly: false,
            mtime: None,
            cursor_char: 0,
            sel_chars: 0,
            sel_char: None,
            jump_to: None,
            uid,
        }
    }

    pub fn from_path(path: &Path, uid: u64) -> Result<Tab, String> {
        let lf = files::load(path)?;
        let title = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        Ok(Tab {
            path: Some(path.to_path_buf()),
            title,
            text: lf.text,
            dirty: false,
            encoding: lf.encoding,
            eol: lf.eol,
            lang: Lang::detect(path),
            readonly: false,
            mtime: lf.mtime,
            cursor_char: 0,
            sel_chars: 0,
            sel_char: None,
            jump_to: None,
            uid,
        })
    }

    /// 窗口标题用:已保存显示完整路径,未保存显示标题。
    pub fn display_path(&self) -> String {
        match &self.path {
            Some(p) => p.display().to_string(),
            None => self.title.clone(),
        }
    }

    /// 行数(按 `\n` 切分,与行号栏一致)。
    pub fn line_count(&self) -> usize {
        self.text.split('\n').count()
    }
}
