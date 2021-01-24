use anyhow::Result;
use chrono::{Duration, Utc};
use flight_tracker::Tracker;
use itertools::Itertools;
use postgres::{Client, NoTls};
use std::{io, panic, process};
use std::io::BufRead;
use std::io::BufReader;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::{cmp::min, fmt};
use structopt::StructOpt;
use termion::raw::IntoRawMode;
use tui::{backend::TermionBackend, Terminal};
use ui::App;
use std::io::{Write, stdout, stdin};
use termion::event::Key;
use termion::input::TermRead;

mod ui;

const REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);
const NA: &str = "";

#[derive(StructOpt)]
#[structopt(about = "Track aircraft via ADSB")]
struct Cli {
    #[structopt(subcommand)]
    cmd: Command,
    #[structopt(
        name = "expire",
        help = "Number of seconds before removing stale entries",
        default_value = "60",
        short = "e",
        long = "expire"
    )]
    expire: i64,
    #[structopt(
        name = "stats",
        help = "Show message statistics",
        short = "s",
        long = "stats"
    )]
    stats: bool,
}

#[derive(StructOpt)]
enum Command {
    #[structopt(about = "Read messages from stdin")]
    Stdin,
    #[structopt(about = "Read messages from a TCP server")]
    Tcp {
        #[structopt(help = "host")]
        host: String,
        #[structopt(help = "port", default_value = "30002")]
        port: u16,
    },
    Postgres {
        #[structopt(
            help = "SQL query",
            default_value = "SELECT timestamp, data FROM pings order by timestamp asc"
        )]
        query: String,
    },
}

fn main() -> Result<()> {
    let args = Cli::from_args();
    let tracker = Arc::new(Mutex::new(Tracker::new()));
    let expire = Duration::seconds(args.expire);
    let show_stats = args.stats;
    let mut app = App::new("Untitled Flight Tracker", tracker.clone());
    let reader = match args.cmd {
        Command::Stdin => read_from_stdin(tracker),
        Command::Tcp { host, port } => read_from_network(host, port, tracker),
        Command::Postgres { query } => read_from_postgres(tracker, query),
    };
    // let writer = write_output(app, expire, show_stats);

    let mut stdout = io::stdout()
        .into_raw_mode()
        .expect("Unable to switch stdout to raw mode");
    write!(stdout, "{}", termion::clear::All).expect("Unable to clear terminal");
        let backend = TermionBackend::new(stdout);
    let mut terminal = Terminal::new(backend).expect("Unable to initialize terminal");

    let orig_hook = panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        orig_hook(panic_info);
        println!("Aborting");
        process::exit(1);
    }));

    let stdin = termion::async_stdin();
    let mut keys = stdin.keys();
    loop {
        terminal.draw(|f| ui::draw(f, &mut app));
        if app.should_quit {
            break;
        }
        thread::sleep(REFRESH_INTERVAL);
        let key = keys.next();
        if let Some(event) = key {
            match event? {
                Key::Char('q') => {
                    app.should_quit = true;
                },
                _ => {}
            }
        }
    }
    Ok(())
}

fn read_from_stdin(tracker: Arc<Mutex<Tracker>>) -> JoinHandle<Result<()>> {
    thread::spawn(move || {
        let mut input = String::new();
        loop {
            let _ = io::stdin().read_line(&mut input)?;
            input = input.trim().to_string();
            let mut tracker = tracker.lock().unwrap();
            let _ = tracker.update_with_avr(&input, Utc::now());
            input.clear();
        }
    })
}

struct Ping {
    timestamp: chrono::DateTime<Utc>,
    data: Vec<u8>,
}

fn read_from_postgres(tracker: Arc<Mutex<Tracker>>, query: String) -> JoinHandle<Result<()>> {
    thread::spawn(move || {
        let mut client = Client::connect(
            "host=storage.local port=54322 user=orbital password=orbital",
            NoTls,
        )?;
        let mut trans = client.transaction().unwrap();
        let portal = trans.bind(query.as_str(), &[])?;
        loop {
            let result = trans.query_portal(&portal, 10000)?;
            let mut tracker = tracker.lock().unwrap();
            for row in result {
                let ping = Ping {
                    timestamp: row.get(0),
                    data: row.get(1),
                };
                let _ = tracker.update_with_binary(&ping.data, ping.timestamp);
            }
        }
    })
}

fn read_from_network(
    host: String,
    port: u16,
    tracker: Arc<Mutex<Tracker>>,
) -> JoinHandle<Result<()>> {
    thread::spawn(move || {
        let stream = TcpStream::connect((host.as_str(), port))?;
        let mut reader = BufReader::new(stream);
        let mut input = String::new();
        loop {
            let _ = std::io::BufRead::read_line(&mut reader, &mut input)?;
            let mut tracker = tracker.lock().unwrap();
            let _ = tracker.update_with_avr(&input, Utc::now());
            input.clear();
        }
    })
}

fn print_ascii_table(tracker: &Tracker, expire: &Duration) {
    // Clear screen
    print!("\x1B[2J\x1B[H");
    if let Some(now) = tracker.get_most_recent_message_time() {
        let aircraft_list = tracker.get_current_aircraft(expire, now);
        println!(
            "{:>27} {:>10} {:>9} {:>11}",
            "time", "# msgs", "# unk", "proc msgs/s"
        );
        println!(
            "{:>27} {:>10} {:>9} {:>11.1}",
            now,
            tracker.get_num_messages(),
            tracker.get_num_unknown_messages(),
            tracker.get_messages_per_second_real_time().unwrap_or(0.0)
        );
        println!(
            "{:>6} {:>10} {:>8} {:>6} {:>5} {:>8} {:>17} {:>5}",
            "icao", "call", "alt", "hdg", "gs", "vr", "lat/lon", "last"
        );
    }
}

fn print_message_stats(tracker: &Tracker) {
    // Clear screen
    print!("\x1B[2J\x1B[H");
    println!("---------- Known messages:");
    let counts = tracker.get_known_message_statistics();
    for df in counts.keys().sorted() {
        println!("{:>4} {:>9}", df, counts[df]);
    }
    println!("---------- Unknown messages:");
    let counts = tracker.get_unknown_message_statistics();
    for df in counts.keys().sorted() {
        println!("{:>4} {:>9}", df, counts[df]);
    }
    println!("---------- Unknown messages:");
    let datas = tracker.get_unknown_message_data();
    for df in datas.keys().sorted() {
        println!("{:>4} {:02X?}", df, datas[df]);
    }
}
