use adsb::ICAOAddress;
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use flight_tracker::{icao, Tracker};
use postgres::{Client, NoTls};
use std::io::Write;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::{collections::HashMap, io::BufReader};
use std::{io, panic, process};
use structopt::StructOpt;
use termion::event::Key;
use termion::input::TermRead;
use termion::raw::IntoRawMode;
use tui::{backend::TermionBackend, Terminal};
use ui::App;

mod ui;

const REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);

#[derive(StructOpt)]
#[structopt(about = "Track aircraft via ADSB")]
struct Cli {
    #[structopt(subcommand)]
    cmd: Command,
    #[structopt(
        name = "expire",
        help = "Number of seconds before removing stale entries",
        default_value = "120",
        short = "e",
        long = "expire"
    )]
    expire: i64,
    #[structopt(long)]
    interactive: bool,
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

enum InteractiveScreen {
    Screen1,
    Screen2,
}

fn main() -> Result<()> {
    let args = Cli::from_args();
    let tracker = Arc::new(Mutex::new(Tracker::new()));
    let _expire = Duration::seconds(args.expire);
    let mut app = App::new("Untitled Flight Tracker", tracker.clone());
    if args.interactive {
        let _reader = match args.cmd {
            Command::Stdin => read_from_stdin(tracker),
            Command::Tcp { host, port } => read_from_network(host, port, tracker),
            Command::Postgres { query } => read_from_postgres(tracker, query),
        };
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
        let mut screen = InteractiveScreen::Screen1;
        loop {
            terminal
                .draw(|f| match screen {
                    InteractiveScreen::Screen1 => {
                        ui::draw_screen_1(f, &app);
                    }
                    InteractiveScreen::Screen2 => {
                        ui::draw_screen_2(f, &app);
                    }
                })
                .expect("Unable to render");
            if app.should_quit {
                break;
            }
            thread::sleep(REFRESH_INTERVAL);
            let key = keys.next();
            if let Some(event) = key {
                match event? {
                    Key::Char('q') => {
                        app.should_quit = true;
                    }
                    Key::Char('1') => screen = InteractiveScreen::Screen1,
                    Key::Char('2') => screen = InteractiveScreen::Screen2,
                    _ => {}
                }
            }
        }
    } else {
        match args.cmd {
            Command::Postgres { query } => read_from_postgres2(tracker, query).unwrap(),
            _ => {
                panic!("No.");
            }
        };
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
                let time: DateTime<Utc> = row.get("timestamp");
                let data: Vec<u8> = row.get("data");
                let _ = tracker.update_with_binary(&data, time);
            }
        }
    })
}

fn read_from_postgres2(tracker: Arc<Mutex<Tracker>>, query: String) -> Result<()> {
    let mut client = Client::connect(
        "host=storage.local port=54322 user=orbital password=orbital",
        NoTls,
    )?;
    let mut trans = client.transaction().unwrap();
    let portal = trans.bind(query.as_str(), &[])?;
    let mut last_printed: HashMap<ICAOAddress, DateTime<Utc>> = HashMap::new();
    let min_print_duration = chrono::Duration::seconds(10);
    let mut num_rows = 1;
    while num_rows > 0 {
        println!(".");
        let result = trans.query_portal(&portal, 10000)?;
        let mut tracker = tracker.lock().unwrap();
        num_rows = 0;
        for row in result {
            num_rows += 1;
            let time: DateTime<Utc> = row.get("timestamp");
            let data: Vec<u8> = row.get("data");
            let (message, _) = adsb::parse_binary(&data)?;
            let i = icao(&message);
            tracker.update_with_message(message, &data, time);
            if tracker.get_num_messages() % 10000 == 0 {
                if let Some(msg_rate) = tracker.get_messages_per_second_real_time() {
                    // eprintln!("{:#?}", tracker.pos_update_times);
                    eprintln!(
                        "# {} messages total, {} messages/sec",
                        tracker.get_num_messages(),
                        msg_rate
                    )
                }
            }
            // println!("{:?}", tracker.map);
            // println!("{:?}", last_printed);
            if let Some(icao) = i {
                let print = match last_printed.get(&icao) {
                    Some(ts) => time.signed_duration_since(*ts) > min_print_duration,
                    None => true,
                };
                if print {
                    let ac = tracker.map.get(&icao).unwrap();
                    if let (Some(lat), Some(lon)) = (ac.latitude, ac.longitude) {
                        println!(
                            "{},{},{},{},{}",
                            time,
                            icao,
                            lat,
                            lon,
                            ac.altitude.unwrap()
                        );
                        last_printed.insert(icao, time);
                    }
                }
            }
        }
    }
    Ok(())
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
