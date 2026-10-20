use crate::model::TranscriptBlock;
use crate::model::TranscriptKind;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;

use super::*;

pub(super) struct TranscriptLayout {
    pub(super) lines: Vec<Line<'static>>,
    pub(super) block_starts: Vec<usize>,
    pub(super) user_blocks: Vec<usize>,
}

pub(super) fn transcript_layout(
    blocks: &[TranscriptBlock],
    width: u16,
    highlight_block: Option<usize>,
) -> TranscriptLayout {
    let width = width.max(8) as usize;
    let mut lines = Vec::new();
    let mut block_starts = Vec::with_capacity(blocks.len());
    let mut user_blocks = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        if index > 0 {
            lines.push(Line::default());
        }
        block_starts.push(lines.len());
        if block.kind == TranscriptKind::User {
            user_blocks.push(index);
        }
        let highlighted = highlight_block == Some(index);
        lines.extend(transcript_block_lines(block, width, highlighted));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "No transcript content available",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )));
    }
    TranscriptLayout {
        lines,
        block_starts,
        user_blocks,
    }
}

fn transcript_block_lines(
    block: &TranscriptBlock,
    width: usize,
    highlighted: bool,
) -> Vec<Line<'static>> {
    match block.kind {
        TranscriptKind::User => user_transcript_lines(&block.text, width, highlighted),
        TranscriptKind::Thinking => styled_transcript_lines(
            &block.text,
            width,
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ),
        TranscriptKind::Tool => {
            styled_transcript_lines(&block.text, width, Style::default().fg(Color::DarkGray))
        }
        TranscriptKind::Agent => styled_transcript_lines(&block.text, width, Style::default()),
    }
}

pub(super) fn user_transcript_lines(
    text: &str,
    width: usize,
    highlighted: bool,
) -> Vec<Line<'static>> {
    let wrap_width = width.saturating_sub(LIVE_PREFIX_COLS + 1).max(1);
    let style = user_message_style(highlighted);
    let mut out = vec![Line::from("").style(style)];
    for (index, line) in wrap_text(text, wrap_width).into_iter().enumerate() {
        let prefix = if index == 0 { "› " } else { "  " };
        out.push(
            Line::from(vec![
                Span::styled(
                    prefix,
                    Style::default().add_modifier(Modifier::BOLD | Modifier::DIM),
                ),
                Span::styled(line, style),
            ])
            .style(style),
        );
    }
    out.push(Line::from("").style(style));
    out
}

fn styled_transcript_lines(text: &str, width: usize, style: Style) -> Vec<Line<'static>> {
    wrap_text(text, width)
        .into_iter()
        .map(|line| Line::from(Span::styled(line, style)))
        .collect()
}

pub(super) fn user_message_style(highlighted: bool) -> Style {
    let style = Style::default().bg(Color::Rgb(
        USER_MESSAGE_BG_DARK.0,
        USER_MESSAGE_BG_DARK.1,
        USER_MESSAGE_BG_DARK.2,
    ));
    if highlighted {
        style.add_modifier(Modifier::REVERSED)
    } else {
        style
    }
}

pub(super) fn previous_user_block(user_blocks: &[usize], current: Option<usize>) -> Option<usize> {
    if user_blocks.is_empty() {
        return None;
    }
    let Some(current) = current else {
        return user_blocks.last().copied();
    };
    let position = user_blocks
        .iter()
        .position(|block| *block == current)
        .unwrap_or_else(|| {
            user_blocks
                .iter()
                .position(|block| *block > current)
                .unwrap_or(user_blocks.len())
        });
    user_blocks
        .get(position.saturating_sub(1))
        .copied()
        .or_else(|| user_blocks.first().copied())
}

pub(super) fn next_user_block(user_blocks: &[usize], current: Option<usize>) -> Option<usize> {
    if user_blocks.is_empty() {
        return None;
    }
    let Some(current) = current else {
        return user_blocks.last().copied();
    };
    let position = user_blocks
        .iter()
        .position(|block| *block == current)
        .unwrap_or_else(|| {
            user_blocks
                .iter()
                .position(|block| *block > current)
                .unwrap_or(user_blocks.len().saturating_sub(1))
        });
    user_blocks
        .get(position.saturating_add(1))
        .copied()
        .or_else(|| user_blocks.last().copied())
}

pub(super) fn ensure_block_visible(
    scroll: usize,
    layout: &TranscriptLayout,
    block_index: usize,
    height: usize,
) -> usize {
    let Some(&first) = layout.block_starts.get(block_index) else {
        return scroll;
    };
    let last = layout
        .block_starts
        .iter()
        .enumerate()
        .skip(block_index + 1)
        .map(|(_, start)| *start)
        .next()
        .unwrap_or(layout.lines.len())
        .saturating_sub(1);
    let height = height.max(1);
    let current_bottom = scroll.saturating_add(height.saturating_sub(1));
    if first < scroll {
        first
    } else if last > current_bottom {
        last.saturating_sub(height.saturating_sub(1))
    } else {
        scroll
    }
}

pub(super) fn apply_scroll_delta(scroll: usize, delta: isize, max_scroll: usize) -> usize {
    if delta.is_negative() {
        scroll.saturating_sub(delta.unsigned_abs()).min(max_scroll)
    } else {
        scroll.saturating_add(delta as usize).min(max_scroll)
    }
}

pub(super) fn transcript_content_height(viewport_height: u16) -> u16 {
    viewport_height
        .saturating_sub(TRANSCRIPT_HINT_HEIGHT)
        .saturating_sub(2)
}
