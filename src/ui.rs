use chrono::Duration;
use flight_tracker::Tracker;
use std::{
    fmt,
    sync::{Arc, Mutex},
};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame,
};

const NA: &str = "";

pub fn fmt_value<T: fmt::Display>(value: Option<T>, precision: usize) -> String {
    value
        .map(|v| format!("{:>.1$}", v, precision))
        .unwrap_or_else(|| NA.to_string())
}

pub struct TabsState<'a> {
    pub titles: Vec<&'a str>,
    pub index: usize,
}

impl<'a> TabsState<'a> {
    pub fn new(titles: Vec<&'a str>) -> TabsState {
        TabsState { titles, index: 0 }
    }
    pub fn next(&mut self) {
        self.index = (self.index + 1) % self.titles.len();
    }

    pub fn previous(&mut self) {
        if self.index > 0 {
            self.index -= 1;
        } else {
            self.index = self.titles.len() - 1;
        }
    }
}

pub struct App<'a> {
    pub title: &'a str,
    pub should_quit: bool,
    pub tabs: TabsState<'a>,
    tracker: Arc<Mutex<Tracker>>,
}

impl<'a> App<'a> {
    pub fn new(title: &'a str, tracker: Arc<Mutex<Tracker>>) -> App<'a> {
        App {
            title,
            should_quit: false,
            tabs: TabsState::new(vec!["Tab0", "Tab1"]),
            tracker,
        }
    }
}

pub struct StatefulTable<'a> {
    state: TableState,
    items: Vec<Vec<&'a str>>,
}

impl<'a> StatefulTable<'a> {
    fn new() -> StatefulTable<'a> {
        StatefulTable {
            state: TableState::default(),
            items: vec![
                vec!["Row11", "Row12", "Row13"],
                vec!["Row21", "Row22", "Row23"],
                vec!["Row31", "Row32", "Row33"],
                vec!["Row41", "Row42", "Row43"],
                vec!["Row51", "Row52", "Row53"],
                vec!["Row61", "Row62Test", "Row63"],
                vec!["Row71", "Row72", "Row73"],
                vec!["Row81", "Row82", "Row83"],
                vec!["Row91", "Row92", "Row93"],
                vec!["Row101", "Row102", "Row103"],
                vec!["Row111", "Row112", "Row113"],
                vec!["Row121", "Row122", "Row123"],
                vec!["Row131", "Row132", "Row133"],
                vec!["Row141", "Row142", "Row143"],
                vec!["Row151", "Row152", "Row153"],
                vec!["Row161", "Row162", "Row163"],
                vec!["Row171", "Row172", "Row173"],
                vec!["Row181", "Row182", "Row183"],
                vec!["Row191", "Row192", "Row193"],
            ],
        }
    }
    pub fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.items.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.items.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
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
    let selected_style = Style::default().add_modifier(Modifier::REVERSED);
    let normal_style = Style::default();
    let header_cells = [
        "ICAO", "CALL", "SQK", "ALT", "HDG", "GS", "VR", "POS", "LAST",
    ]
    .iter()
    .map(|h| Cell::from(*h).style(Style::default()));
    let header = Row::new(header_cells).style(normal_style);
    let rows = match tracker.get_most_recent_message_time() {
        Some(now) => {
            tracker
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
                .collect::<Vec<Row>>()
        }
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
        .highlight_style(selected_style)
        .highlight_symbol(">> ")
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
