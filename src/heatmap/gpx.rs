use chrono::{DateTime, Utc};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use simple_error::bail;
use std::error::Error;

pub fn get_pts(
    mut reader: Reader<&[u8]>,
    type_filters: &Option<Vec<super::ActivityType>>,
    start: &Option<DateTime<Utc>>,
    end: &Option<DateTime<Utc>>,
) -> Result<Vec<super::TrkPt>, Box<dyn Error>> {
    let mut buf = Vec::new();

    let filter_strings = match type_filters {
        Some(fs) => Some(
            fs.iter()
                .map(|f| match f {
                    super::ActivityType::Bike => "1",
                    super::ActivityType::Run => "9",
                    super::ActivityType::Walk => "10",
                })
                .collect(),
        ),
        None => None,
    };

    let mut trk_pts = Vec::new();

    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) => match e.name() {
                b"metadata" => {
                    if start.is_some() || end.is_some() {
                        if let Some(ref time) = parse_metadata(&mut reader, &mut buf)? {
                            if let Some(start) = start {
                                if time < start {
                                    return Ok(Vec::new());
                                }
                            }
                            if let Some(end) = end {
                                if time > end {
                                    return Ok(Vec::new());
                                }
                            }
                        }
                    }
                }
                b"trk" => trk_pts = parse_trk(&mut reader, &mut buf, &filter_strings)?,
                _ => (),
            },
            Ok(Event::Eof) => break,
            Err(e) => bail!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }

        buf.clear();
    }

    Ok(trk_pts)
}

fn parse_metadata(
    mut reader: &mut Reader<&[u8]>,
    mut buf: &mut Vec<u8>,
) -> Result<Option<DateTime<Utc>>, Box<dyn Error>> {
    let mut time = None;

    loop {
        buf.clear();

        match reader.read_event(buf) {
            Ok(Event::Start(ref e)) => {
                if let b"time" = e.name() {
                    time = parse_time(&mut reader, &mut buf)?;
                }
            }
            Ok(Event::End(ref e)) => {
                if let b"metadata" = e.name() {
                    return Ok(time);
                }
            }
            Ok(Event::Eof) => bail!("Hit EOF while in <metadata>"),
            Err(e) => bail!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }
    }
}

fn parse_trkpt(
    mut reader: &mut Reader<&[u8]>,
    event: &BytesStart,
) -> Result<Option<super::TrkPt>, Box<dyn Error>> {
    let mut buf = Vec::new();

    let mut lat: Option<f64> = None;
    let mut lng: Option<f64> = None;
    let mut time: Option<DateTime<Utc>> = None;

    // the <trkpt> tag has "lat" and "lon" attributes that we read and parse into floats
    for attr in event.attributes() {
        if let Ok(attr) = attr {
            match attr.key {
                b"lat" => lat = Some(std::str::from_utf8(&attr.unescaped_value()?)?.parse()?),
                b"lon" => lng = Some(std::str::from_utf8(&attr.unescaped_value()?)?.parse()?),
                _ => (),
            }
        }
    }

    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) => {
                if let b"time" = e.name() {
                    time = parse_time(&mut reader, &mut buf)?;
                }
            }
            Ok(Event::End(ref e)) => {
                if let b"trkpt" = e.name() {
                    if lat.is_none() || lng.is_none() {
                        eprintln!("Incomplete <Trackpoint>: {:?} {:?} {:?}", lat, lng, time);
                        return Ok(None);
                    }
                    return Ok(Some(super::TrkPt {
                        center: super::Point {
                            lat: lat.unwrap(),
                            lng: lng.unwrap(),
                        },
                        time,
                    }));
                }
            }
            Ok(Event::Eof) => bail!("Hit EOF while in <trkpt>"),
            Err(e) => bail!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }

        buf.clear();
    }
}

fn parse_trk(
    mut reader: &mut Reader<&[u8]>,
    mut buf: &mut Vec<u8>,
    filter_strings: &Option<Vec<&str>>,
) -> Result<Vec<super::TrkPt>, Box<dyn Error>> {
    let mut trk_pts = Vec::new();

    loop {
        buf.clear();

        match reader.read_event(buf) {
            Ok(Event::Start(ref e)) => match e.name() {
                b"trkseg" => trk_pts = parse_trkseg(&mut reader, &mut buf)?,
                b"type" => {
                    if filter_strings.is_some()
                        && !type_check(&mut reader, &mut buf, filter_strings.as_ref().unwrap())?
                    {
                        return Ok(Vec::new());
                    }
                }
                _ => (),
            },
            Ok(Event::End(ref e)) => {
                if let b"trk" = e.name() {
                    return Ok(trk_pts);
                }
            }
            Ok(Event::Eof) => bail!("Hit EOF while in <trk>"),
            Err(e) => bail!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }
    }
}

fn parse_trkseg(
    mut reader: &mut Reader<&[u8]>,
    buf: &mut Vec<u8>,
) -> Result<Vec<super::TrkPt>, Box<dyn Error>> {
    let mut trk_pts = Vec::new();

    loop {
        buf.clear();

        match reader.read_event(buf) {
            Ok(Event::Start(ref e)) => {
                if let b"trkpt" = e.name() {
                    if let Some(trkpt) = parse_trkpt(&mut reader, e)? {
                        trk_pts.push(trkpt);
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                if let b"trkseg" = e.name() {
                    return Ok(trk_pts);
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
) -> Result<Option<DateTime<Utc>>, Box<dyn Error>> {
    let mut time = None;

    loop {
        buf.clear();

        match reader.read_event(buf) {
            Ok(Event::Text(e)) => {
                // read and parse text value in <time>
                time = Some(match e.unescape_and_decode(&reader) {
                    Ok(s) => s.parse::<DateTime<Utc>>()?,
                    Err(e) => return Err(Box::new(e)),
                });
            }
            Ok(Event::End(ref e)) => {
                if let b"time" = e.name() {
                    return Ok(time);
                }
            }
            Ok(Event::Eof) => bail!("Hit EOF while in <time>"),
            Err(e) => bail!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }
    }
}

fn type_check(
    reader: &mut Reader<&[u8]>,
    buf: &mut Vec<u8>,
    filter_strings: &[&str],
) -> Result<bool, Box<dyn Error>> {
    loop {
        buf.clear();

        match reader.read_event(buf) {
            Ok(Event::Text(e)) => {
                // check that segment type matches filter
                return Ok(match e.unescape_and_decode(&reader) {
                    Ok(s) => filter_strings.contains(&&s[..]),
                    Err(e) => return Err(Box::new(e)),
                });
            }
            Ok(Event::Eof) => bail!("Hit EOF while checking <type>"),
            Err(e) => bail!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }
    }
}
