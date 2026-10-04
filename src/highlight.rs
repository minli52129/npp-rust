//! 轻量语法高亮:按语言配置扫描文本,产出字节区间的着色片段。
//! 支持:Rust / Python / JS / TS / C / C++ / Java / C# / Go / JSON /
//! XML / HTML / Markdown / Shell / SQL / YAML / TOML&INI / 纯文本。

use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Plain,
    Rust,
    Python,
    JavaScript,
    TypeScript,
    C,
    Cpp,
    Java,
    CSharp,
    Go,
    Json,
    Xml,
    Markdown,
    Shell,
    Sql,
    Yaml,
    TomlIni,
}

impl Lang {
    pub const ALL: [Lang; 17] = [
        Lang::Plain,
        Lang::Rust,
        Lang::Python,
        Lang::JavaScript,
        Lang::TypeScript,
        Lang::C,
        Lang::Cpp,
        Lang::Java,
        Lang::CSharp,
        Lang::Go,
        Lang::Json,
        Lang::Xml,
        Lang::Markdown,
        Lang::Shell,
        Lang::Sql,
        Lang::Yaml,
        Lang::TomlIni,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Lang::Plain => "纯文本",
            Lang::Rust => "Rust",
            Lang::Python => "Python",
            Lang::JavaScript => "JavaScript",
            Lang::TypeScript => "TypeScript",
            Lang::C => "C",
            Lang::Cpp => "C++",
            Lang::Java => "Java",
            Lang::CSharp => "C#",
            Lang::Go => "Go",
            Lang::Json => "JSON",
            Lang::Xml => "XML/HTML",
            Lang::Markdown => "Markdown",
            Lang::Shell => "Shell",
            Lang::Sql => "SQL",
            Lang::Yaml => "YAML",
            Lang::TomlIni => "TOML/INI",
        }
    }

    /// 按扩展名猜测语言。
    pub fn detect(path: &Path) -> Lang {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "rs" => Lang::Rust,
            "py" | "pyw" | "pyi" => Lang::Python,
            "js" | "mjs" | "cjs" | "jsx" => Lang::JavaScript,
            "ts" | "tsx" => Lang::TypeScript,
            "c" | "h" => Lang::C,
            "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" | "ipp" => Lang::Cpp,
            "java" => Lang::Java,
            "cs" => Lang::CSharp,
            "go" => Lang::Go,
            "json" | "jsonc" | "json5" => Lang::Json,
            "xml" | "html" | "htm" | "xhtml" | "svg" | "xaml" | "csproj" | "plist" => Lang::Xml,
            "md" | "markdown" => Lang::Markdown,
            "sh" | "bash" | "zsh" | "bashrc" | "zshrc" => Lang::Shell,
            "sql" => Lang::Sql,
            "yml" | "yaml" => Lang::Yaml,
            "toml" | "ini" | "cfg" | "conf" | "properties" | "gitconfig" => Lang::TomlIni,
            _ => Lang::Plain,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokKind {
    Norm,
    Keyword,
    Type,
    Fn,
    Num,
    Str,
    Comment,
    Punc,
    Tag,
    Heading,
}

/// 单个着色片段(字节区间,保证覆盖全文、互不重叠且有序)。
#[derive(Clone, Copy, Debug)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub kind: TokKind,
}

struct LangCfg {
    line_comments: &'static [&'static str],
    block_comment: Option<(&'static str, &'static str)>,
    strings: &'static [char],
    keywords: &'static [&'static str],
    types: &'static [&'static str],
    ci: bool,
    tagged: bool,
    /// 行首 `key:` / `key=` 样式键名(YAML/TOML/INI)
    key_value: bool,
    /// Markdown 风格:行首 `#` 到行尾整行着色
    heading_hash: bool,
}

const KW_EMPTY: &[&str] = &[];
const TYPES_EMPTY: &[&str] = &[];

const RUST_TYPES: &[&str] = &[
    "bool", "char", "str", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64",
    "i128", "isize", "f32", "f64", "String", "Vec", "Box", "Rc", "Arc", "Option", "Result",
    "HashMap", "HashSet", "BTreeMap", "BTreeSet", "Self",
];
const C_TYPES: &[&str] = &[
    "void", "char", "short", "int", "long", "float", "double", "unsigned", "signed", "size_t",
    "ssize_t", "uint8_t", "uint16_t", "uint32_t", "uint64_t", "int8_t", "int16_t", "int32_t",
    "int64_t", "bool", "FILE",
];
const JAVA_TYPES: &[&str] = &[
    "boolean", "byte", "short", "int", "long", "float", "double", "char", "void", "String",
    "Object", "Integer", "Long", "Double", "Float", "Boolean", "List", "Map", "Set",
];

fn cfg(lang: Lang) -> LangCfg {
    match lang {
        Lang::Plain => LangCfg {
            line_comments: &[],
            block_comment: None,
            strings: &[],
            keywords: KW_EMPTY,
            types: TYPES_EMPTY,
            ci: false,
            tagged: false,
            key_value: false,
            heading_hash: false,
        },
        Lang::Rust => LangCfg {
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            strings: &['"', '\''],
            keywords: &[
                "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else",
                "enum", "extern", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod",
                "move", "mut", "pub", "ref", "return", "static", "struct", "super", "trait",
                "type", "unsafe", "use", "where", "while", "true", "false", "self", "macro_rules",
            ],
            types: RUST_TYPES,
            ci: false,
            tagged: false,
            key_value: false,
            heading_hash: false,
        },
        Lang::Python => LangCfg {
            line_comments: &["#"],
            block_comment: Some(("\"\"\"", "\"\"\"")),
            strings: &['"', '\''],
            keywords: &[
                "and", "as", "assert", "async", "await", "break", "class", "continue", "def",
                "del", "elif", "else", "except", "finally", "for", "from", "global", "if",
                "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise",
                "return", "try", "while", "with", "yield", "True", "False", "None", "self",
            ],
            types: TYPES_EMPTY,
            ci: false,
            tagged: false,
            key_value: false,
            heading_hash: false,
        },
        Lang::JavaScript | Lang::TypeScript => LangCfg {
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            strings: &['"', '\'', '`'],
            keywords: &[
                "async", "await", "break", "case", "catch", "class", "const", "continue",
                "debugger", "default", "delete", "do", "else", "enum", "export", "extends",
                "false", "finally", "for", "function", "if", "implements", "import", "in",
                "instanceof", "interface", "let", "new", "null", "of", "private", "protected",
                "public", "readonly", "return", "satisfies", "static", "super", "switch", "this",
                "throw", "true", "try", "type", "typeof", "var", "void", "while", "yield", "as",
                "from", "keyof", "namespace", "declare", "abstract",
            ],
            types: &["string", "number", "boolean", "any", "unknown", "never", "undefined", "bigint", "symbol"],
            ci: false,
            tagged: false,
            key_value: false,
            heading_hash: false,
        },
        Lang::C => LangCfg {
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            strings: &['"', '\''],
            keywords: &[
                "auto", "break", "case", "const", "continue", "default", "do", "else", "enum",
                "extern", "for", "goto", "if", "inline", "register", "restrict", "return",
                "sizeof", "static", "struct", "switch", "typedef", "union", "volatile", "while",
                "NULL", "true", "false", "#include", "#define", "#ifdef", "#ifndef", "#endif",
                "#if", "#else", "#elif", "#pragma", "#undef",
            ],
            types: C_TYPES,
            ci: false,
            tagged: false,
            key_value: false,
            heading_hash: false,
        },
        Lang::Cpp => LangCfg {
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            strings: &['"', '\''],
            keywords: &[
                "alignas", "alignof", "auto", "break", "case", "catch", "class", "const",
                "constexpr", "const_cast", "continue", "decltype", "default", "delete", "do",
                "dynamic_cast", "else", "enum", "explicit", "export", "extern", "false", "final",
                "for", "friend", "goto", "if", "inline", "mutable", "namespace", "new", "noexcept",
                "nullptr", "operator", "override", "private", "protected", "public", "register",
                "reinterpret_cast", "return", "sizeof", "static", "static_assert",
                "static_cast", "struct", "switch", "template", "this", "throw", "true", "try",
                "typedef", "typeid", "typename", "union", "using", "virtual", "volatile",
                "while", "#include", "#define", "#ifdef", "#ifndef", "#endif", "#if", "#else",
                "#elif", "#pragma",
            ],
            types: C_TYPES,
            ci: false,
            tagged: false,
            key_value: false,
            heading_hash: false,
        },
        Lang::Java => LangCfg {
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            strings: &['"', '\''],
            keywords: &[
                "abstract", "assert", "boolean", "break", "byte", "case", "catch", "char",
                "class", "const", "continue", "default", "do", "double", "else", "enum",
                "extends", "final", "finally", "float", "for", "goto", "if", "implements",
                "import", "instanceof", "int", "interface", "long", "native", "new", "package",
                "private", "protected", "public", "return", "short", "static", "strictfp",
                "super", "switch", "synchronized", "this", "throw", "throws", "transient", "try",
                "void", "volatile", "while", "true", "false", "null", "var", "record", "yield",
            ],
            types: JAVA_TYPES,
            ci: false,
            tagged: false,
            key_value: false,
            heading_hash: false,
        },
        Lang::CSharp => LangCfg {
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            strings: &['"', '\''],
            keywords: &[
                "abstract", "as", "async", "await", "base", "bool", "break", "byte", "case",
                "catch", "char", "checked", "class", "const", "continue", "decimal", "default",
                "delegate", "do", "double", "dynamic", "else", "enum", "event", "explicit",
                "extern", "false", "finally", "fixed", "float", "for", "foreach", "get", "goto",
                "if", "implicit", "in", "init", "int", "interface", "internal", "is", "lock",
                "long", "namespace", "new", "null", "object", "operator", "out", "override",
                "params", "private", "protected", "public", "readonly", "record", "ref",
                "return", "sbyte", "sealed", "set", "short", "sizeof", "stackalloc", "static",
                "string", "struct", "switch", "this", "throw", "true", "try", "typeof", "uint",
                "ulong", "unchecked", "unsafe", "ushort", "using", "var", "virtual", "void",
                "volatile", "when", "where", "while", "yield",
            ],
            types: TYPES_EMPTY,
            ci: false,
            tagged: false,
            key_value: false,
            heading_hash: false,
        },
        Lang::Go => LangCfg {
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            strings: &['"', '\'', '`'],
            keywords: &[
                "break", "case", "chan", "const", "continue", "default", "defer", "else",
                "fallthrough", "for", "func", "go", "goto", "if", "import", "interface", "map",
                "package", "range", "return", "select", "struct", "switch", "type", "var",
                "nil", "true", "false", "iota",
            ],
            types: &[
                "bool", "byte", "complex64", "complex128", "error", "float32", "float64", "int",
                "int8", "int16", "int32", "int64", "rune", "string", "uint", "uint8", "uint16",
                "uint32", "uint64", "uintptr", "any",
            ],
            ci: false,
            tagged: false,
            key_value: false,
            heading_hash: false,
        },
        Lang::Json => LangCfg {
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            strings: &['"'],
            keywords: &["true", "false", "null"],
            types: TYPES_EMPTY,
            ci: false,
            tagged: false,
            key_value: false,
            heading_hash: false,
        },
        Lang::Xml => LangCfg {
            line_comments: &[],
            block_comment: Some(("<!--", "-->")),
            strings: &['"', '\''],
            keywords: KW_EMPTY,
            types: TYPES_EMPTY,
            ci: false,
            tagged: true,
            key_value: false,
            heading_hash: false,
        },
        Lang::Markdown => LangCfg {
            line_comments: &[],
            block_comment: None,
            strings: &['`', '"'],
            keywords: KW_EMPTY,
            types: TYPES_EMPTY,
            ci: false,
            tagged: false,
            key_value: false,
            heading_hash: true,
        },
        Lang::Shell => LangCfg {
            line_comments: &["#"],
            block_comment: None,
            strings: &['"', '\''],
            keywords: &[
                "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done",
                "case", "esac", "in", "function", "return", "exit", "local", "export", "set",
                "unset", "shift", "break", "continue", "source", "alias", "readonly", "true",
                "false", "echo", "cd", "printf",
            ],
            types: TYPES_EMPTY,
            ci: false,
            tagged: false,
            key_value: false,
            heading_hash: false,
        },
        Lang::Sql => LangCfg {
            line_comments: &["--", "#"],
            block_comment: Some(("/*", "*/")),
            strings: &['\''],
            keywords: &[
                "SELECT", "FROM", "WHERE", "INSERT", "INTO", "VALUES", "UPDATE", "SET", "DELETE",
                "CREATE", "TABLE", "DATABASE", "SCHEMA", "DROP", "ALTER", "ADD", "COLUMN",
                "PRIMARY", "KEY", "FOREIGN", "REFERENCES", "INDEX", "VIEW", "TRIGGER",
                "PROCEDURE", "JOIN", "INNER", "LEFT", "RIGHT", "FULL", "OUTER", "CROSS", "ON",
                "AS", "AND", "OR", "NOT", "NULL", "IS", "IN", "BETWEEN", "LIKE", "EXISTS",
                "GROUP", "BY", "ORDER", "HAVING", "LIMIT", "OFFSET", "UNION", "ALL", "DISTINCT",
                "CASE", "WHEN", "THEN", "ELSE", "END", "IF", "BEGIN", "COMMIT", "ROLLBACK",
                "TRANSACTION", "DEFAULT", "UNIQUE", "AUTO_INCREMENT", "WITH", "RECURSIVE",
                "CAST", "ASC", "DESC",
            ],
            types: &[
                "INT", "INTEGER", "BIGINT", "SMALLINT", "TINYINT", "FLOAT", "DOUBLE", "REAL",
                "DECIMAL", "NUMERIC", "CHAR", "VARCHAR", "TEXT", "DATE", "TIME", "DATETIME",
                "TIMESTAMP", "BOOLEAN", "BLOB", "JSON",
            ],
            ci: true,
            tagged: false,
            key_value: false,
            heading_hash: false,
        },
        Lang::Yaml => LangCfg {
            line_comments: &["#"],
            block_comment: None,
            strings: &['"', '\''],
            keywords: &["true", "false", "null", "yes", "no", "on", "off"],
            types: TYPES_EMPTY,
            ci: false,
            tagged: false,
            key_value: true,
            heading_hash: false,
        },
        Lang::TomlIni => LangCfg {
            line_comments: &["#", ";"],
            block_comment: None,
            strings: &['"', '\''],
            keywords: &["true", "false"],
            types: TYPES_EMPTY,
            ci: false,
            tagged: false,
            key_value: true,
            heading_hash: false,
        },
    }
}

fn is_id_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '$'
}

fn is_id_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// 把全文扫描为着色片段。保证片段有序、互不重叠、完整覆盖 [0, len)。
pub fn tokenize(text: &str, lang: Lang) -> Vec<Span> {
    let c = cfg(lang);
    let bytes = text.as_bytes();
    let n = bytes.len();
    let mut spans: Vec<Span> = Vec::new();
    let mut in_block_comment = false;
    let mut i = 0usize;

    // 片段收集辅助:若与前一片段同类则合并,避免碎片过多
    macro_rules! push {
        ($start:expr, $end:expr, $kind:expr) => {
            let (s, e, k) = ($start, $end, $kind);
            if e > s {
                if let Some(last) = spans.last_mut() {
                    if last.kind == k && last.end == s {
                        last.end = e;
                    } else {
                        spans.push(Span { start: s, end: e, kind: k });
                    }
                } else {
                    spans.push(Span { start: s, end: e, kind: k });
                }
            }
        };
    }

    while i < n {
        // 块注释延续
        if in_block_comment {
            if let Some((_, be)) = c.block_comment {
                match text[i..].find(be) {
                    Some(p) => {
                        let end = i + p + be.len();
                        push!(i, end, TokKind::Comment);
                        i = end;
                        in_block_comment = false;
                    }
                    None => {
                        push!(i, n, TokKind::Comment);
                        i = n;
                    }
                }
                continue;
            } else {
                in_block_comment = false;
            }
        }

        let ch = text[i..].chars().next().unwrap();

        // Markdown 行首标题
        if c.heading_hash && ch == '#' && (i == 0 || bytes[i - 1] == b'\n') {
            let end = text[i..].find('\n').map_or(n, |p| i + p);
            push!(i, end, TokKind::Heading);
            i = end;
            continue;
        }

        // YAML/TOML/INI 行首键名 `key:` 或 `key=`
        if c.key_value && (i == 0 || bytes[i - 1] == b'\n') {
            let mut j = i;
            let mut is_key = false;
            while j < n {
                let cj = text[j..].chars().next().unwrap();
                if is_id_continue(cj) || cj == ' ' || cj == '\t' {
                    j += cj.len_utf8();
                } else if cj == ':' || cj == '=' {
                    // 键值分隔符之后是值,着色键名部分(含到冒号)
                    is_key = true;
                    break;
                } else {
                    break;
                }
            }
            if is_key {
                let end = (j + 1).min(n);
                push!(i, end, TokKind::Keyword);
                i = end;
                continue;
            }
        }

        // 行注释
        if !c.line_comments.is_empty()
            && c.line_comments.iter().any(|p| text[i..].starts_with(p))
        {
            let end = text[i..].find('\n').map_or(n, |p| i + p);
            push!(i, end, TokKind::Comment);
            i = end;
            continue;
        }

        // 块注释开始
        if let Some((bs, be)) = c.block_comment {
            if text[i..].starts_with(bs) {
                match text[i + bs.len()..].find(be) {
                    Some(p) => {
                        let end = i + bs.len() + p + be.len();
                        push!(i, end, TokKind::Comment);
                        i = end;
                    }
                    None => {
                        in_block_comment = true;
                        push!(i, n, TokKind::Comment);
                        i = n;
                    }
                }
                continue;
            }
        }

        // 字符串
        if c.strings.contains(&ch) {
            let quote = ch;
            let mut j = i + quote.len_utf8();
            let mut closed = false;
            while j < n {
                let cj = text[j..].chars().next().unwrap();
                if cj == '\\' && quote != '`' {
                    j += cj.len_utf8();
                    if j < n {
                        j += text[j..].chars().next().unwrap().len_utf8();
                    }
                    continue;
                }
                if cj == quote {
                    j += cj.len_utf8();
                    closed = true;
                    break;
                }
                if cj == '\n' {
                    break; // 字符串不跨行(反引号模板串除外)
                }
                j += cj.len_utf8();
            }
            if quote == '`' && !closed {
                // 模板字符串允许跨行:扫到下一个反引号或文末
                j = i + 1;
                while j < n {
                    let cj = text[j..].chars().next().unwrap();
                    if cj == '\\' {
                        j += 1;
                        if j < n {
                            j += text[j..].chars().next().unwrap().len_utf8();
                        }
                        continue;
                    }
                    if cj == '`' {
                        j += 1;
                        break;
                    }
                    j += cj.len_utf8();
                }
            }
            push!(i, j.min(n), TokKind::Str);
            i = j.min(n);
            continue;
        }

        // 标识符 / 关键字
        if is_id_start(ch) {
            let mut j = i + ch.len_utf8();
            while j < n {
                let cj = text[j..].chars().next().unwrap();
                if is_id_continue(cj) {
                    j += cj.len_utf8();
                } else {
                    break;
                }
            }
            let word = &text[i..j];
            let kind = if word_starts_keyword(word, &c) {
                TokKind::Keyword
            } else if c.types.contains(&word) {
                TokKind::Type
            } else {
                // 后面跟 `(` 视为函数调用/声明
                let mut k = j;
                while k < n {
                    let ck = text[k..].chars().next().unwrap();
                    if ck == ' ' || ck == '\t' {
                        k += ck.len_utf8();
                    } else {
                        break;
                    }
                }
                if k < n && text[k..].starts_with('(') {
                    TokKind::Fn
                } else {
                    TokKind::Norm
                }
            };
            push!(i, j, kind);
            i = j;
            continue;
        }

        // 数字
        if ch.is_ascii_digit() {
            let mut j = i + 1;
            while j < n {
                let cj = text[j..].chars().next().unwrap();
                if cj.is_ascii_alphanumeric() || cj == '.' || cj == '_' {
                    j += 1;
                } else {
                    break;
                }
            }
            push!(i, j, TokKind::Num);
            i = j;
            continue;
        }

        // XML 标签名
        if c.tagged && ch == '<' && i + 1 < n {
            let nc = text[i + 1..].chars().next().unwrap();
            if is_id_start(nc) || nc == '/' {
                let mut j = i + 1;
                if text[j..].starts_with('/') {
                    j += 1;
                }
                let name_start = j;
                while j < n {
                    let cj = text[j..].chars().next().unwrap();
                    if is_id_continue(cj) || cj == '-' || cj == ':' {
                        j += cj.len_utf8();
                    } else {
                        break;
                    }
                }
                if j > name_start {
                    push!(i, name_start, TokKind::Punc);
                    push!(name_start, j, TokKind::Tag);
                    i = j;
                    continue;
                }
            }
        }

        // 空白
        if ch.is_whitespace() {
            let mut j = i + ch.len_utf8();
            while j < n {
                let cj = text[j..].chars().next().unwrap();
                if cj.is_whitespace() {
                    j += cj.len_utf8();
                } else {
                    break;
                }
            }
            push!(i, j, TokKind::Norm);
            i = j;
            continue;
        }

        // 其余:标点/运算符连续段
        let mut j = i + ch.len_utf8();
        while j < n {
            let cj = text[j..].chars().next().unwrap();
            if cj.is_whitespace()
                || is_id_start(cj)
                || cj.is_ascii_digit()
                || c.strings.contains(&cj)
            {
                break;
            }
            j += cj.len_utf8();
        }
        push!(i, j, TokKind::Punc);
        i = j;
    }

    // 兜底:确保完整覆盖
    if spans.is_empty() && n > 0 {
        spans.push(Span { start: 0, end: n, kind: TokKind::Norm });
    }
    spans
}

fn word_starts_keyword(word: &str, c: &LangCfg) -> bool {
    if c.ci {
        c.keywords.iter().any(|k| k.eq_ignore_ascii_case(word))
    } else {
        c.keywords.contains(&word)
    }
}

/// 片段对应的主题色。
pub fn kind_color(kind: TokKind, dark: bool) -> [u8; 3] {
    if dark {
        match kind {
            TokKind::Norm => [0xD4, 0xD4, 0xD4],
            TokKind::Keyword => [0xC5, 0x86, 0xC0],
            TokKind::Type => [0x4E, 0xC9, 0xB0],
            TokKind::Fn => [0xDC, 0xDC, 0xAA],
            TokKind::Num => [0xB5, 0xCE, 0xA8],
            TokKind::Str => [0xCE, 0x91, 0x78],
            TokKind::Comment => [0x6A, 0x99, 0x55],
            TokKind::Punc => [0xA0, 0xA0, 0xA0],
            TokKind::Tag => [0x56, 0x9C, 0xD6],
            TokKind::Heading => [0x56, 0x9C, 0xD6],
        }
    } else {
        match kind {
            TokKind::Norm => [0x1F, 0x1F, 0x1F],
            TokKind::Keyword => [0xAF, 0x00, 0xDB],
            TokKind::Type => [0x26, 0x7F, 0x99],
            TokKind::Fn => [0x79, 0x5E, 0x26],
            TokKind::Num => [0x09, 0x86, 0x58],
            TokKind::Str => [0xA3, 0x15, 0x15],
            TokKind::Comment => [0x00, 0x80, 0x00],
            TokKind::Punc => [0x5F, 0x5F, 0x5F],
            TokKind::Tag => [0x80, 0x00, 0x00],
            TokKind::Heading => [0x04, 0x51, 0xA5],
        }
    }
}
