use anyhow::Result;
use chrono::{Utc, Duration};
use flight_tracker::Tracker;
use postgres::{Client, NoTls};
use std::fmt;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use structopt::StructOpt;
use itertools::Itertools;

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
    Postgres,
}

fn main() -> Result<()> {
    let args = Cli::from_args();
    let tracker = Arc::new(Mutex::new(Tracker::new()));
    let expire = Duration::seconds(args.expire);
    let writer = write_output(tracker.clone(), expire);
    let reader = match args.cmd {
        Command::Stdin => read_from_stdin(tracker),
        Command::Tcp { host, port } => read_from_network(host, port, tracker),
        Command::Postgres => read_from_postgres(tracker),
    };

    reader.join().unwrap()?;
    writer.join().unwrap()?;

    Ok(())
}

fn read_from_stdin(tracker: Arc<Mutex<Tracker>>) -> JoinHandle<Result<()>> {
    thread::spawn(move || {
        let mut input = String::new();
        loop {
            let _ = io::stdin().read_line(&mut input)?;
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

fn read_from_postgres(tracker: Arc<Mutex<Tracker>>) -> JoinHandle<Result<()>> {
    thread::spawn(move || {
        let mut client = Client::connect(
            "host=storage.local port=54322 user=orbital password=orbital",
            NoTls,
        )?;
        let mut trans = client.transaction().unwrap();
        let portal = trans.bind(
            "SELECT timestamp, data FROM pings order by timestamp asc",
            &[],
        )?;
        loop {
            let result = trans.query_portal(&portal, 10000)?;
            for row in result {
                let ping = Ping {
                    timestamp: row.get(0),
                    data: row.get(1),
                };
                let mut tracker = tracker.lock().unwrap();
                let _ = tracker.update_with_binary(&ping.data, Utc::now());
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
            let _ = reader.read_line(&mut input)?;
            let mut tracker = tracker.lock().unwrap();
            let _ = tracker.update_with_avr(&input, Utc::now());
            input.clear();
        }
    })
}

fn write_output(tracker: Arc<Mutex<Tracker>>, expire: Duration) -> JoinHandle<Result<()>> {
    thread::spawn(move || loop {
        thread::sleep(REFRESH_INTERVAL);
        let tracker = tracker.lock().unwrap();
        // print_ascii_table(&tracker, &expire);
        print_message_stats(&tracker);
    })
}

fn fmt_value<T: fmt::Display>(value: Option<T>, precision: usize) -> String {
    value
        .map(|v| format!("{:.1$}", v, precision))
        .unwrap_or_else(|| NA.to_string())
}

fn print_ascii_table(tracker: &Tracker, expire: &Duration) {
    let aircraft_list = tracker.get_current_aircraft(expire);
    // Clear screen
    print!("\x1B[2J\x1B[H");
    println!(
        "{:>6} {:>10} {:>8} {:>6} {:>5} {:>8} {:>17} {:>5} {:>6} {:>10} {:>10}",
        "icao", "call", "alt", "hdg", "gs", "vr", "lat/lon", "last",
        aircraft_list.len(),
        tracker.get_num_messages(),
        tracker.get_num_unknown_messages()
    );
    println!("{}", "-".repeat(72));
    let now = Utc::now();
    for aircraft in aircraft_list {
        println!(
            "{:>6} {:>10} {:>8} {:>6} {:>5} {:>8} {:>8},{:>8} {:>5}",
            aircraft.icao_address,
            aircraft.callsign.clone().unwrap_or_else(|| NA.to_string()),
            fmt_value(aircraft.altitude, 0),
            fmt_value(aircraft.heading, 0),
            fmt_value(aircraft.ground_speed, 0),
            fmt_value(aircraft.vertical_rate, 0),
            fmt_value(aircraft.latitude, 4),
            fmt_value(aircraft.longitude, 4),
            now.signed_duration_since(aircraft.last_seen).num_seconds()
        );
    }
}

fn print_message_stats(tracker: &Tracker) {
    // Clear screen
    print!("\x1B[2J\x1B[H");
    println!("Unknown messages:");
    let counts = tracker.get_unknown_message_statistics();
    for df in counts.keys().sorted() {
        println!("{:>4} {:>9}", df, counts[df]);
    }
    println!("Known messages:");
    let counts = tracker.get_known_message_statistics();
    for df in counts.keys().sorted() {
        println!("{:>4} {:>9}", df, counts[df]);
    }
}
