//! 编辑器组件:行号栏 + 语法高亮 + 查找高亮 + 自动缩进 + 跳转定位。
//!
//! 结构:外层 ScrollArea 负责滚动,内部并排放置「行号列(纯绘制)」与
//! 多行 TextEdit。TextEdit 会随内容增长到完整高度,行号与文本由同一份
//! galley 的行信息精确对齐(支持自动换行)。

use std::ops::Range;
use std::sync::Arc;

use eframe::egui;
use egui::{
    text::{CharIndex, CCursor, CCursorRange, Galley, LayoutJob, TextFormat, TextWrapping},
    widgets::text_edit::TextEditState,
    Align2, Color32, Event, FontId, Id, Key, KeyboardShortcut, Modifiers, Pos2, Rect, Sense,
    Stroke, TextBuffer, TextEdit, Ui, vec2,
};

use crate::highlight::{self, Lang, TokKind};
use crate::tab::Tab;
use crate::textops;

/// 字符数(供边界钳制)。
pub fn char_len(text: &str) -> usize {
    text.chars().count()
}

/// 编辑器内部文本缓冲(包装 String,支持只读)。
pub struct EditorBuffer<'a> {
    s: &'a mut String,
    mutable: bool,
}

impl<'a> EditorBuffer<'a> {
    pub fn new(s: &'a mut String, mutable: bool) -> Self {
        EditorBuffer { s, mutable }
    }
}

impl TextBuffer for EditorBuffer<'_> {
    fn is_mutable(&self) -> bool {
        self.mutable
    }
    fn as_str(&self) -> &str {
        self.s.as_str()
    }
    fn type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<EditorBuffer<'static>>()
    }
    fn insert_text(&mut self, text: &str, char_index: CharIndex) -> usize {
        if !self.mutable {
            return 0;
        }
        self.s.insert_text(text, char_index)
    }
    fn delete_char_range(&mut self, char_range: Range<CharIndex>) {
        if !self.mutable {
            return;
        }
        self.s.delete_char_range(char_range);
    }
}

/// 编辑器 UI 缓存(跨帧,仅活动标签页使用)。
#[derive(Default)]
pub struct EditorUi {
    /// 上次排版过的文本(判断是否需要重新着色)
    cache_text: String,
    spans: Vec<highlight::Span>,
    /// 上帧编辑器完整高度(行号栏占位用)
    content_height: f32,
}

/// 编辑器参数。
pub struct EditorOpts<'a> {
    pub wrap: bool,
    pub font_size: f32,
    pub dark: bool,
    /// 是否做语法着色
    pub highlight: bool,
    /// 查找匹配区间(字节偏移,升序,已对齐字符边界)
    pub matches: &'a [Range<usize>],
    /// 当前定位到的匹配(更醒目的高亮)
    pub current_match: Option<Range<usize>>,
    pub cache: &'a mut EditorUi,
}

pub struct EditorOut {
    pub text_changed: bool,
}

pub fn editor_id(uid: u64) -> Id {
    Id::new(("npp_editor", uid))
}

/// 显示一个标签页的编辑器(在 ScrollArea 内容中调用)。
pub fn show_editor(ui: &mut Ui, tab: &mut Tab, opts: EditorOpts<'_>) -> EditorOut {
    let EditorOpts {
        wrap,
        font_size,
        dark,
        highlight,
        matches,
        current_match,
        cache,
    } = opts;

    let font_id = FontId::monospace(font_size);
    let line_h = ui.fonts_mut(|f| f.row_height(&font_id)) + ui.spacing().extra_text_line_spacing;

    // 行号栏宽度
    let line_count = tab.text.split('\n').count().max(1);
    let digits = line_count.to_string().len().max(2);
    let num_font = FontId::monospace(font_size * 0.82);
    let num_char_w = ui
        .fonts_mut(|f| {
            f.layout_no_wrap("0".repeat(digits), num_font.clone(), Color32::WHITE)
                .size()
                .x
        })
        / digits as f32;
    let gutter_w = digits as f32 * num_char_w + 16.0;

    let id = editor_id(tab.uid);

    // 消费待跳转目标:写入 TextEditState 光标,egui 自动滚动到该处
    if let Some((s, e)) = tab.jump_to {
        if let Some(mut state) = TextEditState::load(ui.ctx(), id) {
            state.cursor.set_char_range(Some(CCursorRange {
                primary: CCursor::new(CharIndex(e)),
                secondary: CCursor::new(CharIndex(s)),
                h_pos: None,
            }));
            state.store(ui.ctx(), id);
            tab.jump_to = None;
            ui.ctx().request_repaint();
        }
        // TextEditState 尚未建立(首次显示)则下一帧重试
    }

    let gutter_h = cache.content_height.max(line_h * 2.0);

    let out = ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
        // 行号栏占位
        let (gutter_rect, _) = ui.allocate_exact_size(vec2(gutter_w, gutter_h), Sense::hover());

        // 换行宽度(无换行 = 无限宽,交由外部横向滚动)
        let wrap_width = if wrap {
            (ui.available_width() - 2.0).max(60.0)
        } else {
            f32::INFINITY
        };

        let lang: Lang = tab.lang;
        let text_len = tab.text.len();
        let current_match = current_match.filter(|r| r.start < r.end && r.end <= text_len);

        let mut layouter = |ui: &Ui, buf: &dyn TextBuffer, _w: f32| -> Arc<Galley> {
            let text = buf.as_str();

            // 文本变化 → 重新做语法着色
            if cache.cache_text != text {
                cache.cache_text.clear();
                cache.cache_text.push_str(text);
                cache.spans = highlight::tokenize(&cache.cache_text, lang);
            }

            let mut job = LayoutJob {
                wrap: TextWrapping {
                    max_width: wrap_width,
                    ..Default::default()
                },
                keep_trailing_whitespace: true,
                ..Default::default()
            };

            let m_bg = match_bg(dark, false);
            let c_bg = match_bg(dark, true);

            let color_of = |kind: TokKind| {
                let [r, g, b] = if highlight {
                    highlight::kind_color(kind, dark)
                } else {
                    highlight::kind_color(TokKind::Norm, dark)
                };
                Color32::from_rgb(r, g, b)
            };

            let fmt_of = |kind: TokKind, bg: Color32| TextFormat {
                font_id: font_id.clone(),
                color: color_of(kind),
                background: bg,
                line_height: Some(line_h),
                ..Default::default()
            };

            // 拼接片段(双指针合并查找高亮)
            let mut mi = 0usize;
            for span in &cache.spans {
                let span_end = span.end.min(text_len);
                if span.start >= span_end {
                    continue;
                }
                while mi < matches.len() && matches[mi].end <= span.start {
                    mi += 1;
                }
                let mut s = span.start;
                let mut k = mi;
                while s < span_end {
                    if k < matches.len() && matches[k].start <= s {
                        let is_current = current_match.as_ref() == Some(&matches[k]);
                        let end = matches[k].end.min(span_end);
                        if end > s {
                            let slice = text.get(s..end).unwrap_or("");
                            if !slice.is_empty() {
                                job.append(
                                    slice,
                                    0.0,
                                    fmt_of(span.kind, if is_current { c_bg } else { m_bg }),
                                );
                            }
                            s = end;
                        }
                        if matches[k].end <= s {
                            k += 1;
                        }
                        continue;
                    }
                    if k < matches.len() && matches[k].start < span_end {
                        let b = matches[k].start.clamp(s, span_end);
                        if b > s {
                            let slice = text.get(s..b).unwrap_or("");
                            if !slice.is_empty() {
                                job.append(slice, 0.0, fmt_of(span.kind, Color32::TRANSPARENT));
                            }
                            s = b;
                        }
                        continue;
                    }
                    let slice = text.get(s..span_end).unwrap_or("");
                    if !slice.is_empty() {
                        job.append(slice, 0.0, fmt_of(span.kind, Color32::TRANSPARENT));
                    }
                    s = span_end;
                }
            }

            ui.fonts_mut(|f| f.layout_job(job))
        };

        let mut text_changed;
        {
            // 拦截回车(改由我们自己做自动缩进);F25 永远不会被按下
            let mut buffer = EditorBuffer::new(&mut tab.text, !tab.readonly);
            let text_edit = TextEdit::multiline(&mut buffer)
                .id(id)
                .code_editor()
                .return_key(KeyboardShortcut::new(Modifiers::NONE, Key::F25))
                .desired_width(wrap_width)
                .desired_rows(1)
                .layouter(&mut layouter);

            let out = text_edit.show(ui);

            // ---- 行号栏绘制(与 galley 行信息精确对齐) ----
            paint_gutter(
                ui.painter(),
                gutter_rect,
                &out.galley,
                out.galley_pos,
                dark,
                out.response.rect,
            );

            // ---- 高度缓存 ----
            let widget_h = out.response.rect.height();
            if (cache.content_height - widget_h).abs() > 0.5 {
                cache.content_height = widget_h;
                ui.ctx().request_repaint();
            }

            // ---- 光标信息回写 ----
            text_changed = out.response.changed();
            let cursor = out.cursor_range.or_else(|| {
                // 无焦点时 out.cursor_range 为 None,从状态里读
                TextEditState::load(ui.ctx(), id).and_then(|st| st.cursor.range(&out.galley))
            });
            if let Some(r) = cursor {
                let p = r.primary.index.0.min(char_len(&tab.text));
                let s = r.secondary.index.0.min(char_len(&tab.text));
                tab.cursor_char = p;
                tab.sel_chars = p.abs_diff(s);
                tab.sel_char = Some((s.min(p), s.max(p)));
            } else {
                tab.sel_char = None;
            }

            // ---- 编辑器内按键:自动缩进、F5 插入日期时间 ----
            if !tab.readonly && out.response.has_focus() {
                if let Some(cursor_range) = out.cursor_range {
                    let has_ime =
                        ui.input(|i| i.events.iter().any(|e| matches!(e, Event::Ime(_))));
                    let m = ui.input(|i| i.modifiers);
                    let enter = ui.input(|i| i.key_pressed(Key::Enter)) && !has_ime;
                    let f5 = ui.input(|i| i.key_pressed(Key::F5)) && !has_ime;

                    if enter && !(m.ctrl || m.alt || m.mac_cmd) {
                        let sorted = cursor_range.as_sorted_char_range();
                        let (s, e) = (sorted.start.0, sorted.end.0);
                        let char_count = tab.text.chars().count();
                        let sb = textops::char_to_byte(&tab.text, s.min(char_count));
                        let line_start = tab.text[..sb].rfind('\n').map_or(0, |p| p + 1);
                        let indent: String = tab.text[line_start..sb]
                            .chars()
                            .take_while(|c| *c == ' ' || *c == '\t')
                            .collect();
                        let insertion = format!("\n{}", indent);
                        let eb = textops::char_to_byte(&tab.text, e.min(char_count));
                        tab.text.replace_range(sb..eb, &insertion);
                        set_cursor(
                            ui.ctx(),
                            id,
                            CCursorRange::one(CCursor::new(CharIndex(
                                s + insertion.chars().count(),
                            ))),
                        );
                        ui.ctx().request_repaint();
                        text_changed = true;
                    } else if f5 {
                        let stamp = chrono::Local::now()
                            .format("%Y-%m-%d %H:%M:%S")
                            .to_string();
                        let sorted = cursor_range.as_sorted_char_range();
                        let (s, e) = (sorted.start.0, sorted.end.0);
                        let char_count = tab.text.chars().count();
                        let sb = textops::char_to_byte(&tab.text, s.min(char_count));
                        let eb = textops::char_to_byte(&tab.text, e.min(char_count));
                        tab.text.replace_range(sb..eb, &stamp);
                        set_cursor(
                            ui.ctx(),
                            id,
                            CCursorRange::one(CCursor::new(CharIndex(
                                s + stamp.chars().count(),
                            ))),
                        );
                        ui.ctx().request_repaint();
                        text_changed = true;
                    }
                }
            }
        }

        EditorOut { text_changed }
    })
    .inner;

    // 空白区域可点击:聚焦编辑器并把光标移到文末(仿 Notepad++ 的大面积编辑区)
    let remaining = (ui.available_height()).max(0.0);
    if remaining > 0.0 {
        let (_, resp) = ui.allocate_exact_size(vec2(ui.available_width(), remaining), Sense::click());
        if resp.clicked() {
            let end = char_len(&tab.text);
            tab.jump_to = Some((end, end));
            ui.memory_mut(|m| m.request_focus(editor_id(tab.uid)));
            ui.ctx().request_repaint();
        }
    }

    out
}

fn match_bg(dark: bool, current: bool) -> Color32 {
    if dark {
        if current {
            Color32::from_rgba_unmultiplied(255, 140, 0, 130)
        } else {
            Color32::from_rgba_unmultiplied(255, 160, 0, 60)
        }
    } else if current {
        Color32::from_rgba_unmultiplied(255, 160, 0, 130)
    } else {
        Color32::from_rgba_unmultiplied(255, 190, 0, 70)
    }
}

// ---------------------------------------------------------------------------
// 行号栏
// ---------------------------------------------------------------------------

fn paint_gutter(
    painter: &egui::Painter,
    rect: Rect,
    galley: &Galley,
    galley_pos: Pos2,
    dark: bool,
    visible: Rect,
) {
    let bg = if dark {
        Color32::from_rgb(0x1B, 0x1E, 0x24)
    } else {
        Color32::from_rgb(0xF0, 0xF0, 0xF2)
    };
    let line_color = if dark {
        Color32::from_rgb(0x30, 0x33, 0x3A)
    } else {
        Color32::from_rgb(0xDD, 0xDD, 0xE0)
    };
    let num_color = if dark {
        Color32::from_rgb(0x7A, 0x80, 0x8C)
    } else {
        Color32::from_rgb(0x9A, 0x9A, 0xA2)
    };
    painter.rect_filled(rect, 0.0, bg);
    painter.line_segment(
        [
            Pos2::new(rect.right(), rect.top()),
            Pos2::new(rect.right(), rect.bottom()),
        ],
        Stroke::new(1.0, line_color),
    );

    let num_font = FontId::monospace(11.0);
    let pad_right = 10.0;

    // 可见范围(内容坐标),只画可见行号,避免大文件每帧上万个排版调用
    let vis_top = visible.top() - galley_pos.y - 40.0;
    let vis_bottom = visible.bottom() - galley_pos.y + 40.0;

    let mut line_no: usize = 0;
    for (ri, row) in galley.rows.iter().enumerate() {
        let starts_line = ri == 0 || galley.rows[ri - 1].ends_with_newline;
        if !starts_line {
            continue;
        }
        line_no += 1;
        let y = galley_pos.y + row.pos.y;
        if y < vis_top || y > vis_bottom {
            continue;
        }
        painter.text(
            Pos2::new(rect.right() - pad_right, y),
            Align2::RIGHT_TOP,
            line_no.to_string(),
            num_font.clone(),
            num_color,
        );
    }
}

/// 设置编辑器光标(供跳转后仍需微调时使用)。
pub fn set_cursor(ctx: &egui::Context, id: Id, range: CCursorRange) {
    let mut st = TextEditState::load(ctx, id).unwrap_or_default();
    st.cursor.set_char_range(Some(range));
    st.store(ctx, id);
}
