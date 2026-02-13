use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Wrap},
    Frame,
};

use super::app::*;

pub fn draw(frame: &mut Frame, app: &App) {
    match app.screen {
        Screen::Setup => draw_setup(frame, app),
        Screen::Running => draw_running(frame, app),
        Screen::Results => draw_results(frame, app),
    }
}

fn draw_setup(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let outer = Block::default()
        .title("  EMAS - Evolutionary Multi-Agent System  ")
        .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(6),
            Constraint::Min(14),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(inner);

    draw_problem_field(frame, app, chunks[0]);
    draw_config_fields(frame, app, chunks[1]);
    draw_start_area(frame, app, chunks[2]);
    draw_setup_help(frame, app, chunks[3]);
}

fn draw_problem_field(frame: &mut Frame, app: &App, area: Rect) {
    let is_selected = app.selected_field == F_PROBLEM;
    let is_editing = is_selected && app.editing;

    let border_color = if is_editing {
        Color::Yellow
    } else if is_selected {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .title(" Problem ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let field = &app.fields[F_PROBLEM];
    let display_text = if field.value.is_empty() && !is_editing {
        Span::styled(
            field.placeholder,
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
        )
    } else {
        Span::raw(&field.value)
    };

    let paragraph = Paragraph::new(display_text)
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);

    if is_editing {
        let inner = Block::default().borders(Borders::ALL).inner(area);
        let cursor_x = inner.x + app.cursor as u16;
        let cursor_y = inner.y;
        if cursor_x < inner.x + inner.width {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

fn draw_config_fields(frame: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let left_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Length(5),
            Constraint::Min(0),
        ])
        .split(cols[0]);

    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Length(5),
            Constraint::Min(0),
        ])
        .split(cols[1]);

    let agent_fields = vec![
        (F_PROVIDER, "Provider"),
        (F_MODEL, "Model"),
        (F_API_URL, "API URL"),
        (F_API_KEY, "API Key"),
    ];
    draw_field_group(frame, app, left_rows[0], " Agent ", &agent_fields);

    let judge_fields = vec![
        (F_JUDGE_PROVIDER, "Judge Provider"),
        (F_JUDGE_MODEL, "Judge Model"),
    ];
    draw_field_group(frame, app, right_rows[0], " Judge ", &judge_fields);

    let param_left = vec![
        (F_POPULATION, "Population"),
        (F_TEAM_SIZE, "Team Size"),
        (F_GENERATIONS, "Generations"),
    ];
    draw_field_group(frame, app, left_rows[1], " Parameters ", &param_left);

    let param_right = vec![
        (F_THRESHOLD, "Threshold"),
        (F_MUTATION, "Mutation Rate"),
    ];
    draw_field_group(frame, app, right_rows[1], "", &param_right);

    let weights = vec![
        (F_QUALITY_W, "Quality W."),
        (F_CONSISTENCY_W, "Consistency W."),
        (F_EFFICIENCY_W, "Efficiency W."),
    ];
    draw_field_group(frame, app, left_rows[2], " Weights ", &weights);
}

fn draw_field_group(
    frame: &mut Frame,
    app: &App,
    area: Rect,
    title: &str,
    fields: &[(usize, &str)],
) {
    let block = if title.is_empty() {
        Block::default()
    } else {
        Block::default()
            .title(title)
            .title_style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
    };
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let items: Vec<ListItem> = fields
        .iter()
        .map(|(idx, label)| {
            let is_selected = app.selected_field == *idx;
            let is_editing = is_selected && app.editing;
            let field = &app.fields[*idx];

            let marker = if is_editing {
                "* "
            } else if is_selected {
                "> "
            } else {
                "  "
            };

            let value_display = if is_editing {
                field.value.clone()
            } else {
                field.display_value()
            };
            let select_hint = match &field.kind {
                FieldKind::Select { .. } => " <>",
                _ => "",
            };

            let label_style = if is_selected {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };

            let value_style = if is_editing {
                Style::default().fg(Color::Yellow)
            } else if field.value.is_empty() {
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(Line::from(vec![
                Span::styled(marker, label_style),
                Span::styled(format!("{:<16}", label), label_style),
                Span::styled(value_display, value_style),
                Span::styled(select_hint, Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

fn draw_start_area(frame: &mut Frame, app: &App, area: Rect) {
    if let Some(err) = &app.error_message {
        let msg = Paragraph::new(Line::from(vec![
            Span::styled("  Error: ", Style::default().fg(Color::Red)),
            Span::styled(err.as_str(), Style::default().fg(Color::Red)),
        ]));
        frame.render_widget(msg, area);
    } else {
        let btn_text = "  Start Evolution (F5)  ";
        let btn = Paragraph::new(Line::from(vec![Span::styled(
            btn_text,
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )]))
        .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(btn, area);
    }
}

fn draw_setup_help(frame: &mut Frame, app: &App, area: Rect) {
    let help = if app.editing {
        "  Esc: stop editing  |  Tab: next field  |  F5: start  |  < >: cursor"
    } else {
        "  Tab/Up-Down: navigate  |  Enter: edit  |  < >: select option  |  F5: start  |  q: quit"
    };
    let p = Paragraph::new(Span::styled(
        help,
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(p, area);
}

// Running Screen

fn draw_running(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let title = format!(
        "  EMAS - Generation {}/{}  ",
        app.generation, app.max_generations
    );
    let outer = Block::default()
        .title(title)
        .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(6),
            Constraint::Length(1),
            Constraint::Length(8),
            Constraint::Length(2),
        ])
        .split(inner);

    let problem_display = if app.problem_text_cache.len() > (chunks[0].width as usize).saturating_sub(14) {
        let max = (chunks[0].width as usize).saturating_sub(17);
        format!("  Problem: {}...", &app.problem_text_cache[..max.min(app.problem_text_cache.len())])
    } else {
        format!("  Problem: {}", app.problem_text_cache)
    };
    let problem_line = Paragraph::new(Line::from(vec![
        Span::styled(
            problem_display,
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
        ),
    ]));
    frame.render_widget(problem_line, chunks[0]);

    let progress = if app.max_generations > 0 {
        (app.generation as f64 / app.max_generations as f64).min(1.0)
    } else {
        0.0
    };
    let gauge = Gauge::default()
        .block(
            Block::default()
                .title(" Progress ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .gauge_style(Style::default().fg(Color::Green).bg(Color::Black))
        .ratio(progress)
        .label(format!(
            "{}/{}  ({:.0}%)",
            app.generation,
            app.max_generations,
            progress * 100.0
        ));
    frame.render_widget(gauge, chunks[2]);

    let elapsed_str = app.format_elapsed();
    let eta_str = app
        .estimate_remaining()
        .map(|e| format!("  ETA: ~{}", e))
        .unwrap_or_default();
    let token_str = if app.total_tokens > 0 {
        format!("  Tokens: {}", format_token_count(app.total_tokens))
    } else {
        String::new()
    };
    let phase_str = format!("  Phase: {}", app.phase);

    let stats_line = Line::from(vec![
        Span::styled("  Elapsed: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            elapsed_str,
            Style::default().fg(Color::White),
        ),
        Span::styled(
            eta_str,
            Style::default().fg(Color::Yellow),
        ),
        Span::styled(
            token_str,
            Style::default().fg(Color::Cyan),
        ),
        Span::styled(
            phase_str,
            Style::default().fg(Color::Magenta),
        ),
    ]);
    frame.render_widget(Paragraph::new(stats_line), chunks[3]);

    draw_team_scores(frame, app, chunks[5]);

    let status_line = if !app.best_name.is_empty() {
        Line::from(vec![
            Span::styled("  Best: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(&app.best_name, Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(
                format!(" ({:.2}/10)", app.best_score),
                Style::default().fg(Color::Green),
            ),
        ])
    } else {
        Line::from(Span::styled(
            &app.status,
            Style::default().fg(Color::Gray),
        ))
    };
    frame.render_widget(Paragraph::new(status_line), chunks[6]);

    draw_log(frame, app, chunks[7]);

    let help = Paragraph::new(Span::styled(
        "  Up/Down: select team  |  Enter: team details  |  q: quit",
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(help, chunks[8]);

    if app.show_team_detail {
        draw_team_detail_overlay(frame, app);
    }
}

fn draw_team_scores(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Team Scores ")
        .title_style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.team_scores.is_empty() {
        let waiting = Paragraph::new(Span::styled(
            "  Waiting for results...",
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
        ));
        frame.render_widget(waiting, inner);
        return;
    }

    let items: Vec<ListItem> = app
        .team_scores
        .iter()
        .enumerate()
        .map(|(i, ts)| {
            let is_selected = i == app.selected_team;
            let marker = if is_selected { ">" } else { " " };
            let bar = score_bar_spans(ts.total, 20);

            let name_style = if is_selected {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else if i == 0 {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let mut spans = vec![
                Span::styled(
                    format!(" {} {:<16}", marker, ts.name),
                    name_style,
                ),
            ];
            spans.extend(bar);
            spans.push(Span::styled(
                format!(
                    "  {:.2}  Q:{:.1} C:{:.1} E:{:.1}",
                    ts.total, ts.quality, ts.consistency, ts.efficiency
                ),
                Style::default().fg(Color::DarkGray),
            ));

            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

fn draw_team_detail_overlay(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let overlay_w = (area.width * 60 / 100).max(40).min(area.width.saturating_sub(4));
    let overlay_h = 14u16.min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(overlay_w)) / 2;
    let y = area.y + (area.height.saturating_sub(overlay_h)) / 2;
    let overlay_area = Rect::new(x, y, overlay_w, overlay_h);

    let clear = Block::default()
        .style(Style::default().bg(Color::Black));
    frame.render_widget(clear, overlay_area);

    let team_name = app.team_scores.get(app.selected_team).map(|ts| &ts.name);
    let detail = team_name.and_then(|name| {
        app.team_details.iter().find(|d| &d.name == name)
    });
    let score = app.team_scores.get(app.selected_team);

    let mut lines: Vec<Line> = Vec::new();

    if let (Some(detail), Some(score)) = (detail, score) {
        lines.push(Line::from(vec![
            Span::styled("  Team: ", Style::default().fg(Color::Yellow)),
            Span::styled(
                &detail.name,
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("     Score: {:.2}/10", score.total),
                Style::default().fg(Color::Green),
            ),
        ]));
        lines.push(Line::from(Span::styled(
            format!(
                "     Q:{:.1}  C:{:.1}  E:{:.1}",
                score.quality, score.consistency, score.efficiency,
            ),
            Style::default().fg(Color::Gray),
        )));
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "  Agents:",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )));

        for (i, agent) in detail.agents.iter().enumerate() {
            let is_last = i == detail.agents.len() - 1;
            let branch = if is_last { "  └──" } else { "  ├──" };

            let tag = if agent.is_red_team {
                Span::styled(" [RED] ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
            } else {
                Span::raw(" ")
            };

            lines.push(Line::from(vec![
                Span::styled(branch, Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(" {}", agent.name),
                    Style::default().fg(Color::Cyan),
                ),
                tag,
                Span::styled(
                    format!("({}, temp {:.2})", agent.strategy, agent.temperature),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "  No detail available for this team.",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let block = Block::default()
        .title(" Team Detail ")
        .title_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(Color::Black));

    let paragraph = Paragraph::new(Text::from(lines))
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, overlay_area);
}

fn draw_log(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Activity Log ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let max_lines = inner.height as usize;
    let start = app.logs.len().saturating_sub(max_lines);
    let items: Vec<ListItem> = app.logs[start..]
        .iter()
        .map(|line| {
            let color = if line.contains("Error:") {
                Color::Red
            } else if line.contains("Warning:") {
                Color::Yellow
            } else if line.contains("started") {
                Color::Cyan
            } else if line.contains("Evolving:") {
                Color::Magenta
            } else if line.contains("Converged") || line.contains("complete") {
                Color::Green
            } else if line.contains("Synthesising") {
                Color::Blue
            } else {
                Color::Gray
            };
            ListItem::new(Span::styled(
                line.as_str(),
                Style::default().fg(color),
            ))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

fn format_token_count(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        format!("{}", tokens)
    }
}

// Results Screen

fn draw_results(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let outer = Block::default()
        .title("  EMAS - Results  ")
        .title_style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let result = match &app.result {
        Some(r) => r,
        None => return,
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(10),
            Constraint::Min(6),
            Constraint::Length(2),
        ])
        .split(inner);

    draw_winner_info(frame, result, chunks[0]);
    draw_synthesis(frame, app, result, chunks[1]);

    let help = Paragraph::new(Span::styled(
        "  Up-Down/PgUp/PgDn: scroll  |  n: new run  |  q: quit",
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(help, chunks[2]);
}

fn draw_winner_info(frame: &mut Frame, result: &crate::arena::EvolutionResult, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(vec![
        Span::styled("  Winner: ", Style::default().fg(Color::Yellow)),
        Span::styled(
            &result.best_team.name,
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("     Score: {:.2}/10", result.best_score.total),
            Style::default().fg(Color::Green),
        ),
    ]));

    lines.push(Line::from(vec![
        Span::styled(
            format!(
                "     Generations: {}     Q:{:.1}  C:{:.1}  E:{:.1}",
                result.generations_run,
                result.best_score.quality,
                result.best_score.consistency,
                result.best_score.efficiency,
            ),
            Style::default().fg(Color::Gray),
        ),
    ]));

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "     Team Composition:",
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
    )));

    for (i, agent) in result.best_team.agents.iter().enumerate() {
        let is_last = i == result.best_team.agents.len() - 1;
        let branch = if is_last { "     |--" } else { "     |--" };
        lines.push(Line::from(vec![
            Span::styled(branch, Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(" {} ", agent.genotype.name),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(
                format!("({}, temp {:.2})", agent.genotype.strategy, agent.genotype.temperature),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }

    let block = Block::default()
        .title(" Winner ")
        .title_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let paragraph = Paragraph::new(Text::from(lines)).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_synthesis(
    frame: &mut Frame,
    app: &App,
    result: &crate::arena::EvolutionResult,
    area: Rect,
) {
    let block = Block::default()
        .title(" Synthesised Response ")
        .title_style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let paragraph = Paragraph::new(result.synthesis.as_str())
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((app.scroll_offset, 0));

    frame.render_widget(paragraph, area);
}

fn score_bar_spans(score: f64, width: usize) -> Vec<Span<'static>> {
    let clamped = score.clamp(0.0, 10.0);
    let filled = (clamped / 10.0 * width as f64) as usize;
    let empty = width.saturating_sub(filled);

    let color = if clamped >= 8.0 {
        Color::Green
    } else if clamped >= 5.0 {
        Color::Yellow
    } else {
        Color::Red
    };

    vec![
        Span::styled("#".repeat(filled), Style::default().fg(color)),
        Span::styled("-".repeat(empty), Style::default().fg(Color::DarkGray)),
    ]
}
