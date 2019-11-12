use chrono::{DateTime, Utc};
use quick_xml::events::Event;
use quick_xml::Reader;

pub struct TrkPt {
    pub lat: f64,
    pub lng: f64,
    pub time: Option<DateTime<Utc>>,
}

pub fn get_pts(gpx: &str) -> Vec<TrkPt> {
    let mut reader = Reader::from_str(&gpx);
    reader.trim_text(true);

    let mut buf = Vec::new();

    let mut in_trk = false;
    let mut in_time = false;

    let mut curr_trk_pt: Option<&mut TrkPt> = None;
    let mut trk_pts = Vec::new();

    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) => match e.name() {
                b"trk" => in_trk = true,
                b"trkpt" => {
                    if !in_trk {
                        panic!("trkpt out of trk");
                    }
                    if curr_trk_pt.is_some() {
                        panic!("nested trkpt");
                    }

                    let mut lng = 0.0;
                    let mut lat = 0.0;
                    for attr in e.attributes().map(Result::unwrap) {
                        match attr.key {
                            b"lat" => {
                                lat = std::str::from_utf8(
                                    &attr
                                        .unescaped_value()
                                        .expect("Error getting lat from trkpt"),
                                )
                                .expect("Error parsing lat into string")
                                .parse()
                                .expect("Error parsing f64 from lat")
                            }
                            b"lon" => {
                                lng = std::str::from_utf8(
                                    &attr
                                        .unescaped_value()
                                        .expect("Error getting lng from trkpt"),
                                )
                                .expect("Error parsing lng into string")
                                .parse()
                                .expect("Error parsing f64 from lng")
                            }
                            _ => (),
                        }
                    }

                    trk_pts.push(TrkPt {
                        lat,
                        lng,
                        time: None,
                    });

                    curr_trk_pt = trk_pts.last_mut();
                }
                b"time" => {
                    if curr_trk_pt.is_none() {
                        eprintln!("time outside of trkpt");
                        continue;
                    }
                    in_time = true;
                }
                _ => (),
            },
            Ok(Event::End(ref e)) => match e.name() {
                b"trk" => in_trk = false,
                b"time" => in_time = false,
                b"trkpt" => {
                    curr_trk_pt = None;
                }
                _ => (),
            },
            Ok(Event::Text(e)) => {
                if in_time {
                    curr_trk_pt.as_mut().unwrap().time = Some(
                        e.unescape_and_decode(&reader)
                            .unwrap()
                            .parse::<DateTime<Utc>>()
                            .expect("Error parsing timestamp from time"),
                    )
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }

        buf.clear();
    }

    trk_pts
}

pub fn haversine(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    let r = 6371e3; // earth mean radius in meters

    let lat_rad_1 = lat1.to_radians();
    let lat_rad_2 = lat2.to_radians();
    let lat_delta = (lat2 - lat1).to_radians();
    let lng_delta = (lng2 - lng1).to_radians();

    let lat_sin = (lat_delta / 2.0).sin();
    let lng_sin = (lng_delta / 2.0).sin();

    let a = lat_sin.mul_add(
        lat_sin,
        lat_rad_1.cos() * lat_rad_2.cos() * lng_sin * lng_sin,
    );
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());

    r * c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haversine_test() {
        assert!(
            (haversine(31.2626, -100.3555, 38.1345, -89.6150) - 1_242_682.405_520_137_2).abs()
                < std::f64::EPSILON
        );
    }
}
