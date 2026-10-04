//! 查找/替换引擎:普通文本与正则两种模式,支持大小写、全字匹配、循环查找。

use regex::RegexBuilder;
use std::ops::Range;

#[derive(Clone, Debug, Default)]
pub struct FindParams {
    pub query: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
    pub loop_around: bool,
}

impl FindParams {
    pub fn key(&self) -> Option<(String, bool, bool, bool)> {
        if self.query.is_empty() {
            return None;
        }
        Some((
            self.query.clone(),
            self.case_sensitive,
            self.whole_word,
            self.regex,
        ))
    }
}

pub struct Matcher {
    re: regex::Regex,
}

impl Matcher {
    pub fn build(p: &FindParams) -> Result<Matcher, String> {
        if p.query.is_empty() {
            return Err("查找内容为空".to_string());
        }
        let pat = if p.regex {
            p.query.clone()
        } else {
            let esc = regex::escape(&p.query);
            if p.whole_word {
                format!(r"\b(?:{})\b", esc)
            } else {
                esc
            }
        };
        RegexBuilder::new(&pat)
            .case_insensitive(!p.case_sensitive)
            .multi_line(true)
            .size_limit(64 << 20)
            .build()
            .map(|re| Matcher { re })
            .map_err(|e| format!("正则表达式无效: {}", e))
    }

    /// 从 `from`(字节偏移)向后查找第一个匹配。
    pub fn find_next(&self, text: &str, from: usize) -> Option<Range<usize>> {
        let from = from.min(text.len());
        // 调整到字符边界
        let from = next_char_boundary(text, from);
        for m in self.re.find_iter(text) {
            if m.start() >= from {
                return Some(m.range());
            }
        }
        None
    }

    /// 向前查找 `before`(字节偏移)之前的最后一个匹配。
    pub fn find_prev(&self, text: &str, before: usize) -> Option<Range<usize>> {
        let before = next_char_boundary(text, before.min(text.len()));
        let mut found = None;
        for m in self.re.find_iter(text) {
            if m.end() <= before {
                found = Some(m.range());
            } else {
                break;
            }
        }
        found
    }

    pub fn count(&self, text: &str) -> usize {
        self.re.find_iter(text).count()
    }

    /// 列出全部匹配(上限 `cap`)。返回 (匹配区间, 是否被截断)。
    pub fn find_all(&self, text: &str, cap: usize) -> (Vec<Range<usize>>, bool) {
        let mut out = Vec::new();
        let mut truncated = false;
        for m in self.re.find_iter(text) {
            if out.len() >= cap {
                truncated = true;
                break;
            }
            out.push(m.range());
        }
        (out, truncated)
    }

    /// 对已匹配文本做单次替换(正则模式支持 $1 等分组引用)。
    pub fn replace_captures(&self, matched: &str, repl: &str) -> String {
        self.re.replace(matched, repl).into_owned()
    }

    /// 全部替换。正则模式下 `repl` 支持 `$1` 等分组引用;
    /// 文本模式下按字面量替换。返回 (新文本, 替换次数)。
    pub fn replace_all(&self, text: &str, repl: &str, literal_repl: bool) -> (String, usize) {
        let mut count = 0usize;
        if literal_repl {
            let mut out = String::with_capacity(text.len());
            let mut last = 0usize;
            for m in self.re.find_iter(text) {
                out.push_str(&text[last..m.start()]);
                out.push_str(repl);
                last = m.end();
                count += 1;
            }
            out.push_str(&text[last..]);
            (out, count)
        } else {
            let result = self
                .re
                .replace_all(text, repl)
                .into_owned();
            // 重新统计次数
            count = self.count(text);
            (result, count)
        }
    }
}

fn next_char_boundary(text: &str, mut i: usize) -> usize {
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}
