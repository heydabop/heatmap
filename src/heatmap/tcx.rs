use chrono::{DateTime, Utc};
use quick_xml::events::Event;
use quick_xml::Reader;
use simple_error::{bail, SimpleError};

#[allow(clippy::too_many_lines)]
pub fn get_pts(
    mut reader: Reader<&[u8]>,
    type_filter: &Option<super::ActivityType>,
) -> Result<Vec<super::TrkPt>, SimpleError> {
    let mut buf = Vec::new();

    let filter_string = match type_filter {
        Some(super::ActivityType::Bike) => Some("Biking"),
        Some(super::ActivityType::Run) => Some("Running"),
        None => None,
    };

    // The following bools track if we're between a given start an end event
    let mut in_activities = false; // true if we're between a <Activites> and </Activites> tag (the bulk of the tcx file)
    let mut in_activity = false; // etc
    let mut in_lap = false;
    let mut in_track = false;
    let mut in_trackpoint = false;
    let mut in_time = false;
    let mut in_position = false;
    let mut in_latitude_degrees = false;
    let mut in_longitude_degrees = false;

    let mut trk_pts = Vec::new();

    let mut curr_lat: Option<f64> = None;
    let mut curr_lng: Option<f64> = None;
    let mut curr_time: Option<DateTime<Utc>> = None;

    loop {
        buf.clear();
        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) => match e.name() {
                b"Activities" => in_activities = true,
                b"Activity" => {
                    if !in_activities {
                        bail!("<Activity> out of <Activites>");
                    }
                    in_activity = true;

                    if let Some(filter_string) = filter_string {
                        for attr in e.attributes().map(Result::unwrap) {
                            if let b"Sport" = attr.key {
                                if filter_string
                                    != std::str::from_utf8(
                                        &attr
                                            .unescaped_value()
                                            .expect("Error getting Sport from Activity"),
                                    )
                                    .expect("Error parsing Sport into string")
                                {
                                    return Ok(Vec::new());
                                }
                            }
                        }
                    }
                }
                b"Lap" => {
                    if !in_activity {
                        bail!("<Lap> out of <Activity>");
                    }
                    in_lap = true;
                }
                b"Track" => {
                    if !in_lap {
                        bail!("<Track> out of <Lap>");
                    }
                    in_track = true;
                }
                b"Trackpoint" => {
                    if !in_track {
                        bail!("<Trackpoint> out of <Track>");
                    }
                    in_trackpoint = true;
                }
                b"Time" => {
                    if !in_trackpoint {
                        bail!("<Time> out of <Trackpoint>");
                    }
                    in_time = true;
                }
                b"Position" => {
                    if !in_trackpoint {
                        bail!("<Position> out of <Trackpoint>");
                    }
                    in_position = true;
                }
                b"LatitudeDegrees" => {
                    if !in_position {
                        bail!("<LatitudeDegrees> out of <Position>");
                    }
                    in_latitude_degrees = true;
                }
                b"LongitudeDegrees" => {
                    if !in_position {
                        bail!("<LongitudeDegrees> out of <Position>");
                    }
                    in_longitude_degrees = true;
                }
                _ => (),
            },
            Ok(Event::End(ref e)) => match e.name() {
                b"Activites" => in_activities = false,
                b"Activity" => in_activity = false,
                b"Lap" => in_lap = false,
                b"Track" => in_track = false,
                b"Trackpoint" => {
                    in_trackpoint = false;
                    if curr_lat.is_none() || curr_lng.is_none() || curr_time.is_none() {
                        eprintln!(
                            "Incomplete <Trackpoint>: {:?} {:?} {:?}",
                            curr_lat, curr_lng, curr_time
                        );
                        continue;
                    }
                    trk_pts.push(super::TrkPt {
                        center: super::Point {
                            lat: curr_lat.take().unwrap(),
                            lng: curr_lng.take().unwrap(),
                        },
                        time: curr_time.take().unwrap(),
                    });
                }
                b"Time" => in_time = false,
                b"Position" => in_position = false,
                b"LatitudeDegrees" => in_latitude_degrees = false,
                b"LongitudeDegrees" => in_longitude_degrees = false,
                _ => (),
            },
            Ok(Event::Text(e)) => {
                if in_latitude_degrees {
                    curr_lat = Some(
                        e.unescape_and_decode(&reader)
                            .unwrap()
                            .parse::<f64>()
                            .expect("Error parsing f64 from <LatitudeDegrees>"),
                    );
                }
                if in_longitude_degrees {
                    curr_lng = Some(
                        e.unescape_and_decode(&reader)
                            .unwrap()
                            .parse::<f64>()
                            .expect("Error parsing f64 from <LongitudeDegrees>"),
                    );
                }
                if in_time {
                    curr_time = Some(
                        e.unescape_and_decode(&reader)
                            .unwrap()
                            .parse::<DateTime<Utc>>()
                            .expect("Error parsing timestamp from <Time>"),
                    );
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => bail!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }
    }

    Ok(trk_pts)
}
