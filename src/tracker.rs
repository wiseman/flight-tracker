use adsb::*;
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use MessageKind::*;

/// A tracked aircraft
#[derive(Debug, Clone)]
pub struct Aircraft {
    /// Unique 24-bit ICAO address assigned to an aircraft upon national registration
    pub icao_address: ICAOAddress,
    /// Current aircraft callsign
    pub callsign: Option<String>,
    /// Squawk code
    pub squawk: Option<Squawk>,
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
    pub last_seen: DateTime<Utc>,
    last_cpr_even: Option<(CPRFrame, DateTime<Utc>)>,
    last_cpr_odd: Option<(CPRFrame, DateTime<Utc>)>,
    last_pos_seen: Option<DateTime<Utc>>
}

impl Aircraft {
    fn new(icao_address: ICAOAddress, time: DateTime<Utc>) -> Self {
        Aircraft {
            icao_address,
            callsign: None,
            squawk: None,
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
            last_pos_seen: None,
        }
    }

    fn update_position(&mut self, cpr_frame: CPRFrame, when: DateTime<Utc>) -> Option<Duration> {
        let last_parity = cpr_frame.parity.clone();
        match last_parity {
            Parity::Even => {
                self.last_cpr_even = Some((cpr_frame, when));
            }
            Parity::Odd => {
                self.last_cpr_odd = Some((cpr_frame, when));
            }
        }

        if let (Some((even, even_time)), Some((odd, odd_time))) =
            (&self.last_cpr_even, &self.last_cpr_odd)
        {
            let delta = even_time.signed_duration_since(*odd_time);
            if delta.num_seconds().abs() <= 30 {
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
                    let last_pos_seen = self.last_pos_seen;
                    self.last_pos_seen = Some(when);
                    if let Some(last_pos_seen) = last_pos_seen {
                        Some(when.signed_duration_since(last_pos_seen))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    }
}

/// Stores the set of currently tracked aircraft
#[derive(Default)]
pub struct Tracker {
    pub map: HashMap<ICAOAddress, Aircraft>,
    num_messages: u64,
    num_unknown_messages: u64,
    unknown_message_counts: HashMap<u8, u64>,
    unknown_message_data: HashMap<u8, Vec<u8>>,
    known_message_counts: HashMap<u8, u64>,
    most_recent_message_time: Option<DateTime<Utc>>,
    first_message_real_time: Option<DateTime<Utc>>,
    most_recent_message_real_time: Option<DateTime<Utc>>,
    pub pos_update_times: HashMap<i64, u32>,
}

pub fn parse_avr(frame: &str) -> Result<Vec<u8>, std::num::ParseIntError> {
    (1..frame.len() - 1)
        .step_by(2)
        .map(|i| u8::from_str_radix(&frame[i..i + 2], 16))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_avr() {
        let v = vec![141, 164, 109, 79, 153, 21, 88, 24, 168, 4, 64, 117, 211, 43];
        assert_eq!(parse_avr("*8DA46D4F99155818A8044075D32B;"), Ok(v));
    }
}

impl Tracker {
    /// Create a new tracker
    pub fn new() -> Self {
        Tracker::default()
    }

    /// Update the tracker with a received ADSB message in AVR format
    pub fn update_with_avr(
        &mut self,
        frame: &str,
        time: DateTime<Utc>,
    ) -> Result<(), adsb::ParserError> {
        let (message, _) = adsb::parse_avr(frame)?;
        // println!("{:?}", message);
        let data = parse_avr(frame);
        match data {
            Ok(data) => {
                self.update_with_message(message, &data, time);
                Ok(())
            }
            Err(err) => {
                println!("{:?}", err);
                panic!("Done");
            }
        }
    }

    /// Update the tracker with a received ADSB message in binary format
    pub fn update_with_binary(
        &mut self,
        frame: &[u8],
        time: DateTime<Utc>,
    ) -> Result<(), adsb::ParserError> {
        let (message, _) = adsb::parse_binary(frame)?;
        self.update_with_message(message, frame, time);
        Ok(())
    }

    fn update_unknown_message_statistics(&mut self, message: &Message, data: &[u8]) {
        let df = message.downlink_format;
        *self.unknown_message_counts.entry(df).or_insert(0) += 1;
        self.unknown_message_data.insert(df, Vec::from(data));
        self.num_unknown_messages += 1;
    }

    pub fn update_with_message(
        &mut self,
        message: Message,
        data: &[u8],
        time: DateTime<Utc>,
    ) {
        // println!("{:>10} {:?} {:02X?}", self.get_num_messages(), time, data);
        let now = Utc::now();
        self.num_messages += 1;
        let icao_address = match message.kind {
            ADSBMessage { icao_address, .. } => {
                *self
                    .known_message_counts
                    .entry(message.downlink_format)
                    .or_insert(0) += 1;
                icao_address
            }
            ModeSMessage { icao_address, .. } => {
                // if !self.map.contains_key(&icao_address) {
                //     println!("{}", icao_address);
                //     println!("{:?}", message.kind);
                // }
                *self
                    .known_message_counts
                    .entry(message.downlink_format)
                    .or_insert(0) += 1;
                icao_address
            }
            Unknown => {
                self.update_unknown_message_statistics(&message, data);
                return;
            }
        };

        let aircraft = self
            .map
            .entry(icao_address)
            .or_insert_with(|| Aircraft::new(icao_address, time));

        match message.kind {
            ADSBMessage {
                kind: ADSBMessageKind::AircraftIdentification { callsign, .. },
                ..
            } => {
                aircraft.callsign = Some(callsign.trim().to_string());
            }
            ADSBMessage {
                kind:
                    ADSBMessageKind::AirbornePosition {
                        altitude,
                        cpr_frame,
                    },
                ..
            } => {
                aircraft.altitude = Some(altitude);
                let update_duration = aircraft.update_position(cpr_frame, time);
                if let Some(dur) = update_duration {
                    let ms = 100 * (dur.num_milliseconds() / 100);
                    let count = self.pos_update_times.entry(ms).or_insert(0);
                    *count += 1;
                }
            }
            ADSBMessage {
                kind:
                    ADSBMessageKind::AirborneVelocity {
                        heading,
                        ground_speed,
                        vertical_rate,
                        vertical_rate_source,
                    },
                ..
            } => {
                aircraft.heading = Some(heading);
                aircraft.ground_speed = Some(ground_speed);
                aircraft.vertical_rate = Some(vertical_rate);
                aircraft.vertical_rate_source = Some(vertical_rate_source);
            }
            ModeSMessage {
                kind: ModeSMessageKind::SurveillanceIdentity { squawk, .. },
                ..
            } => {
                aircraft.squawk = Some(squawk);
            }
            Unknown => {}
        }
        aircraft.last_seen = time;

        match self.most_recent_message_time {
            Some(most_recent_time) => {
                if most_recent_time < time {
                    self.most_recent_message_time = Some(time);
                }
            }
            None => {
                self.most_recent_message_time = Some(time);
            }
        };
        if self.first_message_real_time.is_none() {
            self.first_message_real_time = Some(now);
        }
        self.most_recent_message_real_time = Some(now);
    }

    /// Get a list of aircraft last seen in the given interval
    pub fn get_current_aircraft(
        &self,
        interval: &Duration,
        now: DateTime<Utc>,
    ) -> Vec<&Aircraft> {
        self.map
            .values()
            .filter(|a| now.signed_duration_since(a.last_seen) < *interval)
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
        &self.unknown_message_counts
    }

    pub fn get_unknown_message_data(&self) -> &HashMap<u8, Vec<u8>> {
        &self.unknown_message_data
    }

    pub fn get_known_message_statistics(&self) -> &HashMap<u8, u64> {
        &self.known_message_counts
    }

    pub fn get_most_recent_message_time(&self) -> Option<DateTime<Utc>> {
        self.most_recent_message_time
    }

    pub fn get_messages_per_second_real_time(&self) -> Option<f64> {
        match self.first_message_real_time {
            Some(start) => match self.most_recent_message_real_time {
                Some(end) => Some(
                    1000.0 * (self.num_messages as f64)
                        / (end.signed_duration_since(start).num_milliseconds() as f64),
                ),
                None => None,
            },
            None => None,
        }
    }
}

pub fn icao(message: &Message) -> Option<ICAOAddress> {
    match message.kind {
        ADSBMessage { icao_address, .. } => Some(icao_address),
        ModeSMessage { icao_address, .. } => Some(icao_address),
        Unknown => None,
    }
}
