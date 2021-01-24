use chrono::Duration;
use flight_tracker::Tracker;
use std::{
    fmt,
    sync::{Arc, Mutex},
};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Span, Spans},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame,
};

const NA: &str = "";

pub fn fmt_value<T: fmt::Display>(value: Option<T>, precision: usize) -> String {
    value
        .map(|v| format!("{:>.1$}", v, precision))
        .unwrap_or_else(|| NA.to_string())
}

pub struct App<'a> {
    pub title: &'a str,
    pub should_quit: bool,
    tracker: Arc<Mutex<Tracker>>,
}

impl<'a> App<'a> {
    pub fn new(title: &'a str, tracker: Arc<Mutex<Tracker>>) -> App<'a> {
        App {
            title,
            should_quit: false,
            tracker,
        }
    }
}

pub fn draw<B: Backend>(f: &mut Frame<B>, app: &App) {
    let rects = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(0)].as_ref())
        .margin(0)
        .split(f.size());
    draw_message_stats(f, app, rects[0]);
    draw_aircraft_table(f, app, rects[1]);
}

fn draw_message_stats<B: Backend>(f: &mut Frame<B>, app: &App, rect: Rect) {
    let tracker = app.tracker.lock().unwrap();
    if let Some(now) = tracker.get_most_recent_message_time() {
        let text = vec![
            Spans::from(vec![Span::raw(format!("{}", now))]),
            Spans::from(vec![Span::raw(format!(
                "# msgs:       {:>10}",
                tracker.get_num_messages()
            ))]),
            Spans::from(vec![Span::raw(format!(
                "# unk:        {:>10}",
                tracker.get_num_unknown_messages()
            ))]),
            Spans::from(vec![Span::raw(format!(
                "# msgs proc/s {:>12.1}",
                tracker.get_messages_per_second_real_time().unwrap_or(0.0)
            ))]),
        ];
        f.render_widget(Paragraph::new(text), rect);
    }
}

fn draw_aircraft_table<B: Backend>(f: &mut Frame<B>, app: &App, rect: Rect) {
    let tracker = app.tracker.lock().unwrap();
    let normal_style = Style::default();
    let header_cells = [
        "ICAO", "CALL", "SQK", "ALT", "HDG", "GS", "VR", "POS", "LAST",
    ]
    .iter()
    .map(|h| Cell::from(*h).style(Style::default()));
    let header = Row::new(header_cells).style(normal_style);
    let rows = match tracker.get_most_recent_message_time() {
        Some(now) => tracker
            .get_current_aircraft(&Duration::seconds(60), now)
            .iter()
            .map(|a| {
                let aircraft = *a;
                let fields = vec![
                    format!("{}", aircraft.icao_address),
                    aircraft.callsign.clone().unwrap_or_else(|| "".to_string()),
                    "".to_string(),
                    fmt_value(aircraft.altitude, 0),
                    fmt_value(aircraft.heading, 0),
                    fmt_value(aircraft.ground_speed, 0),
                    fmt_value(aircraft.vertical_rate, 0),
                    format!(
                        "{:>8},{:>8}",
                        fmt_value(aircraft.latitude, 4),
                        fmt_value(aircraft.longitude, 4)
                    ),
                    fmt_value(
                        Some(now.signed_duration_since(aircraft.last_seen).num_seconds()),
                        0,
                    ),
                ];
                Row::new(fields.iter().map(|item| Cell::from(item.clone())))
            })
            .collect::<Vec<Row>>(),
        _ => {
            vec![Row::new(vec![
                Cell::from("empty".to_string()),
                Cell::from("empty2".to_string()),
                Cell::from("empty3".to_string()),
            ])]
        }
    };
    let t = Table::new(rows)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title("Aircraft"))
        .widths(&[
            Constraint::Length(6),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(6),
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Length(8),
            Constraint::Length(17),
            Constraint::Length(5),
        ]);
    f.render_widget(t, rect);
}
