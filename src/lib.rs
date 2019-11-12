use chrono::{DateTime, NaiveDateTime, Utc};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::fmt;

#[derive(PartialEq)]
pub struct Point {
    pub lat: f64,
    pub lng: f64,
}

impl fmt::Debug for Point {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}, {}", self.lat, self.lng)
    }
}

#[derive(PartialEq)]
pub struct TrkPt {
    pub center: Point,
    pub time: Option<DateTime<Utc>>,
}

impl fmt::Debug for TrkPt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} @ {}",
            self.center,
            self.time
                .unwrap_or_else(|| DateTime::<Utc>::from_utc(
                    NaiveDateTime::from_timestamp(0, 0),
                    Utc
                ))
                .to_rfc3339()
        )
    }
}

pub struct MapInfo {
    pub center: Point,
    pub min: Point,
    pub zoom: f64,
    pub scale: f64,
}

#[must_use]
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

#[must_use]
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

#[must_use]
pub fn calculate_map(pixels: u32, min: &Point, max: &Point) -> MapInfo {
    let pixels = f64::from(pixels);

    let lat = min.lat + (max.lat - min.lat) / 2.0;
    let lng = min.lng + (max.lng - min.lng) / 2.0;

    let map_width_meters = haversine(&Point { lat, lng: min.lng }, &Point { lat, lng: max.lng });
    let map_height_meters = haversine(&Point { lat: min.lat, lng }, &Point { lat: max.lat, lng });
    let map_meters = map_height_meters.max(map_width_meters);

    println!("meters: {}, {}", map_width_meters, map_height_meters);

    let meters_per_pixel = map_meters / pixels;
    println!("meters per pixel: {}", meters_per_pixel);

    let lat_delta = max.lat - min.lat;
    let lng_delta = max.lng - min.lng;
    println!("delta: {}, {}", lng_delta, lat_delta);
    println!(
        "delta/meters: {:e}, {:e}",
        lng_delta / map_width_meters,
        lat_delta / map_height_meters
    );
    let map_delta = (lat_delta).max(lng_delta);
    let scale = pixels / map_delta;
    println!("delta: {}, scale: {}", map_delta, scale);

    let zoom = ((10_018_755.0 * lat.to_radians().cos()) / meters_per_pixel).ln()
        / std::f64::consts::LN_2
        - 7.0;

    let half_delta = map_delta / 2.0;
    MapInfo {
        center: Point { lat, lng },
        min: Point {
            lat: lat - half_delta,
            lng: lng - half_delta,
        },
        zoom,
        scale,
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
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::unreadable_literal)]
    fn trk_pts() {
        let gpx = r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:schemaLocation="http://www.topografix.com/GPX/1/1 http://www.topografix.com/GPX/1/1/gpx.xsd" version="1.1" xmlns="http://www.topografix.com/GPX/1/1">
 <metadata>
  <time>2019-05-09T02:39:00Z</time>
 </metadata>
 <trk>
  <name>Ride</name>
  <type>1</type>
  <trkseg>
   <trkpt lat="30.2430140" lon="-97.8100160">
    <ele>177.8</ele>
    <time>2019-11-10T20:49:52Z</time>
   </trkpt>
   <trkpt lat="30.2429950" lon="-97.8100270">
    <ele>177.6</ele>
    <time>2019-11-10T20:49:53Z</time>
   </trkpt>
   <trkpt lat="30.2428630" lon="-97.8101550">
    <ele>177.9</ele>
    <time>2019-11-10T20:49:54Z</time>
   </trkpt>
   <trkpt lat="30.2428470" lon="-97.8102190">
    <ele>178.0</ele>
    <time>2019-11-10T20:49:55Z</time>
   </trkpt>
   <trkpt lat="30.2428310" lon="-97.8102830">
    <ele>178.2</ele>
    <time>2019-11-10T20:49:56Z</time>
   </trkpt>
   <trkpt lat="30.2427670" lon="-97.8105240">
    <ele>179.0</ele>
    <time>2019-11-10T20:49:57Z</time>
   </trkpt>
   <trkpt lat="30.2427500" lon="-97.8105730">
    <ele>179.1</ele>
    <time>2019-11-10T20:49:58Z</time>
   </trkpt>
   <trkpt lat="30.2427330" lon="-97.8106130">
    <ele>179.3</ele>
    <time>2019-11-10T20:49:59Z</time>
   </trkpt>
  </trkseg>
 </trk>
</gpx>
"#;
        assert_eq!(
            get_pts(&gpx),
            vec![
                TrkPt {
                    center: Point {
                        lat: 30.2430140,
                        lng: -97.8100160
                    },
                    time: Some("2019-11-10T20:49:52Z".parse::<DateTime<Utc>>().unwrap())
                },
                TrkPt {
                    center: Point {
                        lat: 30.2429950,
                        lng: -97.8100270
                    },
                    time: Some("2019-11-10T20:49:53Z".parse::<DateTime<Utc>>().unwrap())
                },
                TrkPt {
                    center: Point {
                        lat: 30.2428630,
                        lng: -97.8101550
                    },
                    time: Some("2019-11-10T20:49:54Z".parse::<DateTime<Utc>>().unwrap())
                },
                TrkPt {
                    center: Point {
                        lat: 30.2428470,
                        lng: -97.8102190
                    },
                    time: Some("2019-11-10T20:49:55Z".parse::<DateTime<Utc>>().unwrap())
                },
                TrkPt {
                    center: Point {
                        lat: 30.2428310,
                        lng: -97.8102830
                    },
                    time: Some("2019-11-10T20:49:56Z".parse::<DateTime<Utc>>().unwrap())
                },
                TrkPt {
                    center: Point {
                        lat: 30.2427670,
                        lng: -97.8105240
                    },
                    time: Some("2019-11-10T20:49:57Z".parse::<DateTime<Utc>>().unwrap())
                },
                TrkPt {
                    center: Point {
                        lat: 30.2427500,
                        lng: -97.8105730
                    },
                    time: Some("2019-11-10T20:49:58Z".parse::<DateTime<Utc>>().unwrap())
                },
                TrkPt {
                    center: Point {
                        lat: 30.2427330,
                        lng: -97.8106130
                    },
                    time: Some("2019-11-10T20:49:59Z".parse::<DateTime<Utc>>().unwrap())
                }
            ]
        );
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
        assert!((map_info.scale - 3_155.221_102_118_793).abs() < std::f64::EPSILON);
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
        assert!((map_info.scale - 21_573.809_395_390_985).abs() < std::f64::EPSILON);
    }
}
