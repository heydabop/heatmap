#![feature(clamp)]

use chrono::{DateTime, NaiveDateTime, Utc};
use image::{Rgb, RgbImage, Rgba, RgbaImage};
use quick_xml::events::Event;
use quick_xml::Reader;
use simple_error::{bail, SimpleError};
use std::fmt;
use std::fs;

const R: f64 = 6371e3; // earth mean radius in meters

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
    pub scale: Point,
}

pub fn get_pts(gpx: &str) -> Result<Vec<TrkPt>, SimpleError> {
    let mut reader = Reader::from_str(&gpx);
    reader.trim_text(true);

    let mut buf = Vec::new();

    let mut in_trk = false;
    let mut in_time = false;

    let mut curr_trk_pt: Option<&mut TrkPt> = None;
    let mut trk_pts = Vec::new();

    match reader.read_event(&mut buf) {
        Ok(Event::Decl(_)) => (),
        Err(e) => bail!("Error at position {}: {:?}", reader.buffer_position(), e),
        _ => bail!("Expected <?xml>"),
    }

    match reader.read_event(&mut buf) {
        Ok(Event::Start(ref e)) => match e.name() {
            b"gpx" => (),
            _ => bail!("Expected <gpx>, got {:?}", e.name()),
        },
        Err(e) => bail!("Error at position {}: {:?}", reader.buffer_position(), e),
        _ => bail!("Expected <gpx>"),
    }

    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) => match e.name() {
                b"trk" => in_trk = true,
                b"trkpt" => {
                    if !in_trk {
                        bail!("trkpt out of trk");
                    }
                    if curr_trk_pt.is_some() {
                        bail!("nested trkpt");
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
            Err(e) => bail!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }

        buf.clear();
    }

    Ok(trk_pts)
}

#[must_use]
pub fn get_pts_dir(directory: &str) -> (Vec<TrkPt>, u16) {
    let mut trk_pts = Vec::new();
    let mut count = 0;

    for entry in fs::read_dir(directory).expect("Error reading directory") {
        match entry {
            Ok(file) => match file.file_type() {
                Ok(f_type) => {
                    if !f_type.is_file() {
                        continue;
                    }
                    let contents = fs::read_to_string(file.path()).expect("Unable to read file");
                    match get_pts(&contents) {
                        Ok(mut pts) => {
                            trk_pts.append(&mut pts);
                            count += 1;
                        }
                        Err(e) => eprintln!("Error reading {:?}\n{}", file.path(), e),
                    }
                }
                Err(e) => eprintln!("Error getting file type\n{}", e),
            },
            Err(e) => eprintln!("Error reading directory entry\n{}", e),
        }
    }

    (trk_pts, count)
}

#[must_use]
pub fn haversine(p1: &Point, p2: &Point) -> f64 {
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

    R * c
}

#[must_use]
pub fn destination(p: &Point, bearing: f64, distance: f64) -> Point {
    let ang_dist = distance / R;

    let lat_rad = p.lat.to_radians();
    let b_rad = bearing.to_radians();

    let lat = (lat_rad.sin() * ang_dist.cos() + lat_rad.cos() * ang_dist.sin() * b_rad.cos())
        .asin()
        .to_degrees();
    let lng = p.lng
        + (b_rad.sin() * ang_dist.sin() * lat_rad.cos())
            .atan2(ang_dist.cos() - lat_rad.sin() * lat.to_radians().sin())
            .to_degrees();
    Point { lat, lng }
}

#[must_use]
pub fn calculate_map(pixels: u32, min: &Point, max: &Point) -> MapInfo {
    let pixels = f64::from(pixels);

    let lat = min.lat + (max.lat - min.lat) / 2.0;
    let lng = min.lng + (max.lng - min.lng) / 2.0;

    let map_width_meters = haversine(&Point { lat, lng: min.lng }, &Point { lat, lng: max.lng });
    let map_height_meters = haversine(&Point { lat: min.lat, lng }, &Point { lat: max.lat, lng });
    let map_meters = map_height_meters.max(map_width_meters);

    let meters_per_pixel = (map_meters / pixels) * 1.1;

    let zoom = ((10_018_755.0 * lat.to_radians().cos()) / meters_per_pixel).ln()
        / std::f64::consts::LN_2
        - 7.0;

    let center = Point { lat, lng };

    let diagonal = ((meters_per_pixel * pixels / 2.0).powi(2) * 2.0).sqrt();
    let min = destination(&center, 315.0, diagonal);
    let max = destination(&center, 135.0, diagonal);
    let scale = Point {
        lat: pixels / (max.lat - min.lat),
        lng: pixels / (max.lng - min.lng),
    };

    MapInfo {
        center,
        min,
        zoom,
        scale,
    }
}

#[must_use]
pub fn overlay_image(
    mut map_image: RgbImage,
    map_info: &MapInfo,
    trk_pts: &[TrkPt],
    trks: u16,
) -> RgbImage {
    let width = map_image.width();
    let height = map_image.height();

    let mut path_image = RgbaImage::new(width, height);

    let max_x = f64::from(width - 2);
    let max_y = f64::from(width - 2);
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    for pt in trk_pts {
        let x = ((pt.center.lng - map_info.min.lng) * map_info.scale.lng)
            .clamp(1.0, max_x)
            .round() as u32;
        let y = ((pt.center.lat - map_info.min.lat) * map_info.scale.lat)
            .clamp(1.0, max_y)
            .round() as u32;
        path_image.put_pixel(x, y, Rgba([255, 0, 0, 255]));
        for (x1, y1) in &[(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
            let p = path_image.get_pixel_mut(*x1, *y1);
            let Rgba(data) = *p;
            *p = Rgba([
                255,
                data[1],
                data[2],
                (u16::from(data[3]) + (64 / trks)).min(255) as u8,
            ]);
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    for (x, y, path_pixel) in path_image.enumerate_pixels() {
        let Rgba(path_data) = *path_pixel;
        if path_data[3] > 0 {
            let map_pixel = map_image.get_pixel_mut(x, y);
            let Rgb(map_data) = *map_pixel;
            let alpha = f64::from(path_data[3]) / 255.0;
            let mut new_pixel = [0; 3];
            for i in 0..2 {
                new_pixel[i] = (f64::from(path_data[i]) * alpha
                    + f64::from(map_data[i]) * (1.0 - alpha))
                    .round() as u8;
            }
            *map_pixel = Rgb(new_pixel);
        }
    }

    map_image
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
    fn destination_test() {
        let dest = destination(
            &Point {
                lat: 30.343_888,
                lng: -103.970_1,
            },
            0.0,
            300.0,
        );
        assert!((dest.lat - 30.346_585_964_817_75).abs() < std::f64::EPSILON);
        assert!((dest.lng - -103.9701).abs() < std::f64::EPSILON);
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
            get_pts(&gpx).unwrap(),
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
}
