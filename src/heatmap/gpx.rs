use chrono::{DateTime, Utc};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use simple_error::{bail, SimpleError};

pub fn get_pts(
    mut reader: Reader<&[u8]>,
    type_filter: &Option<super::ActivityType>,
    start: &Option<DateTime<Utc>>,
    end: &Option<DateTime<Utc>>,
) -> Result<Vec<super::TrkPt>, SimpleError> {
    let mut buf = Vec::new();

    let filter_string = match type_filter {
        Some(super::ActivityType::Bike) => Some("1"),
        Some(super::ActivityType::Run) => Some("9"),
        None => None,
    };

    let mut in_trk = false; // true if we're between a <trk> and </trk> tag (the bulk of the gpx file)
    let mut in_trkseg = false; // true if we're between a <trkseg> and </trkseg> tag

    let mut trk_pts = Vec::new();

    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) => match e.name() {
                b"metadata" => {
                    if start.is_some() || end.is_some() {
                        if let Some(time) = metadata_time(&mut reader)? {
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
                b"trk" => in_trk = true, // mark that we're within <trk> </trk>, which we will be for most of the file
                b"trkseg" => in_trkseg = true, // mark that we're within <trkseg> </trkseg>
                b"type" => {
                    if in_trk
                        && filter_string.is_some()
                        && !type_check(&mut reader, filter_string.unwrap())?
                    {
                        return Ok(Vec::new());
                    }
                }
                b"trkpt" => {
                    if !in_trk {
                        // we could ignore a <trkpt> outside of <trk> but this seems malformed so we error out
                        bail!("trkpt out of trk");
                    }
                    if !in_trkseg {
                        // we could ignore a <trkpt> outside of <trkseg> but this seems malformed so we error out
                        bail!("trkpt out of trkseg");
                    }

                    if let Some(trkpt) = trkpt(&mut reader, e)? {
                        trk_pts.push(trkpt);
                    }
                }
                _ => (),
            },
            Ok(Event::End(ref e)) => match e.name() {
                b"trk" => in_trk = false,
                b"trkseg" => in_trkseg = false,
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

fn metadata_time(reader: &mut Reader<&[u8]>) -> Result<Option<DateTime<Utc>>, SimpleError> {
    let mut buf = Vec::new();

    let mut in_time = false; // true if we're in a <time> tag (the next event should be the Text of the tag))
    let mut time = None;

    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) => {
                if let b"time" = e.name() {
                    in_time = true; // mark that we're in a <time> tag and the next Text event is the start time of the gpx
                }
            }
            Ok(Event::End(ref e)) => match e.name() {
                b"metadata" => return Ok(time),
                b"time" => in_time = false,
                _ => (),
            },
            Ok(Event::Text(e)) => {
                if in_time {
                    // if we're in <time> read and parse it
                    time = Some(
                        e.unescape_and_decode(&reader)
                            .unwrap()
                            .parse::<DateTime<Utc>>()
                            .expect("Error parsing timestamp from time"),
                    );
                }
            }
            Ok(Event::Eof) => bail!("Hit EOF while getting time from <metadata>"),
            Err(e) => bail!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }

        buf.clear();
    }
}

fn trkpt(
    reader: &mut Reader<&[u8]>,
    event: &BytesStart,
) -> Result<Option<super::TrkPt>, SimpleError> {
    let mut buf = Vec::new();

    let mut in_time = false; // true if we're in a <time> tag (the next event should be the Text of the tag))

    let mut lat: Option<f64> = None;
    let mut lng: Option<f64> = None;
    let mut time: Option<DateTime<Utc>> = None;

    // the <trkpt> tag has "lat" and "lon" attributes that we read and parse into floats
    for attr in event.attributes().map(Result::unwrap) {
        match attr.key {
            b"lat" => {
                lat = Some(
                    std::str::from_utf8(
                        &attr
                            .unescaped_value()
                            .expect("Error getting lat from trkpt"),
                    )
                    .expect("Error parsing lat into string")
                    .parse()
                    .expect("Error parsing f64 from lat"),
                )
            }
            b"lon" => {
                lng = Some(
                    std::str::from_utf8(
                        &attr
                            .unescaped_value()
                            .expect("Error getting lng from trkpt"),
                    )
                    .expect("Error parsing lng into string")
                    .parse()
                    .expect("Error parsing f64 from lng"),
                )
            }
            _ => (),
        }
    }

    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) => {
                if let b"time" = e.name() {
                    in_time = true; // mark that we're in a <time> tag and the next Text event is time for our trk_pt
                }
            }
            Ok(Event::End(ref e)) => match e.name() {
                b"time" => in_time = false,
                b"trkpt" => {
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
                _ => (),
            },
            Ok(Event::Text(e)) => {
                if in_time {
                    // if we're in <time> read and parse it for trk_pt
                    time = Some(
                        e.unescape_and_decode(&reader)
                            .unwrap()
                            .parse::<DateTime<Utc>>()
                            .expect("Error parsing timestamp from time"),
                    );
                }
            }
            Ok(Event::Eof) => bail!("Hit EOF while getting data from <trkpt>"),
            Err(e) => bail!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }

        buf.clear();
    }
}

fn type_check(reader: &mut Reader<&[u8]>, filter_string: &str) -> Result<bool, SimpleError> {
    let mut buf = Vec::new();

    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Text(e)) => {
                // check that segment type matches filter
                return Ok(e.unescape_and_decode(&reader).unwrap() == filter_string);
            }
            Ok(Event::Eof) => bail!("Hit EOF while checking <type>"),
            Err(e) => bail!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }

        buf.clear();
    }
}
