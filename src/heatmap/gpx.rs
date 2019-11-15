use chrono::{DateTime, Utc};
use quick_xml::events::Event;
use quick_xml::Reader;
use simple_error::{bail, SimpleError};

pub fn get_pts(
    mut reader: Reader<&[u8]>,
    type_filter: &Option<String>,
) -> Result<Vec<super::TrkPt>, SimpleError> {
    let mut buf = Vec::new();

    let mut in_trk = false; // true if we're between a <trk> and </trk> tag (the bulk of the gpx file)
    let mut in_trkseg = false; // true if we're between a <trkseg> and </trkseg> tag
    let mut in_time = false; // true if we're in a <time> tag (the next event should be the Text of the tag))
    let mut in_type = false; // true if we're in a <type> tag that's in a <trk> block

    let mut curr_trk_pt: Option<&mut super::TrkPt> = None; // refernece to the TrkPt current being processed (stored at the tail of the trk_pts vector), this is set to None when we hit </trkpt>
    let mut trk_pts = Vec::new();

    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) => match e.name() {
                b"trk" => in_trk = true, // mark that we're within <trk> </trk>, which we will be for most of the file
                b"trkseg" => in_trkseg = true, // mark that we're within <trkseg> </trkseg>
                b"type" => in_type = in_trk, // mark that we're within <type> in a <trk>
                b"trkpt" => {
                    if !in_trk {
                        // we could ignore a <trkpt> outside of <trk> but this seems malformed so we error out
                        bail!("trkpt out of trk");
                    }
                    if !in_trkseg {
                        // we could ignore a <trkpt> outside of <trkseg> but this seems malformed so we error out
                        bail!("trkpt out of trkseg");
                    }
                    if curr_trk_pt.is_some() {
                        // same here, seems malformed so we error out
                        bail!("nested trkpt");
                    }

                    let mut lng = 0.0;
                    let mut lat = 0.0;
                    // the <trkpt> tag has "lat" and "lon" attributes that we read and parse into floats
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

                    // push this half finished TrkPt and hold a reference to it
                    trk_pts.push(super::TrkPt {
                        center: super::Point { lat, lng },
                        time: None,
                    });
                    curr_trk_pt = trk_pts.last_mut();
                }
                b"time" => {
                    if curr_trk_pt.is_none() {
                        continue;
                    }
                    in_time = true; // mark that we're in a <time> tag and the next Text event is time for our curr_trk_pt
                }
                _ => (),
            },
            Ok(Event::End(ref e)) => match e.name() {
                b"trk" => in_trk = false,
                b"trkseg" => in_trkseg = false,
                b"time" => in_time = false,
                b"type" => in_type = false,
                b"trkpt" => {
                    curr_trk_pt = None; // done with this TrkPt
                }
                _ => (),
            },
            Ok(Event::Text(e)) => {
                if in_type {
                    if let Some(filter) = type_filter {
                        // if we're in <type> and we have a set filter, check that this segment matches that filter, otherwise return nothing
                        if e.unescape_and_decode(&reader).unwrap() != *filter {
                            return Ok(Vec::new());
                        }
                    }
                }
                if in_time {
                    // if we're in <time> read and parse it for the curr_trk_pt
                    curr_trk_pt
                        .as_mut()
                        .expect("curr_trk_pt is none when in_time is true")
                        .time = Some(
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
