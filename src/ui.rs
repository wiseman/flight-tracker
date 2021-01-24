use chrono::Duration;
use flight_tracker::Tracker;
use std::sync::{Arc, Mutex};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table, TableState, Tabs, Wrap},
    Frame,
};

const NA: &str = "";

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

pub fn draw<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let rects = Layout::default()
        .constraints([Constraint::Percentage(100)].as_ref())
        .margin(0)
        .split(f.size());
    let selected_style = Style::default().add_modifier(Modifier::REVERSED);
    let normal_style = Style::default();
    let header_cells = ["icao", "call", "alt"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default()));
    let header = Row::new(header_cells).style(normal_style);
    let tracker = app.tracker.lock().unwrap();
    let rows = match tracker.get_most_recent_message_time() {
        Some(now) => {
            tracker
                .get_current_aircraft(&Duration::seconds(60), now)
                .iter()
                .map(|a| {
                    let aircraft = *a;
                    let fields = vec![
                        format!("{:?}", aircraft.icao_address),
                        "123".to_string(),
                        // *aircraft.callsign.unwrap_or("".to_string()),
                        format!("{:?}", aircraft.altitude),
                    ];
                    Row::new(fields.iter().map(|item| Cell::from(item.clone())))
                    // let cells = vec![
                    //     format!("{:?}", aircraft.icao_address),
                    //     aircraft.callsign.unwrap_or("".to_string()),
                    //     format!("{:?}", aircraft.altitude),
                    // ]
                    // .iter()
                    // .map(|item| Cell::from(*item));
                    // Row::new(cells)
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
        .block(Block::default().borders(Borders::ALL).title("Table"))
        .highlight_style(selected_style)
        .highlight_symbol(">> ")
        .widths(&[
            Constraint::Percentage(50),
            Constraint::Length(30),
            Constraint::Max(10),
        ]);
    f.render_widget(t, rects[0]);
}

fn draw_aircraft_table<B>(f: &mut Frame<B>, area: Rect)
where
    B: Backend,
{
    let text = vec![Spans::from("Hello!")];
    let block = Block::default().borders(Borders::ALL).title(Span::styled(
        "Footer",
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
    ));
    let paragraph = Paragraph::new(text).block(block).wrap(Wrap { trim: true });
    f.render_widget(paragraph, area);
}
