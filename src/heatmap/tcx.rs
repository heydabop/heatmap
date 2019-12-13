use chrono::{DateTime, Utc};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use simple_error::bail;
use std::error::Error;

pub fn get_pts(
    mut reader: Reader<&[u8]>,
    type_filter: &Option<super::ActivityType>,
    start: &Option<DateTime<Utc>>,
    end: &Option<DateTime<Utc>>,
) -> Result<Vec<super::TrkPt>, Box<dyn Error>> {
    let mut buf = Vec::new();

    let filter_string = match type_filter {
        Some(super::ActivityType::Bike) => Some("Biking"),
        Some(super::ActivityType::Run) => Some("Running"),
        Some(super::ActivityType::Walk) => Some("Other"),
        None => None,
    };

    let mut trk_pts = None;

    loop {
        buf.clear();

        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) => {
                if let b"Activity" = e.name() {
                    trk_pts = Some(parse_activity(&mut reader, &e, filter_string, start, end)?);
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => bail!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }
    }

    match trk_pts {
        Some(t) => Ok(t),
        None => Ok(Vec::new()),
    }
}

fn parse_activity(
    mut reader: &mut Reader<&[u8]>,
    event: &BytesStart,
    filter_string: Option<&str>,
    start: &Option<DateTime<Utc>>,
    end: &Option<DateTime<Utc>>,
) -> Result<Vec<super::TrkPt>, Box<dyn Error>> {
    let mut buf = Vec::new();

    let mut trk_pts = None;

    // Check if activity type matches provided filter
    if let Some(filter_string) = filter_string {
        for attr in event.attributes() {
            if let Ok(attr) = attr {
                if let b"Sport" = attr.key {
                    if filter_string != std::str::from_utf8(&attr.unescaped_value()?)? {
                        return Ok(Vec::new());
                    }
                }
            }
        }
    }

    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) => {
                if let b"Lap" = e.name() {
                    trk_pts = Some(parse_lap(&mut reader, &e, start, end)?);
                }
            }
            Ok(Event::End(ref e)) => {
                if let b"Activity" = e.name() {
                    match trk_pts {
                        Some(t) => return Ok(t),
                        None => return Ok(Vec::new()),
                    }
                }
            }
            Ok(Event::Eof) => bail!("Hit EOF while in <Activity>"),
            Err(e) => bail!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }

        buf.clear();
    }
}

fn parse_lap(
    mut reader: &mut Reader<&[u8]>,
    event: &BytesStart,
    start: &Option<DateTime<Utc>>,
    end: &Option<DateTime<Utc>>,
) -> Result<Vec<super::TrkPt>, Box<dyn Error>> {
    let mut buf = Vec::new();

    let mut trk_pts = None;

    // check file time if start or end filters are set
    if start.is_some() || end.is_some() {
        for attr in event.attributes() {
            if let Ok(attr) = attr {
                if let b"StartTime" = attr.key {
                    let time =
                        std::str::from_utf8(&attr.unescaped_value()?)?.parse::<DateTime<Utc>>()?;
                    // return no points if start time is before start or after end filters
                    if let Some(start) = start {
                        if time < *start {
                            return Ok(Vec::new());
                        }
                    }
                    if let Some(end) = end {
                        if time > *end {
                            return Ok(Vec::new());
                        }
                    }
                }
            }
        }
    }

    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) => {
                if let b"Track" = e.name() {
                    trk_pts = Some(parse_track(&mut reader, &mut buf)?);
                }
            }
            Ok(Event::End(ref e)) => {
                if let b"Lap" = e.name() {
                    match trk_pts {
                        Some(t) => return Ok(t),
                        None => return Ok(Vec::new()),
                    }
                }
            }
            Ok(Event::Eof) => bail!("Hit EOF while in <Lap>"),
            Err(e) => bail!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }

        buf.clear();
    }
}

fn parse_track(
    mut reader: &mut Reader<&[u8]>,
    mut buf: &mut Vec<u8>,
) -> Result<Vec<super::TrkPt>, Box<dyn Error>> {
    let mut trk_pts = Vec::new();

    loop {
        buf.clear();

        match reader.read_event(buf) {
            Ok(Event::Start(ref e)) => {
                if let b"Trackpoint" = e.name() {
                    match parse_trackpoint(&mut reader, &mut buf) {
                        Ok(pt) => trk_pts.push(pt),
                        Err(e) => eprintln!("{}", e),
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                if let b"Track" = e.name() {
                    return Ok(trk_pts);
                }
            }
            Ok(Event::Eof) => bail!("Hit EOF while in <Track>"),
            Err(e) => bail!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }
    }
}

fn parse_trackpoint(
    mut reader: &mut Reader<&[u8]>,
    mut buf: &mut Vec<u8>,
) -> Result<super::TrkPt, Box<dyn Error>> {
    let mut point = None;
    let mut time = None;

    loop {
        buf.clear();

        match reader.read_event(buf) {
            Ok(Event::Start(ref e)) => match e.name() {
                b"Position" => {
                    point = Some(parse_position(&mut reader, &mut buf)?);
                }
                b"Time" => match parse_time(&mut reader, &mut buf) {
                    Ok(t) => time = Some(t),
                    Err(e) => eprintln!("{}", e),
                },
                _ => (),
            },
            Ok(Event::End(ref e)) => {
                if let b"Trackpoint" = e.name() {
                    match point {
                        Some(center) => return Ok(super::TrkPt { center, time }),
                        None => bail!("Incomplete <Trackpoint>: {:?} {:?} ", point, time),
                    }
                }
            }
            Ok(Event::Eof) => bail!("Hit EOF while in <Trackpoint>"),
            Err(e) => bail!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }
    }
}

fn parse_position(
    mut reader: &mut Reader<&[u8]>,
    mut buf: &mut Vec<u8>,
) -> Result<super::Point, Box<dyn Error>> {
    let mut lat = None;
    let mut lng = None;

    loop {
        buf.clear();

        match reader.read_event(buf) {
            Ok(Event::Start(ref e)) => match e.name() {
                b"LatitudeDegrees" => {
                    lat = Some(parse_degrees(&mut reader, &mut buf)?);
                }
                b"LongitudeDegrees" => {
                    lng = Some(parse_degrees(&mut reader, &mut buf)?);
                }
                _ => (),
            },
            Ok(Event::End(ref e)) => {
                if let b"Position" = e.name() {
                    if let (Some(lat), Some(lng)) = (lat, lng) {
                        return Ok(super::Point { lat, lng });
                    } else {
                        bail!("Incomplete <Position>: {:?} {:?}", lat, lng);
                    }
                }
            }
            Ok(Event::Eof) => bail!("Hit EOF while in <trkseg>"),
            Err(e) => bail!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }
    }
}

fn parse_time(
    reader: &mut Reader<&[u8]>,
    buf: &mut Vec<u8>,
) -> Result<DateTime<Utc>, Box<dyn Error>> {
    loop {
        buf.clear();

        match reader.read_event(buf) {
            Ok(Event::Text(e)) => {
                // read and parse text value in <time>
                return e
                    .unescape_and_decode(&reader)?
                    .parse::<DateTime<Utc>>()
                    .or_else(|err| bail!("Error parsing timestamp from time: {}", err));
            }
            Ok(Event::End(ref e)) => {
                if let b"time" = e.name() {
                    bail!("No text in <time> tag");
                }
            }
            Ok(Event::Eof) => bail!("Hit EOF while in <time>"),
            Err(e) => bail!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }
    }
}

fn parse_degrees(reader: &mut Reader<&[u8]>, buf: &mut Vec<u8>) -> Result<f64, Box<dyn Error>> {
    loop {
        buf.clear();

        match reader.read_event(buf) {
            Ok(Event::Text(e)) => {
                // read and parse text value in <LatitudeDegrees> or <LongitudeDegrees>
                return e
                    .unescape_and_decode(&reader)?
                    .parse::<f64>()
                    .or_else(|e| bail!("Unable to parse degrees: {}", e));
            }
            Ok(Event::Eof) => bail!("Hit EOF while in degrees tag"),
            Err(e) => bail!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }
    }
}
