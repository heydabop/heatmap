#![feature(clamp)]

extern crate reqwest;

use chrono::{DateTime, Utc};
use image::{png, ImageDecoder, Rgb, RgbImage};
use std::path::PathBuf;
use std::process::{self, Command};
use structopt::StructOpt;

mod heatmap;

#[derive(StructOpt)]
#[structopt(name = "heatmap")]
struct Opt {
    /// MapBox API Token
    #[structopt(short = "t", long = "token")]
    access_token: String,

    /// Only mape biking tracks
    #[structopt(long)]
    bike: bool,

    /// RGB Color used for heatmap
    #[structopt(short, long, default_value = "0,255,0")]
    color: String,

    /// Directory containing .gpx files
    #[structopt(name = "DIR", parse(from_os_str))]
    directory: PathBuf,

    /// Only map tracks that started before this date
    #[structopt(long)]
    end: Option<String>,

    /// Mapbox style used for map image
    #[structopt(long = "style", default_value = "dark-v10")]
    mapbox_style: String,

    /// Minimum opacity of any track pixel that has at least 1 track on it
    #[structopt(short, long, default_value = "0.3")]
    min: f64,

    /// Ratio of tracks a pixel has to be part of to become opaque (higher values will result in more transparent tracks)
    #[structopt(short, long, default_value = "0.125")]
    ratio: f64,

    /// Only map running tracks (overridden by --bike)
    #[structopt(long)]
    run: bool,

    /// Only map tracks that started after this date
    #[structopt(long)]
    start: Option<String>,
}

fn main() {
    let opt = Opt::from_args();

    let color: Vec<u8> = opt
        .color
        .split(',')
        .map(|s| {
            s.trim()
                .parse()
                .expect("color must be in form of r,g,b (ex: 0,0,255)")
        })
        .collect();
    if color.len() != 3 {
        eprintln!("color must be in form of r,g,b (ex: 0,0,255)");
        process::exit(1);
    }

    if opt.ratio < 0.0 || opt.ratio > 1.0 {
        eprintln!("ratio must be between 0 and 1");
        process::exit(1);
    }

    let start = match opt.start {
        None => None,
        Some(start) => Some(
            start
                .parse::<DateTime<Utc>>()
                .expect("Unable to parse start into date"),
        ),
    };
    let end = match opt.end {
        None => None,
        Some(end) => Some(
            end.parse::<DateTime<Utc>>()
                .expect("Unable to parse end into date"),
        ),
    };

    let filter = if opt.bike {
        Some(heatmap::ActivityType::Bike)
    } else if opt.run {
        Some(heatmap::ActivityType::Run)
    } else {
        None
    };

    let trk_pts = heatmap::get_pts_dir(&opt.directory, &filter, &start, &end);

    if trk_pts.is_empty() {
        eprintln!("No valid files loaded");
        process::exit(2);
    }

    // calculate min and max points
    let (min, max) = heatmap::min_max(&trk_pts);

    let pixels = 1280;
    let map_info = heatmap::calculate_map(pixels, &min, &max, 2.0);
    // get mapbox static API image based on center and zoom level from map_info
    let mapbox_response = reqwest::get(&format!(
        "https://api.mapbox.com/styles/v1/mapbox/{}/static/{},{},{}/{4}x{4}@2x?access_token={5}",
        opt.mapbox_style,
        map_info.center.lng,
        map_info.center.lat,
        map_info.zoom,
        pixels,
        opt.access_token
    ))
    .expect("Error GETing mapbox image");
    if !mapbox_response.status().is_success() {
        panic!(
            "Non success response code {} from mapbox",
            mapbox_response.status()
        );
    }
    // load mapbox response into image buffer
    let decoder = png::PNGDecoder::new(mapbox_response).expect("Error decoding mapbox response");
    let (map_width, map_height) = decoder.dimensions();
    #[allow(clippy::cast_possible_truncation)]
    let map_image = RgbImage::from_raw(
        map_width as u32,
        map_height as u32,
        decoder.read_image().expect("Erorr reading image into vec"),
    )
    .expect("Error reading RgbImage");

    // overlay path from trk_pts onto map image
    let heatmap_image = heatmap::overlay_image(
        map_image,
        &map_info,
        &trk_pts,
        Rgb([color[0], color[1], color[2]]),
        opt.ratio,
        opt.min,
    );

    let image_filename = format!("heatmap_{}.png", Utc::now().timestamp());
    heatmap_image
        .save(&image_filename)
        .expect("Error saving final png");

    #[cfg(target_os = "macos")]
    {
        // open image in preview
        Command::new("open")
            .args(&[&image_filename])
            .output()
            .unwrap_or_else(|e| panic!("Failed to open {}\n{}", image_filename, e));
    }
}
