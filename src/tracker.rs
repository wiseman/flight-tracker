use adsb::*;
use chrono::{Utc, Duration};
use std::collections::HashMap;
use MessageKind::*;

/// A tracked aircraft
#[derive(Debug, Clone)]
pub struct Aircraft {
    /// Unique 24-bit ICAO address assigned to an aircraft upon national registration
    pub icao_address: ICAOAddress,
    /// Current aircraft callsign
    pub callsign: Option<String>,
    /// Current altitude (feet)
    pub altitude: Option<u16>,
    /// Current heading (degrees)
    pub heading: Option<f64>,
    /// Current ground speed (knots)
    pub ground_speed: Option<f64>,
    /// Current vertical rate (feet per minute)
    pub vertical_rate: Option<i16>,
    /// Current latitude (degrees)
    pub latitude: Option<f64>,
    /// Current longitude (degrees)
    pub longitude: Option<f64>,
    /// Source for vertical rate information
    pub vertical_rate_source: Option<VerticalRateSource>,
    /// Timestamp for last received message
    pub last_seen: chrono::DateTime<Utc>,
    last_cpr_even: Option<CPRFrame>,
    last_cpr_odd: Option<CPRFrame>,
}

impl Aircraft {
    fn new(icao_address: ICAOAddress, time: chrono::DateTime<Utc>) -> Self {
        Aircraft {
            icao_address,
            callsign: None,
            altitude: None,
            heading: None,
            ground_speed: None,
            vertical_rate: None,
            latitude: None,
            longitude: None,
            vertical_rate_source: None,
            last_seen: time,
            last_cpr_even: None,
            last_cpr_odd: None,
        }
    }

    fn update_position(&mut self, cpr_frame: CPRFrame) {
        let last_parity = cpr_frame.parity.clone();
        match last_parity {
            Parity::Even => {
                self.last_cpr_even = Some(cpr_frame);
            }
            Parity::Odd => {
                self.last_cpr_odd = Some(cpr_frame);
            }
        }
        if let (Some(even), Some(odd)) = (&self.last_cpr_even, &self.last_cpr_odd) {
            let position = match last_parity {
                Parity::Even => cpr::get_position((&odd, &even)),
                Parity::Odd => cpr::get_position((&even, &odd)),
            };
            if let Some(Position {
                latitude,
                longitude,
            }) = position
            {
                self.latitude = Some(latitude);
                self.longitude = Some(longitude);
            }
        }
    }
}

/// Stores the set of currently tracked aircraft
#[derive(Default)]
pub struct Tracker {
    map: HashMap<ICAOAddress, Aircraft>,
    num_messages: u64,
    num_unknown_messages: u64,
    unknown_message_counts: HashMap<u8, u64>,
    known_message_counts: HashMap<u8, u64>,
}

impl Tracker {
    /// Create a new tracker
    pub fn new() -> Self {
        Tracker::default()
    }

    /// Update the tracker with a received ADSB message in AVR format
    pub fn update_with_avr(&mut self, frame: &str, time: chrono::DateTime<Utc>) -> Result<(), adsb::ParserError> {
        let (message, _) = adsb::parse_avr(frame)?;
        self.update_with_message(message, time);
        Ok(())
    }

    /// Update the tracker with a received ADSB message in binary format
    pub fn update_with_binary(&mut self, frame: &[u8], time: chrono::DateTime<Utc>) -> Result<(), adsb::ParserError> {
        let (message, _) = adsb::parse_binary(frame)?;
        self.update_with_message(message, time);
        Ok(())
    }

    fn update_unknown_message_statistics(&mut self, message: Message) {
        let df = message.downlink_format;
        *self.unknown_message_counts.entry(df).or_insert(0) += 1;
        self.num_unknown_messages += 1;
    }

    fn update_with_message(&mut self, message: Message, time: chrono::DateTime<Utc>) {
        use ADSBMessageKind::*;

        self.num_messages += 1;
        let (icao_address, kind) = match message {

            Message {
                kind: ADSBMessage {
                    icao_address, kind, ..
                },
                ..
            } => {
                *self.known_message_counts.entry(message.downlink_format).or_insert(0) += 1;
                (icao_address, kind)
            },
            _ => {
                self.update_unknown_message_statistics(message);
                return
            },
        };

        let aircraft = self
            .map
            .entry(icao_address)
            .or_insert_with(|| Aircraft::new(icao_address, time));

        match kind {
            AircraftIdentification { callsign, .. } => {
                aircraft.callsign = Some(callsign.trim().to_string());
            }
            AirbornePosition {
                altitude,
                cpr_frame,
            } => {
                aircraft.altitude = Some(altitude);
                aircraft.update_position(cpr_frame);
            }
            AirborneVelocity {
                heading,
                ground_speed,
                vertical_rate,
                vertical_rate_source,
            } => {
                aircraft.heading = Some(heading);
                aircraft.ground_speed = Some(ground_speed);
                aircraft.vertical_rate = Some(vertical_rate);
                aircraft.vertical_rate_source = Some(vertical_rate_source);
            }
        }

        aircraft.last_seen = time;
    }

    /// Get a list of aircraft last seen in the given interval
    pub fn get_current_aircraft(&self, interval: &Duration) -> Vec<&Aircraft> {
        let now = Utc::now();
        self.map
            .values()
            .filter(|a | now.signed_duration_since(a.last_seen) < *interval)
            .collect()
    }

    // Get a list of all tracked aircraft
    pub fn get_all_aircraft(&self) -> Vec<&Aircraft> {
        self.map.values().collect()
    }

    pub fn get_num_messages(&self) -> u64 {
        self.num_messages
    }

    pub fn get_num_unknown_messages(&self) -> u64 {
        self.num_unknown_messages
    }

    pub fn get_unknown_message_statistics(&self) -> &HashMap<u8, u64> {
        return &self.unknown_message_counts;
    }

    pub fn get_known_message_statistics(&self) -> &HashMap<u8, u64> {
        return &self.known_message_counts;
    }
}
