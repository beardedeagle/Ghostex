use crate::model::Agent;
use crate::model::Session;
use crate::model::TranscriptBlock;
use crate::model::TranscriptKind;
use chrono::DateTime;
use chrono::Local;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Clear;
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use textwrap::Options;
use unicode_width::UnicodeWidthStr;

use super::*;

impl App {
    pub(super) fn draw_list(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();
        frame.render_widget(Clear, area);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(area.height.saturating_sub(PICKER_CHROME_HEIGHT)),
                Constraint::Length(FOOTER_HEIGHT),
            ])
            .split(area);
        let header = inner_x(chunks[0], 1);
        render_slash_header(
            frame,
            header,
            &format!(
                "{}  {} sessions",
                spaced_header_title("Agent history"),
                self.filtered.len()
            ),
        );
        let search = inner_x(chunks[2], 1);
        frame.render_widget(Paragraph::new(self.search_line(search.width)), search);
        let list = inner_x(chunks[4], 2);
        self.draw_rows(frame, list);
        self.draw_footer(frame, chunks[5], list.height);
    }

    fn draw_rows(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let content_rows = area.height as usize;
        self.ensure_selected_visible(content_rows);
        if self.filtered.is_empty() {
            frame.render_widget(
                Paragraph::new("No sessions found").style(Style::default().fg(Color::DarkGray)),
                area,
            );
            return;
        }

        let mut y = area.y;
        if self.scroll_top > 0 && y < area.bottom() {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "more above",
                    Style::default().fg(Color::DarkGray),
                ))),
                Rect::new(area.x, y, area.width, 1),
            );
            y += 1;
        }
        let mut used = usize::from(self.scroll_top > 0);
        for (visible_offset, session_index) in self.filtered[self.scroll_top..].iter().enumerate() {
            if used >= content_rows {
                break;
            }
            let row_index = self.scroll_top + visible_offset;
            let selected = row_index == self.selected;
            let expanded = self.expanded == Some(*session_index);
            let row_lines = self.session_row_lines(*session_index, selected, expanded, area.width);
            for line in row_lines {
                if y >= area.bottom() {
                    break;
                }
                frame.render_widget(Paragraph::new(line), Rect::new(area.x, y, area.width, 1));
                y += 1;
                used += 1;
            }
            if self.density == Density::Comfortable && y < area.bottom() {
                y += ROW_GAP_COMFORTABLE as u16;
                used += ROW_GAP_COMFORTABLE;
            }
        }
        if self.has_more_below(content_rows) && area.bottom() > area.y {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "more below",
                    Style::default().fg(Color::DarkGray),
                ))),
                Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1),
            );
        }
    }

    fn draw_footer(&self, frame: &mut Frame<'_>, area: Rect, list_height: u16) {
        if area.height == 0 {
            return;
        }
        let separator = "─".repeat(area.width as usize);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                separator,
                Style::default().fg(Color::DarkGray),
            ))),
            Rect::new(area.x, area.y, area.width, 1),
        );
        let progress = self.footer_progress(list_height);
        let progress_width = progress.width() as u16;
        if progress_width < area.width {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    progress,
                    Style::default().fg(Color::DarkGray),
                ))),
                Rect::new(
                    area.x + area.width - progress_width.saturating_add(1),
                    area.y,
                    progress_width,
                    1,
                ),
            );
        }
        let hints = [
            ("enter", "open"),
            ("ctrl+r", "resume"),
            (
                "esc",
                if self.query.is_empty() {
                    "quit"
                } else {
                    "clear"
                },
            ),
            ("ctrl+c", "quit"),
        ];
        let hints2 = [
            ("tab", "focus"),
            ("left/right", "option"),
            ("ctrl+t", "transcript"),
            ("ctrl+e", "preview"),
        ];
        let hints3 = [
            (
                "ctrl+o",
                match self.density {
                    Density::Comfortable => "dense",
                    Density::Dense => "comfortable",
                },
            ),
            ("pgup/pgdn", "page"),
        ];
        self.render_hint_line(frame, area, 1, &hints);
        self.render_hint_line(frame, area, 2, &hints2);
        self.render_hint_line(frame, area, 3, &hints3);
    }

    fn render_hint_line(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        offset: u16,
        hints: &[(&str, &str)],
    ) {
        let y = area.y.saturating_add(offset);
        if y >= area.bottom() {
            return;
        }
        let mut spans = vec![Span::raw(" ")];
        for (index, (key, label)) in hints.iter().enumerate() {
            if index > 0 {
                spans.push(Span::raw("   "));
            }
            spans.push(Span::styled(*key, Style::default().fg(Color::Cyan)));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(*label, Style::default().fg(Color::DarkGray)));
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(area.x, y, area.width, 1),
        );
    }

    pub(super) fn draw_transcript(
        &mut self,
        frame: &mut Frame<'_>,
        session_index: usize,
        scroll: usize,
        highlight_block: Option<usize>,
    ) {
        let area = frame.area();
        frame.render_widget(Clear, area);
        let Some(session) = self.sessions.get(session_index) else {
            self.screen = Screen::List;
            return;
        };
        let top_height = area.height.saturating_sub(TRANSCRIPT_HINT_HEIGHT);
        let top_area = Rect::new(area.x, area.y, area.width, top_height);
        let hint_area = Rect::new(
            area.x,
            area.y.saturating_add(top_height),
            area.width,
            area.height.saturating_sub(top_height),
        );
        let content_area = Rect::new(
            top_area.x,
            top_area.y.saturating_add(1),
            top_area.width,
            top_area.height.saturating_sub(2),
        );
        render_slash_header(
            frame,
            Rect::new(top_area.x, top_area.y, top_area.width, 1),
            "T R A N S C R I P T",
        );

        let layout = transcript_layout(&session.blocks, content_area.width, highlight_block);
        let max_scroll = layout
            .lines
            .len()
            .saturating_sub(content_area.height as usize);
        let scroll = if scroll == usize::MAX {
            max_scroll
        } else {
            scroll.min(max_scroll)
        };
        self.screen = Screen::Transcript {
            session_index,
            scroll,
            highlight_block,
        };
        for (row, line) in layout
            .lines
            .iter()
            .skip(scroll)
            .take(content_area.height as usize)
            .enumerate()
        {
            frame.render_widget(
                Paragraph::new(line.clone()),
                Rect::new(
                    content_area.x,
                    content_area.y + row as u16,
                    content_area.width,
                    1,
                ),
            );
        }
        let drawn_rows = layout.lines.len().saturating_sub(scroll) as u16;
        for y in content_area.y + drawn_rows.min(content_area.height)..content_area.bottom() {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "~",
                    Style::default().fg(Color::DarkGray),
                ))),
                Rect::new(content_area.x, y, content_area.width, 1),
            );
        }
        self.draw_transcript_bottom(top_area, frame, scroll, max_scroll);
        self.draw_transcript_hints(frame, hint_area, highlight_block.is_some());
    }

    fn draw_transcript_bottom(
        &self,
        area: Rect,
        frame: &mut Frame<'_>,
        scroll: usize,
        max_scroll: usize,
    ) {
        let y = area.bottom().saturating_sub(1);
        let separator = "─".repeat(area.width as usize);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                separator,
                Style::default().fg(Color::DarkGray),
            ))),
            Rect::new(area.x, y, area.width, 1),
        );
        let percent = if max_scroll == 0 {
            100
        } else {
            ((scroll.min(max_scroll) as f32 / max_scroll as f32) * 100.0).round() as u8
        };
        let label = format!(" {percent}% ");
        let label_width = label.width() as u16;
        if label_width < area.width {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    label,
                    Style::default().fg(Color::DarkGray),
                ))),
                Rect::new(
                    area.x + area.width - label_width.saturating_add(1),
                    y,
                    label_width,
                    1,
                ),
            );
        }
    }

    fn draw_transcript_hints(&self, frame: &mut Frame<'_>, area: Rect, highlight_active: bool) {
        self.render_hint_line(
            frame,
            area,
            0,
            &[
                ("ctrl+r", "to resume"),
                ("↑/↓", "to scroll"),
                ("pgup/pgdn", "to page"),
                ("home/end", "to jump"),
            ],
        );
        let mut hints = vec![("q", "to quit")];
        if highlight_active {
            hints.push(("esc/←", "to edit prev"));
            hints.push(("→", "to edit next"));
            hints.push(("enter", "to edit message"));
        } else {
            hints.push(("esc", "to edit prev"));
        }
        self.render_hint_line(frame, area, 1, &hints);
    }

    fn search_line(&self, width: u16) -> Line<'static> {
        let search_text = if self.query.is_empty() {
            Span::styled("Type to search", Style::default().fg(Color::DarkGray))
        } else {
            Span::raw(format!("Search: {}", self.query))
        };
        let toolbar = self.toolbar_line(width);
        let search_width = search_text.content.width();
        let toolbar_width = toolbar.width();
        let spacer = width
            .saturating_sub((search_width + toolbar_width) as u16)
            .max(2) as usize;
        let mut spans = vec![search_text, Span::raw(" ".repeat(spacer))];
        spans.extend(toolbar.spans);
        Line::from(spans)
    }

    fn toolbar_line(&self, _width: u16) -> Line<'static> {
        let spans = vec![
            Span::styled("Filter: ", Style::default().fg(Color::DarkGray)),
            toolbar_value(
                "Cwd",
                self.filter_mode == FilterMode::Cwd,
                self.toolbar_focus == ToolbarControl::Filter,
            ),
            toolbar_value(
                "All",
                self.filter_mode == FilterMode::All,
                self.toolbar_focus == ToolbarControl::Filter,
            ),
            Span::raw("   "),
            Span::styled("Sort: ", Style::default().fg(Color::DarkGray)),
            toolbar_value(
                "Updated",
                self.sort_key == SortKey::Updated,
                self.toolbar_focus == ToolbarControl::Sort,
            ),
            toolbar_value(
                "Created",
                self.sort_key == SortKey::Created,
                self.toolbar_focus == ToolbarControl::Sort,
            ),
        ];
        Line::from(spans)
    }

    fn session_row_lines(
        &self,
        session_index: usize,
        selected: bool,
        expanded: bool,
        width: u16,
    ) -> Vec<Line<'static>> {
        let session = &self.sessions[session_index];
        let marker_style = if selected {
            selected_session_style().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let indicator = match (selected, expanded) {
            (true, true) => "⌄ ",
            (true, false) => "❯ ",
            (false, _) => "  ",
        };
        let title_width = width.saturating_sub(18) as usize;
        let title = truncate_display(session.display_title(), title_width);
        let mut lines = vec![
            Line::from(vec![
                Span::styled(indicator, marker_style),
                Span::styled(agent_label(session.agent), agent_style(session.agent)),
                Span::raw("  "),
                Span::styled(
                    title,
                    if selected {
                        selected_session_style()
                    } else {
                        Style::default()
                    },
                ),
            ]),
            self.session_meta_line(session, selected),
        ];
        if selected {
            lines = apply_session_row_style(lines, selected_session_style(), width);
        }
        if expanded {
            lines.extend(preview_lines(&session.blocks, width.saturating_sub(4)));
        }
        lines
    }

    fn session_meta_line(&self, session: &Session, selected: bool) -> Line<'static> {
        let date = format_timestamp(session.updated_at);
        let project = if session.project.is_empty() {
            session.path.to_string_lossy().to_string()
        } else {
            session.project.clone()
        };
        let style = if selected {
            selected_session_style()
        } else {
            Style::default().fg(Color::DarkGray)
        };
        Line::from(vec![
            Span::raw("    "),
            Span::styled(date, style),
            Span::raw("  "),
            Span::styled(truncate_display(&project, 64), style),
            Span::raw("  "),
            Span::styled(truncate_display(&session.id, 32), style),
        ])
    }

    fn footer_progress(&self, list_height: u16) -> String {
        let position = if self.filtered.is_empty() {
            0
        } else {
            self.selected + 1
        };
        let percent = self.list_percent(list_height as usize);
        format!(" {position}/{} - {percent}% ", self.filtered.len())
    }

    fn list_percent(&self, list_height: usize) -> u8 {
        if self.filtered.is_empty() {
            return 100;
        }
        let max_scroll = self.filtered.len().saturating_sub(list_height.max(1));
        if max_scroll == 0 {
            return 100;
        }
        ((self.scroll_top.min(max_scroll) as f32 / max_scroll as f32) * 100.0).round() as u8
    }
}

fn render_slash_header(frame: &mut Frame<'_>, area: Rect, title: &str) {
    let header_bg = "/ ".repeat(area.width as usize / 2);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            header_bg,
            Style::default().fg(Color::DarkGray),
        ))),
        area,
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("/ {title}"),
            Style::default().fg(Color::DarkGray),
        ))),
        area,
    );
}

pub(super) fn spaced_header_title(title: &str) -> String {
    title
        .split_whitespace()
        .map(|word| {
            word.to_ascii_uppercase()
                .chars()
                .map(|ch| ch.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("  ")
}

fn selected_session_style() -> Style {
    Style::default().fg(Color::Yellow)
}

fn apply_session_row_style(
    lines: Vec<Line<'static>>,
    style: Style,
    width: u16,
) -> Vec<Line<'static>> {
    lines
        .into_iter()
        .map(|mut line| {
            let padding = (width as usize).saturating_sub(line.width());
            if padding > 0 {
                line.spans.push(Span::styled(" ".repeat(padding), style));
            }
            line.style = line.style.patch(style);
            line
        })
        .collect()
}

fn preview_lines(blocks: &[TranscriptBlock], width: u16) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    let mut recent = blocks
        .iter()
        .rev()
        .filter(|block| block.kind != TranscriptKind::Tool)
        .take(4)
        .collect::<Vec<_>>();
    recent.reverse();
    for block in recent {
        let label = format!("{}: ", block.kind.label());
        let text = truncate_display(
            &clean_one_line(&block.text),
            width.saturating_sub(label.width() as u16) as usize,
        );
        out.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(label, Style::default().fg(Color::DarkGray)),
            Span::styled(text, Style::default().fg(Color::Gray)),
        ]));
    }
    out
}

pub(super) fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let options = Options::new(width).break_words(false);
    let mut out = Vec::new();
    for paragraph in text.lines() {
        if paragraph.trim().is_empty() {
            out.push(String::new());
            continue;
        }
        out.extend(
            textwrap::wrap(paragraph, options.clone())
                .into_iter()
                .map(|line| line.into_owned()),
        );
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

fn toolbar_value(label: &'static str, active: bool, focused: bool) -> Span<'static> {
    if active {
        let style = if focused {
            Style::default().fg(Color::Magenta)
        } else {
            Style::default()
        };
        Span::styled(format!("[{label}]"), style)
    } else {
        Span::styled(format!(" {label} "), Style::default().fg(Color::DarkGray))
    }
}

fn inner_x(area: Rect, pad: u16) -> Rect {
    Rect::new(
        area.x.saturating_add(pad),
        area.y,
        area.width.saturating_sub(pad.saturating_mul(2)),
        area.height,
    )
}

fn agent_label(agent: Agent) -> String {
    format!("{:<6}", agent.label())
}

fn agent_style(agent: Agent) -> Style {
    let color = match agent {
        Agent::Claude => Color::Rgb(255, 136, 76),
        Agent::Codex => Color::Rgb(88, 166, 255),
        Agent::Cursor => Color::Rgb(139, 154, 255),
        Agent::Grok => Color::Rgb(115, 231, 156),
        Agent::Pi => Color::Rgb(248, 173, 7),
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

fn format_timestamp(ts: i64) -> String {
    DateTime::from_timestamp(ts, 0)
        .map(|dt| dt.with_timezone(&Local).format("%b %-d %H:%M").to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn clean_one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_display(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if text.width() <= width {
        return text.to_string();
    }
    let mut out = String::new();
    for ch in text.chars() {
        let next_width = out.width() + ch.to_string().width();
        if next_width + 3 > width {
            break;
        }
        out.push(ch);
    }
    out.push_str("...");
    out
}
