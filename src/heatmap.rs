use chrono::{DateTime, Utc};
use conv::prelude::*;
use image::{Rgb, RgbImage};
use quick_xml::events::Event;
use quick_xml::Reader;
use simple_error::bail;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::PathBuf;

mod gpx;
mod tcx;

const R: f64 = 6371e3; // earth mean radius in meters

#[derive(PartialEq)]
pub struct Point {
    pub lat: f64,
    pub lng: f64,
}

enum XmlType {
    Gpx,
    Tcx,
}

pub enum ActivityType {
    Bike,
    Run,
    Walk,
}

impl fmt::Debug for Point {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}, {}", self.lat, self.lng)
    }
}

impl std::ops::Mul<f64> for Point {
    type Output = Self;

    fn mul(self, factor: f64) -> Self {
        Self {
            lat: self.lat * factor,
            lng: self.lng * factor,
        }
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
            "{:?} @ {:?}",
            self.center,
            self.time.map(|time| time.to_rfc3339())
        )
    }
}

pub struct MapInfo {
    pub center: Point,
    pub min: Point,
    pub zoom: f64,
    pub scale: Point,
}

/// Parses trkpt's from gpx or tcx file into vector
pub fn get_pts(
    contents: &str,
    type_filters: &Option<Vec<ActivityType>>,
    start: &Option<DateTime<Utc>>,
    end: &Option<DateTime<Utc>>,
) -> Result<Vec<TrkPt>, Box<dyn Error>> {
    let mut reader = Reader::from_str(contents);
    reader.trim_text(true);

    let mut buf = Vec::new();

    // check for <?xml> declaration
    match reader.read_event(&mut buf) {
        Ok(Event::Decl(_)) => (),
        Err(e) => bail!("Error at position {}: {:?}", reader.buffer_position(), e),
        _ => bail!("Expected <?xml>"),
    }
    buf.clear();

    // check for <gpx> or <TrainingCenterDatabase> opening tag
    let file_type = match reader.read_event(&mut buf) {
        Ok(Event::Start(ref e)) => match e.name() {
            b"gpx" => XmlType::Gpx,
            b"TrainingCenterDatabase" => XmlType::Tcx,
            _ => bail!(
                "Expected <gpx> or <TrainingCenterDatabase>, got {:?}",
                e.name()
            ),
        },
        Err(e) => bail!("Error at position {}: {:?}", reader.buffer_position(), e),
        _ => bail!("Expected <gpx> or <TrainingCenterDatabase>"),
    };

    match file_type {
        XmlType::Gpx => gpx::get_pts(reader, type_filters, start, end),
        XmlType::Tcx => tcx::get_pts(reader, type_filters, start, end),
    }
}

#[must_use]
/// Iterates over paths in `file_list` and tries to parse files or files in directories as gpx/tcx files
/// Filters by `type_filter` (only returning tracks of the given type) and start/end dates (only returning tracks that start after `start` or before `end`)
/// Returns a vector of vectors (one per processed file) of `TrkPts`
pub fn get_pts_from_files(
    file_list: &[PathBuf],
    type_filters: &Option<Vec<ActivityType>>,
    start: &Option<DateTime<Utc>>,
    end: &Option<DateTime<Utc>>,
) -> Vec<Vec<TrkPt>> {
    let mut trk_pts = Vec::new();

    for path in file_list {
        match fs::metadata(path) {
            Ok(meta) => {
                let f_type = meta.file_type();
                if f_type.is_file() {
                    match get_pts_file(path, type_filters, start, end) {
                        Ok(pts) => {
                            if !pts.is_empty() {
                                trk_pts.push(pts);
                            }
                        }
                        Err(e) => eprintln!("Error reading {path:?}: {e}"),
                    }
                } else if f_type.is_dir() {
                    let mut dir_pts = get_pts_dir(path, type_filters, start, end);
                    trk_pts.append(&mut dir_pts);
                } else {
                    eprintln!("Unable to read {path:?}");
                }
            }
            Err(e) => eprintln!("Error stating {path:?}: {e}"),
        }
    }

    trk_pts
}

/// Attempts to parse `file` as gpx or tcx file and read it into `TrkPt`s
/// Filters by `type_filter` (only returning tracks of the given type) and start/end dates (only returning tracks that start after `start` or before `end`)
/// Returns a vector of `TrkPts` of the waypoints in the file
pub fn get_pts_file(
    file: &PathBuf,
    type_filters: &Option<Vec<ActivityType>>,
    start: &Option<DateTime<Utc>>,
    end: &Option<DateTime<Utc>>,
) -> Result<Vec<TrkPt>, Box<dyn Error>> {
    let contents = fs::read_to_string(file)?;
    get_pts(&contents, type_filters, start, end)
}

#[must_use]
/// Iterates over entires in directory and tries to parse them as gpx or tcx files if they're files.
/// Filters by `type_filter` (only returning tracks of the given type) and start/end dates (only returning tracks that start after `start` or before `end`)
/// Returns a vector of vectors (one per processed file) of `TrkPts` from the directory contents
pub fn get_pts_dir(
    directory: &PathBuf,
    type_filters: &Option<Vec<ActivityType>>,
    start: &Option<DateTime<Utc>>,
    end: &Option<DateTime<Utc>>,
) -> Vec<Vec<TrkPt>> {
    let mut file_list = Vec::new();

    for entry in fs::read_dir(directory).expect("Error reading directory") {
        match entry {
            Ok(file) => file_list.push(file.path()),
            Err(e) => eprintln!("Error reading directory entry: {e}"),
        }
    }

    get_pts_from_files(&file_list, type_filters, start, end)
}

#[must_use]
/// Returns two points that are comprised of the lowest latitude and lowest longitude and highest latitude and highest longitude within `pts`.
/// Note that these values are all considered independently and not as a point, so an input of [[35, 77], [33, 78]] would return ([33, 77], [35, 78]), meaning the output points may not exist in the input.
pub fn min_max(pts: &[Vec<TrkPt>]) -> (Point, Point) {
    let mut min = Point {
        lat: 90.0,
        lng: 180.0,
    };
    let mut max = Point {
        lat: -90.0,
        lng: -180.0,
    };
    for v in pts {
        for pt in v {
            max.lat = max.lat.max(pt.center.lat);
            min.lat = min.lat.min(pt.center.lat);
            max.lng = max.lng.max(pt.center.lng);
            min.lng = min.lng.min(pt.center.lng);
        }
    }

    (min, max)
}

#[must_use]
/// Computes great-circle distance between p1 and p2
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
/// Finds destination point along great-circle path (in meters) from start point p towards bearing
pub fn destination(p: &Point, bearing: f64, distance: f64) -> Point {
    let ang_dist = distance / R;

    let lat_rad = p.lat.to_radians();
    let b_rad = bearing.to_radians();
    let (lat_rad_sin, lat_rad_cos) = lat_rad.sin_cos();
    let (ang_dist_sin, ang_dist_cos) = ang_dist.sin_cos();

    let lat = (lat_rad_sin.mul_add(ang_dist_cos, lat_rad_cos * ang_dist_sin * b_rad.cos()))
        .asin()
        .to_degrees();
    let lng = p.lng
        + (b_rad.sin() * ang_dist_sin * lat_rad_cos)
            .atan2(ang_dist_cos - lat_rad_sin * lat.to_radians().sin())
            .to_degrees();
    Point { lat, lng }
}

#[must_use]
#[allow(clippy::doc_markdown)]
/// Based on image size and lat/lng ranges, calculates the center and MapBox zoom level of a map, and the new minimum lat/lng and scale for linear transformation from lat/lng to pixel
pub fn calculate_map(pixels: u32, min: &Point, max: &Point, scale_multiplier: f64) -> MapInfo {
    let pixels = f64::from(pixels);

    // simple centers
    let lat = min.lat + (max.lat - min.lat) / 2.0;
    let lng = min.lng + (max.lng - min.lng) / 2.0;

    // width and height of map in meters at the center (this will be inaccurate towrads map edges if map is too big)
    let map_width_meters = haversine(&Point { lat, lng: min.lng }, &Point { lat, lng: max.lng });
    let map_height_meters = haversine(&Point { lat: min.lat, lng }, &Point { lat: max.lat, lng });
    // take the great of the two and use it to calculate zoom level
    let map_meters = map_height_meters.max(map_width_meters);

    let meters_per_pixel = (map_meters / pixels) * 1.1; //add padding so min/max aren't right against edge of map

    // calculate MapBox zoom level at center latitude (this will also be inaccuate for larger maps)
    let zoom = ((10_018_755.0 * lat.to_radians().cos()) / meters_per_pixel).ln()
        / std::f64::consts::LN_2
        - 7.0;

    let center = Point { lat, lng };

    // calculate new min/max points on map by finding destination point from center to corners
    let dist_to_edge = meters_per_pixel * pixels / 2.0;
    let diagonal = dist_to_edge.hypot(dist_to_edge);
    let min = destination(&center, 315.0, diagonal);
    let max = destination(&center, 135.0, diagonal);
    // calculate scale for linear transformations of lat/lng to pixel
    let scale = Point {
        lat: pixels / (max.lat - min.lat),
        lng: pixels / (max.lng - min.lng),
    };

    MapInfo {
        center,
        min,
        zoom,
        scale: scale * scale_multiplier,
    }
}

#[must_use]
/// Overlays dots with color `track_color` from `trk_pts` on `map_image` using scaling information in `map_info`
/// `factor` is the multiplier of a mapped pixels opacity (the pixel opacity of the track layer is `factor` / 75th percentile of number of tracks greater than 1 on all pixels)
pub fn overlay_image(
    mut map_image: RgbImage,
    map_info: &MapInfo,
    trk_pts: &[Vec<TrkPt>],
    track_color: Rgb<u8>,
    factor: f64,
    min_alpha: f64,
) -> RgbImage {
    let trks = trk_pts.len();
    let width = i32::value_from(map_image.width()).expect("image width must fit in i32");
    let height = i32::value_from(map_image.height()).expect("image height must fit in i32");

    // count of how many times a pixel is part of a track, will be multiplied by single step and capped to 2 during compositing
    #[allow(clippy::cast_sign_loss)]
    let mut factors = vec![vec![0; width as usize]; height as usize];

    // used to clamp dots (and neighbors) from going beyond image bounds
    let max_x = width - 2;
    let max_y = height - 2;
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    for v in trk_pts {
        let mut prev_x: Option<i32> = None; //the x of the last pixel, for line drawing
        let mut prev_y: Option<i32> = None; //the y of the last pixel
        let mut prev_time: Option<DateTime<Utc>> = None; //the timestamp of the TrkPt used to draw the last pixel
        for pt in v {
            // linear transformations
            let x = ((pt.center.lng - map_info.min.lng) * map_info.scale.lng).round() as i32;
            let y = ((pt.center.lat - map_info.min.lat) * map_info.scale.lat).round() as i32;
            if x < 1 || x > max_x || y < 1 || y > max_y {
                continue;
            }

            // draw a line from previous pixel to this one
            if let Some(prev_x) = prev_x {
                let prev_y = prev_y.unwrap();
                if prev_x == x && prev_y == y {
                    // dont redraw on same pixel repeatedly (to try and prevent overly shading "slow" sections)
                    prev_time = pt.time;
                    continue;
                }

                // dont draw a line between points that are more than 5 seconds apart
                if pt.time.is_none()
                    || prev_time.is_none()
                    || (pt.time.unwrap() - prev_time.unwrap()).num_seconds().abs() <= 5
                {
                    let (x1, y1, x2, y2) = if prev_x >= x {
                        (x, y, prev_x, prev_y)
                    } else {
                        (prev_x, prev_y, x, y)
                    };
                    let slope = f64::from(y2 - y1) / f64::from(x2 - x1);
                    if slope.abs() <= 1.0 {
                        let b = f64::from(y1) - slope * f64::from(x1);
                        //increment along x, drawing at appropriate y
                        for curr_x in x1 + 1..x2 {
                            let curr_y = slope.mul_add(f64::from(curr_x), b).round() as usize;
                            factors[curr_x as usize][curr_y] += 1;
                        }
                    } else {
                        let (x1, y1, x2, y2) = if prev_y >= y {
                            (x, y, prev_x, prev_y)
                        } else {
                            (prev_x, prev_y, x, y)
                        };
                        let slope = f64::from(x2 - x1) / f64::from(y2 - y1);
                        let b = f64::from(x1) - slope * f64::from(y1);
                        //increment along y, drawing at appropriate x
                        for curr_y in y1 + 1..y2 {
                            let curr_x = slope.mul_add(f64::from(curr_y), b).round() as usize;
                            factors[curr_x][curr_y as usize] += 1;
                        }
                    }
                }
            }

            // draw current pixel
            factors[x as usize][y as usize] += 1;

            prev_x = Some(x);
            prev_y = Some(y);
            prev_time = pt.time;
        }
    }

    #[allow(clippy::cast_sign_loss)]
    let mut sorted = Vec::with_capacity(width as usize * height as usize);

    for row in &factors {
        for &factor in row {
            if factor > 1 {
                sorted.push(factor);
            }
        }
    }

    sorted.sort_unstable();

    // Take 75th percentile data point so that the top 25% "densest" pixels are all max alpha
    let single_step = factor / f64::from(sorted[sorted.len() / 4 * 3]);
    println!("Tracks: {trks} -- Step: {single_step:.2}");

    // composit path_image onto map_image
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    for (x, row) in factors.iter().enumerate() {
        for (y, &factor) in row.iter().enumerate() {
            let intensity = f64::from(factor) * single_step;
            if intensity > 0.0 {
                let alpha = intensity.clamp(min_alpha, 1.0);

                let map_pixel = map_image.get_pixel_mut(x as u32, y as u32);
                let Rgb(map_data) = *map_pixel;

                let mut new_pixel = [0; 3];
                // composit each color channel
                for i in 0..3 {
                    let color_a = f64::from(track_color[i]);
                    let color_b = f64::from(map_data[i]);
                    new_pixel[i] = (color_a.mul_add(alpha, color_b * (1.0 - alpha)))
                        .clamp(0.0, 255.0)
                        .round() as u8;
                }

                // save new composited pixel to map_image
                *map_pixel = Rgb(new_pixel);
            }
        }
    }

    map_image
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64;

    #[test]
    #[allow(clippy::unreadable_literal)]
    fn haversine_test() {
        let p1 = Point {
            lat: 31.2626,
            lng: -100.3555,
        };
        let p2 = Point {
            lat: 38.1345,
            lng: -89.6150,
        };
        assert!((haversine(&p1, &p2) - 1242682.4055201372).abs() < f64::EPSILON);
    }

    #[test]
    #[allow(clippy::unreadable_literal)]
    fn destination_test() {
        let dest = destination(
            &Point {
                lat: 30.343888,
                lng: -103.9701,
            },
            0.0,
            300.0,
        );
        assert!((dest.lat - 30.34658596481775).abs() < f64::EPSILON);
        assert!((dest.lng - -103.9701).abs() < f64::EPSILON);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::unreadable_literal)]
    fn min_max_test() {
        let (min, max) = min_max(&[vec![
            TrkPt {
                center: Point {
                    lat: 30.2430140,
                    lng: -97.8100270,
                },
                time: None,
            },
            TrkPt {
                center: Point {
                    lat: 30.2429950,
                    lng: -97.8100160,
                },
                time: None,
            },
            TrkPt {
                center: Point {
                    lat: 30.2428630,
                    lng: -97.8101550,
                },
                time: None,
            },
            TrkPt {
                center: Point {
                    lat: 30.2428470,
                    lng: -97.8102190,
                },
                time: None,
            },
            TrkPt {
                center: Point {
                    lat: 30.2428310,
                    lng: -97.8102830,
                },
                time: None,
            },
            TrkPt {
                center: Point {
                    lat: 30.2427670,
                    lng: -97.8105240,
                },
                time: None,
            },
            TrkPt {
                center: Point {
                    lat: 30.2427500,
                    lng: -97.8105730,
                },
                time: None,
            },
            TrkPt {
                center: Point {
                    lat: 30.2427330,
                    lng: -97.8106130,
                },
                time: None,
            },
        ]]);
        assert!((min.lat - 30.2427330).abs() < f64::EPSILON);
        assert!((min.lng + 97.8106130).abs() < f64::EPSILON);
        assert!((max.lat - 30.2430140).abs() < f64::EPSILON);
        assert!((max.lng + 97.8100160).abs() < f64::EPSILON);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::unreadable_literal)]
    fn gpx() {
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
            get_pts(gpx, &None, &None, &None).unwrap(),
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
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::unreadable_literal)]
    fn tcx() {
        let tcx = r#"<?xml version="1.0" encoding="UTF-8"?>
<TrainingCenterDatabase xsi:schemaLocation="http://www.garmin.com/xmlschemas/TrainingCenterDatabase/v2 http://www.garmin.com/xmlschemas/TrainingCenterDatabasev2.xsd" xmlns:ns5="http://www.garmin.com/xmlschemas/ActivityGoals/v1" xmlns:ns3="http://www.garmin.com/xmlschemas/ActivityExtension/v2" xmlns:ns2="http://www.garmin.com/xmlschemas/UserProfile/v2" xmlns="http://www.garmin.com/xmlschemas/TrainingCenterDatabase/v2" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
 <Activities>
  <Activity Sport="Biking">
   <Id>2019-11-15T21:54:00Z</Id>
   <Lap StartTime="2019-11-15T21:54:00Z">
    <TotalTimeSeconds>5614</TotalTimeSeconds>
    <DistanceMeters>42651.5</DistanceMeters>
    <MaximumSpeed>65880.0</MaximumSpeed>
    <Calories>0</Calories>
    <AverageHeartRateBpm>
     <Value>161</Value>
    </AverageHeartRateBpm>
    <MaximumHeartRateBpm>
     <Value>189</Value>
    </MaximumHeartRateBpm>
    <Intensity>Active</Intensity>
    <TriggerMethod>Manual</TriggerMethod>
    <Track>
     <Trackpoint>
      <Time>2019-11-15T22:25:38Z</Time>
      <Position>
       <LatitudeDegrees>30.2431060</LatitudeDegrees>
       <LongitudeDegrees>-97.8099600</LongitudeDegrees>
      </Position>
      <AltitudeMeters>178.3</AltitudeMeters>
      <DistanceMeters>15251.8</DistanceMeters>
      <HeartRateBpm>
       <Value>131</Value>
      </HeartRateBpm>
      <Extensions>
       <TPX xmlns="http://www.garmin.com/xmlschemas/ActivityExtension/v2">
        <Speed>6.6</Speed>
       </TPX>
      </Extensions>
     </Trackpoint>
     <Trackpoint>
      <Time>2019-11-15T22:25:39Z</Time>
      <Position>
       <LatitudeDegrees>30.2430710</LatitudeDegrees>
       <LongitudeDegrees>-97.8099760</LongitudeDegrees>
      </Position>
      <AltitudeMeters>178.1</AltitudeMeters>
      <DistanceMeters>15257.7</DistanceMeters>
      <HeartRateBpm>
       <Value>130</Value>
      </HeartRateBpm>
      <Extensions>
       <TPX xmlns="http://www.garmin.com/xmlschemas/ActivityExtension/v2">
        <Speed>6.3</Speed>
       </TPX>
      </Extensions>
     </Trackpoint>
     <Trackpoint>
      <Time>2019-11-15T22:25:40Z</Time>
      <Position>
       <LatitudeDegrees>30.2430000</LatitudeDegrees>
       <LongitudeDegrees>-97.8100070</LongitudeDegrees>
      </Position>
      <AltitudeMeters>177.7</AltitudeMeters>
      <DistanceMeters>15264.1</DistanceMeters>
      <HeartRateBpm>
       <Value>130</Value>
      </HeartRateBpm>
      <Extensions>
       <TPX xmlns="http://www.garmin.com/xmlschemas/ActivityExtension/v2">
        <Speed>6.2</Speed>
       </TPX>
      </Extensions>
     </Trackpoint>
    </Track>
   </Lap>
  </Activity>
 </Activities>
</TrainingCenterDatabase>"#;
        assert_eq!(
            get_pts(tcx, &None, &None, &None).unwrap(),
            vec![
                TrkPt {
                    center: Point {
                        lat: 30.2431060,
                        lng: -97.8099600
                    },
                    time: Some("2019-11-15T22:25:38Z".parse::<DateTime<Utc>>().unwrap())
                },
                TrkPt {
                    center: Point {
                        lat: 30.2430710,
                        lng: -97.8099760
                    },
                    time: Some("2019-11-15T22:25:39Z".parse::<DateTime<Utc>>().unwrap())
                },
                TrkPt {
                    center: Point {
                        lat: 30.2430000,
                        lng: -97.8100070
                    },
                    time: Some("2019-11-15T22:25:40Z".parse::<DateTime<Utc>>().unwrap())
                }
            ]
        );
    }
}
