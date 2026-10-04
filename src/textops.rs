//! 文本变换:行操作、大小写转换、字符/字节索引转换。

use std::ops::Range;

// ---------------------------------------------------------------------------
// 索引转换(egui 光标使用「字符索引」,正则使用「字节偏移」)
// ---------------------------------------------------------------------------

pub fn char_to_byte(text: &str, char_idx: usize) -> usize {
    text.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(text.len())
}

pub fn byte_to_char(text: &str, byte_idx: usize) -> usize {
    text[..byte_idx.min(text.len())].chars().count()
}

/// 字符索引所在行的 (行号, 列号),均从 1 开始。
pub fn line_col(text: &str, char_idx: usize) -> (usize, usize) {
    let b = char_to_byte(text, char_idx);
    let before = &text[..b];
    let line = before.bytes().filter(|&c| c == b'\n').count() + 1;
    let col = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
    (line, col)
}

/// 第 `line`(1 基)行的字符索引区间 [行首, 行尾)。
pub fn line_char_range(text: &str, line: usize) -> Option<(usize, usize)> {
    if line == 0 {
        return None;
    }
    let mut byte_start = 0usize;
    for (idx, part) in text.split('\n').enumerate() {
        if idx + 1 == line {
            return Some((
                byte_to_char(text, byte_start),
                byte_to_char(text, byte_start + part.len()),
            ));
        }
        byte_start += part.len() + 1;
    }
    None
}

// ---------------------------------------------------------------------------
// 行操作
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineOp {
    /// 复制行(当前行或选区行,插入到下方)
    Duplicate,
    /// 删除行
    Delete,
    /// 当前行/选区整体上移一行
    MoveUp,
    /// 当前行/选区整体下移一行
    MoveDown,
    SortAsc,
    SortDesc,
    RemoveDup,
    RemoveEmpty,
    RemoveBlank,
    TrimTrailing,
    TrimLeading,
    Join,
    Reverse,
}

/// 应用行操作。
/// - `sel`:选区字节区间(跨多行时扩展到整行);
/// - `cursor_char`:光标字符位置(行级操作无选区时取所在行);
/// - 排序/去重/空白处理等文档级操作无选区时作用于全文。
///
/// 返回 (新文本, 建议的光标字符位置)。
pub fn apply_line_op(
    text: &str,
    sel: Option<Range<usize>>,
    cursor_char: usize,
    op: LineOp,
) -> (String, usize) {
    let line_targeted = matches!(op, LineOp::Duplicate | LineOp::Delete | LineOp::MoveUp | LineOp::MoveDown);

    let mut lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
    let total = lines.len();
    // 末尾空行(文本以 \n 结尾时 split 产生)不属于可操作行
    let real = if total > 0 && lines[total - 1].is_empty() && text.ends_with('\n') {
        total - 1
    } else {
        total
    };

    // 选区是否跨多行
    let multi_line_sel = match &sel {
        Some(r) if r.start < r.end => {
            let s = r.start.min(text.len());
            let e = r.end.min(text.len());
            s < e && text[s..e].contains('\n')
        }
        _ => false,
    };

    // 计算受影响行区间 [l0, l1)(0 基,半开)
    let (l0, l1) = if multi_line_sel {
        let r = sel.unwrap();
        let start_line = text[..r.start.min(text.len())].bytes().filter(|&c| c == b'\n').count();
        let end_line = text[..r.end.min(text.len())].bytes().filter(|&c| c == b'\n').count();
        (start_line.min(real), (end_line + 1).min(real))
    } else if line_targeted {
        let (cl, _) = line_col(text, cursor_char);
        let cl = (cl - 1).clamp(0, real.saturating_sub(1));
        (cl, cl + 1)
    } else {
        (0, real)
    };

    // 文档级操作直接在行向量上进行;记录结果光标行
    let mut new_cursor_line = l0;

    match op {
        LineOp::Duplicate => {
            let copy: Vec<String> = lines[l0..l1].to_vec();
            let after = lines.split_off(l1);
            lines.extend(copy);
            lines.extend(after);
            new_cursor_line = l1; // 光标放到复制出的第一行
        }
        LineOp::Delete => {
            lines.drain(l0..l1);
            if lines.is_empty() {
                lines.push(String::new());
            }
        }
        LineOp::MoveUp => {
            if l0 > 0 {
                let prev = lines.remove(l0 - 1);
                lines.insert(l1 - 1, prev);
                new_cursor_line = l0 - 1;
            }
        }
        LineOp::MoveDown => {
            if l1 < real {
                let next = lines.remove(l1);
                lines.insert(l0, next);
                new_cursor_line = l0 + 1;
            }
        }
        LineOp::SortAsc => lines[l0..l1].sort(),
        LineOp::SortDesc => lines[l0..l1].sort_by(|a, b| b.cmp(a)),
        LineOp::RemoveDup => {
            let mut seen = std::collections::HashSet::new();
            let mut write = l0;
            for read in l0..l1 {
                if seen.insert(lines[read].clone()) {
                    lines.swap(read, write);
                    write += 1;
                }
            }
            lines.drain(write..l1);
        }
        LineOp::RemoveEmpty => {
            let mut write = l0;
            for read in l0..l1 {
                if !lines[read].is_empty() {
                    lines.swap(read, write);
                    write += 1;
                }
            }
            lines.drain(write..l1);
        }
        LineOp::RemoveBlank => {
            let mut write = l0;
            for read in l0..l1 {
                if !lines[read].trim().is_empty() {
                    lines.swap(read, write);
                    write += 1;
                }
            }
            lines.drain(write..l1);
        }
        LineOp::TrimTrailing => {
            for l in lines[l0..l1].iter_mut() {
                *l = l.trim_end().to_string();
            }
        }
        LineOp::TrimLeading => {
            for l in lines[l0..l1].iter_mut() {
                *l = l.trim_start().to_string();
            }
        }
        LineOp::Join => {
            let joined = lines[l0..l1].join(" ");
            lines.drain(l0..l1);
            lines.insert(l0, joined);
        }
        LineOp::Reverse => lines[l0..l1].reverse(),
    }

    let new_text = lines.join("\n");
    // 新光标:目标行行首(字符索引)
    let mut byte_start = 0usize;
    for (idx, part) in new_text.split('\n').enumerate() {
        if idx == new_cursor_line {
            let cursor = byte_to_char(&new_text, byte_start);
            return (new_text, cursor);
        }
        byte_start += part.len() + 1;
    }
    (new_text, 0)
}

fn splice(text: &str, start: usize, end: usize, repl: &str) -> String {
    let mut out = String::with_capacity(text.len() + repl.len());
    out.push_str(&text[..start.min(text.len())]);
    out.push_str(repl);
    out.push_str(&text[end.min(text.len())..]);
    out
}

// ---------------------------------------------------------------------------
// 大小写转换(有选区作用于选区,否则作用于整个文档)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub enum CaseOp {
    Upper,
    Lower,
    Swap,
    Title,
}

pub fn apply_case(text: &str, sel: Option<Range<usize>>, op: CaseOp) -> (String, usize) {
    let (region, region_start) = match sel {
        Some(r) if r.start < r.end => (text[r.start.min(text.len())..r.end.min(text.len())].to_string(), r.start),
        _ => (text.to_string(), 0),
    };
    let converted = match op {
        CaseOp::Upper => region.to_uppercase(),
        CaseOp::Lower => region.to_lowercase(),
        CaseOp::Swap => region
            .chars()
            .map(|c| {
                if c.is_uppercase() {
                    c.to_lowercase().next().unwrap_or(c)
                } else {
                    c.to_uppercase().next().unwrap_or(c)
                }
            })
            .collect(),
        CaseOp::Title => {
            let mut out = String::with_capacity(region.len());
            let mut at_word_start = true;
            for c in region.chars() {
                if c.is_alphanumeric() {
                    if at_word_start {
                        out.extend(c.to_uppercase());
                        at_word_start = false;
                    } else {
                        out.extend(c.to_lowercase());
                    }
                } else {
                    out.push(c);
                    at_word_start = true;
                }
            }
            out
        }
    };
    let new_text = splice(text, region_start, region_start + region.len(), &converted);
    (new_text, byte_to_char(text, region_start))
}
