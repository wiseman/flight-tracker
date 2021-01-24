use anyhow::Result;
use chrono::{Duration, Utc};
use flight_tracker::Tracker;
use postgres::{Client, NoTls};
use std::io::BufReader;
use std::io::Write;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::{io, panic, process};
use structopt::StructOpt;
use termion::event::Key;
use termion::input::TermRead;
use termion::raw::IntoRawMode;
use tui::{backend::TermionBackend, Terminal};
use ui::App;

mod ui;

const REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);

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
    let _expire = Duration::seconds(args.expire);
    let mut app = App::new("Untitled Flight Tracker", tracker.clone());
    let _reader = match args.cmd {
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
        terminal
            .draw(|f| ui::draw(f, &app))
            .expect("Unable to render");
        if app.should_quit {
            break;
        }
        thread::sleep(REFRESH_INTERVAL);
        let key = keys.next();
        if let Some(event) = key {
            if let Key::Char('q') = event? {
                app.should_quit = true;
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
