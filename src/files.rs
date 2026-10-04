//! 文件读写:编码识别/转换、换行符(CRLF/LF/CR)处理。

use std::fs;
use std::path::Path;
use std::time::SystemTime;

/// 文本文件编码。内部文本始终为 UTF-8 `String`,仅在读写磁盘时转换。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileEncoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    /// 中文 Windows 的 ANSI 默认编码(GBK 码表,读取时用 GB18030 超集解码)
    Gbk,
    Big5,
    /// 西文 ANSI(Windows-1252)
    Western,
}

impl FileEncoding {
    pub const ALL: [FileEncoding; 7] = [
        FileEncoding::Utf8,
        FileEncoding::Utf8Bom,
        FileEncoding::Utf16Le,
        FileEncoding::Utf16Be,
        FileEncoding::Gbk,
        FileEncoding::Big5,
        FileEncoding::Western,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FileEncoding::Utf8 => "UTF-8",
            FileEncoding::Utf8Bom => "UTF-8 BOM",
            FileEncoding::Utf16Le => "UTF-16 LE",
            FileEncoding::Utf16Be => "UTF-16 BE",
            FileEncoding::Gbk => "ANSI/GBK",
            FileEncoding::Big5 => "Big5",
            FileEncoding::Western => "ANSI/CP1252",
        }
    }
}

/// 换行符。内部文本统一用 `\n` 存储,保存时转换。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Eol {
    Crlf,
    #[default]
    Lf,
    Cr,
}

impl Eol {
    pub const ALL: [Eol; 3] = [Eol::Crlf, Eol::Lf, Eol::Cr];

    pub fn label(self) -> &'static str {
        match self {
            Eol::Crlf => "Windows (CR LF)",
            Eol::Lf => "Unix (LF)",
            Eol::Cr => "Mac (CR)",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Eol::Crlf => "CRLF",
            Eol::Lf => "LF",
            Eol::Cr => "CR",
        }
    }
}

/// 检测换行符并把文本规范化为内部表示(全部 `\n`)。
pub fn detect_eol_and_normalize(raw: &str) -> (Eol, String) {
    let eol = if raw.contains("\r\n") {
        Eol::Crlf
    } else if raw.contains('\r') {
        Eol::Cr
    } else {
        Eol::Lf
    };
    let mut norm = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            norm.push('\n');
        } else {
            norm.push(c);
        }
    }
    (eol, norm)
}

/// 把内部文本(`\n`)转换为目标换行符表示。
pub fn apply_eol(text: &str, eol: Eol) -> String {
    match eol {
        Eol::Lf => text.to_string(),
        Eol::Crlf => text.replace('\n', "\r\n"),
        Eol::Cr => text.replace('\n', "\r"),
    }
}

/// 依据 BOM / UTF-8 校验 / ANSI 猜测识别编码并解码。
/// 无法表示的字符以 U+FFFD 替换(与 Notepad++ 的处理方式类似)。
pub fn decode(bytes: &[u8]) -> (FileEncoding, String) {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return (
            FileEncoding::Utf8Bom,
            String::from_utf8_lossy(&bytes[3..]).into_owned(),
        );
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let (s, _, _) = encoding_rs::UTF_16LE.decode(bytes);
        return (FileEncoding::Utf16Le, s.into_owned());
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (s, _, _) = encoding_rs::UTF_16BE.decode(bytes);
        return (FileEncoding::Utf16Be, s.into_owned());
    }
    if std::str::from_utf8(bytes).is_ok() {
        return (FileEncoding::Utf8, String::from_utf8_lossy(bytes).into_owned());
    }
    // 非 UTF-8:按中文环境惯例优先猜 GBK(用 GB18030 解码,GBK 超集)
    let (s, _, _) = encoding_rs::GB18030.decode(bytes);
    (FileEncoding::Gbk, s.into_owned())
}

/// 用指定编码解码(「以编码重新加载」用)。返回 (解码文本, 是否有替换字符)。
pub fn decode_as(bytes: &[u8], enc: FileEncoding) -> (String, bool) {
    let (cow, _, had_errors) = match enc {
        FileEncoding::Utf8 | FileEncoding::Utf8Bom => {
            let (s, _, h) = encoding_rs::UTF_8.decode(bytes);
            (s, true, h)
        }
        FileEncoding::Utf16Le => {
            let (s, _, h) = encoding_rs::UTF_16LE.decode(bytes);
            (s, true, h)
        }
        FileEncoding::Utf16Be => {
            let (s, _, h) = encoding_rs::UTF_16BE.decode(bytes);
            (s, true, h)
        }
        FileEncoding::Gbk => {
            let (s, _, h) = encoding_rs::GB18030.decode(bytes);
            (s, true, h)
        }
        FileEncoding::Big5 => {
            let (s, _, h) = encoding_rs::BIG5.decode(bytes);
            (s, true, h)
        }
        FileEncoding::Western => {
            let (s, _, h) = encoding_rs::WINDOWS_1252.decode(bytes);
            (s, true, h)
        }
    };
    let mut text = cow.into_owned();
    if enc == FileEncoding::Utf8Bom && text.starts_with('\u{FEFF}') {
        text.remove(0);
    }
    (text, had_errors)
}

/// 编码为字节序列。返回 (字节, 是否存在无法表示的字符)。
pub fn encode(text: &str, enc: FileEncoding) -> (Vec<u8>, bool) {
    match enc {
        FileEncoding::Utf8 => (text.as_bytes().to_vec(), false),
        FileEncoding::Utf8Bom => {
            let mut v = Vec::with_capacity(text.len() + 3);
            v.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
            v.extend_from_slice(text.as_bytes());
            (v, false)
        }
        FileEncoding::Utf16Le => {
            let (cow, _, h) = encoding_rs::UTF_16LE.encode(text);
            (cow.into_owned(), h)
        }
        FileEncoding::Utf16Be => {
            let (cow, _, h) = encoding_rs::UTF_16BE.encode(text);
            (cow.into_owned(), h)
        }
        FileEncoding::Gbk => {
            let (cow, _, h) = encoding_rs::GBK.encode(text);
            (cow.into_owned(), h)
        }
        FileEncoding::Big5 => {
            let (cow, _, h) = encoding_rs::BIG5.encode(text);
            (cow.into_owned(), h)
        }
        FileEncoding::Western => {
            let (cow, _, h) = encoding_rs::WINDOWS_1252.encode(text);
            (cow.into_owned(), h)
        }
    }
}

pub struct LoadedFile {
    pub text: String,
    pub encoding: FileEncoding,
    pub eol: Eol,
    pub mtime: Option<SystemTime>,
}

/// 读取并解码文件。
pub fn load(path: &Path) -> Result<LoadedFile, String> {
    let bytes = fs::read(path).map_err(|e| format!("无法读取 {}: {}", path.display(), e))?;
    let mtime = fs::metadata(path).and_then(|m| m.modified()).ok();
    let (encoding, raw) = decode(&bytes);
    let (eol, text) = detect_eol_and_normalize(&raw);
    Ok(LoadedFile {
        text,
        encoding,
        eol,
        mtime,
    })
}

/// 保存文件(按指定编码与换行符写出)。
pub fn save(path: &Path, text: &str, eol: Eol, enc: FileEncoding) -> Result<bool, String> {
    let converted = apply_eol(text, eol);
    let (bytes, had_errors) = encode(&converted, enc);
    fs::write(path, bytes).map_err(|e| format!("无法写入 {}: {}", path.display(), e))?;
    Ok(had_errors)
}

/// 重新用某编码解读磁盘上的文件(不改盘上的字节)。
pub fn reload_with(path: &Path, enc: FileEncoding) -> Result<(Eol, String), String> {
    let bytes = fs::read(path).map_err(|e| format!("无法读取 {}: {}", path.display(), e))?;
    let (raw, _) = decode_as(&bytes, enc);
    let (eol, text) = detect_eol_and_normalize(&raw);
    Ok((eol, text))
}
