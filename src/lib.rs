use chrono::{DateTime, Utc};
use quick_xml::events::Event;
use quick_xml::Reader;

pub struct Point {
    pub lat: f64,
    pub lng: f64,
}

pub struct TrkPt {
    pub center: Point,
    pub time: Option<DateTime<Utc>>,
}

pub struct MapInfo {
    pub center: Point,
    pub zoom: f64,
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
                        center: Point { lat, lng },
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

pub fn haversine(p1: &Point, p2: &Point) -> f64 {
    let r = 6371e3; // earth mean radius in meters

    let lat_rad_1 = p1.lat.to_radians();
    let lat_rad_2 = p2.lat.to_radians();
    let lat_delta = (p2.lat - p1.lat).to_radians();
    let lng_delta = (p2.lng - p1.lng).to_radians();

    let lat_sin = (lat_delta / 2.0).sin();
    let lng_sin = (lng_delta / 2.0).sin();

    let a = lat_sin.mul_add(
        lat_sin,
        lat_rad_1.cos() * lat_rad_2.cos() * lng_sin * lng_sin,
    );
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());

    r * c
}

pub fn calculate_map(pixels: u32, min: &Point, max: &Point) -> MapInfo {
    let lat = min.lat + (max.lat - min.lat) / 2.0;
    let lng = min.lng + (max.lng - min.lng) / 2.0;

    let map_width_meters = haversine(&Point { lat, lng: min.lng }, &Point { lat, lng: max.lng });
    let map_height_meters = haversine(&Point { lat: min.lat, lng }, &Point { lat: max.lat, lng });
    let map_meters = map_height_meters.max(map_width_meters);

    println!("meters: {}, {}", map_width_meters, map_height_meters);

    let meters_per_pixel = map_meters / f64::from(pixels);
    println!("meters per pixel: {}", meters_per_pixel);

    let zoom = ((10_018_755.0 * lat.to_radians().cos()) / meters_per_pixel).ln()
        / std::f64::consts::LN_2
        - 7.0;

    MapInfo {
        center: Point { lat, lng },
        zoom,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haversine_test() {
        let p1 = Point {
            lat: 31.2626,
            lng: -100.3555,
        };
        let p2 = Point {
            lat: 38.1345,
            lng: -89.6150,
        };
        assert!((haversine(&p1, &p2) - 1_242_682.405_520_137_2).abs() < std::f64::EPSILON);
    }

    #[test]
    fn wide_map() {
        let map_info = calculate_map(
            800,
            &Point {
                lat: 33.989_316,
                lng: -118.500_123,
            },
            &Point {
                lat: 34.105_721,
                lng: -118.246_575,
            },
        );
        assert!((map_info.center.lat - 34.047_518_5).abs() < std::f64::EPSILON);
        assert!((map_info.center.lng + 118.373_349).abs() < std::f64::EPSILON);
        assert!((map_info.zoom - 11.116_994_223_947_59).abs() < std::f64::EPSILON);
    }

    #[test]
    fn tall_map() {
        let map_info = calculate_map(
            800,
            &Point {
                lat: 33.979_119,
                lng: -118.497_911,
            },
            &Point {
                lat: 34.016_201,
                lng: -118.462_638,
            },
        );
        assert!((map_info.center.lat - 33.997_659_999_999_996).abs() < std::f64::EPSILON);
        assert!((map_info.center.lng + 118.480_274_500_000_01).abs() < std::f64::EPSILON);
        assert!((map_info.zoom - 13.620_010_920_097_176).abs() < std::f64::EPSILON);
    }
}
