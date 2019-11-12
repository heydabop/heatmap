use chrono::{DateTime, Utc};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::env;
use std::fs;
use std::process;

struct TrkPt {
    lat: f64,
    lng: f64,
    time: Option<DateTime<Utc>>,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <gpx file>", args[0]);
        process::exit(1);
    }
    let file = fs::read_to_string(&args[1]).expect("Unable to read file");

    let mut reader = Reader::from_str(&file);
    reader.trim_text(true);

    let mut buf = Vec::new();

    let mut in_trk = false;
    let mut in_time = false;

    let mut curr_trk_pt: Option<Box<TrkPt>> = None;
    let mut trk_pts: Vec<Box<TrkPt>> = Vec::new();

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

                    curr_trk_pt = Some(Box::new(TrkPt {
                        lat,
                        lng,
                        time: None,
                    }));
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
                    trk_pts.push(curr_trk_pt.take().unwrap());
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

    let mut max_lat = -90.0;
    let mut min_lat = 90.0;
    let mut max_lng = -180.0;
    let mut min_lng = 180.0;
    for pt in &trk_pts {
        if pt.lat > max_lat {
            max_lat = pt.lat;
        }
        if pt.lat < min_lat {
            min_lat = pt.lat;
        }
        if pt.lng > max_lng {
            max_lng = pt.lng;
        }
        if pt.lng < min_lng {
            min_lng = pt.lng;
        }
    }

    println!("{}, {}\n{}, {}", max_lat, max_lng, min_lat, min_lng);
}
