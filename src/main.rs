#![feature(clamp)]
#![warn(clippy::pedantic)]

extern crate reqwest;

use chrono::{DateTime, Utc};
use image::{png, ImageDecoder, Rgb, RgbImage};
use std::path::PathBuf;
use std::process;
#[cfg(target_os = "macos")]
use std::process::Command;
use structopt::StructOpt;

mod heatmap;

#[derive(StructOpt)]
#[structopt(name = "heatmap")]
struct Opt {
    /// MapBox API Token
    #[structopt(short = "t", long = "token")]
    access_token: String,

    /// Minimum bounding box of generated map (instead of map growing to fit all points) as the decimal latitude & longitude of the northeast and southwest corners. e.g.: 40.799235,-73.943158,40.763277,-73.985393 (NElat,NElon,SWlat,SWlon)
    #[structopt(long = "box")]
    corners: Option<String>,

    /// Map biking tracks
    #[structopt(long)]
    bike: bool,

    /// RGB Color used for heatmap
    #[structopt(short, long, default_value = "0,255,0")]
    color: String,

    /// Factor used in calculating heatmap pixel opacity (higher values will result in more opaque pixels)
    #[structopt(short, long, default_value = "1")]
    factor: f64,

    /// Input GPX/TCX files and directories
    #[structopt(name = "file list", parse(from_os_str))]
    file_list: Vec<PathBuf>,

    /// Only map tracks that started before this date
    #[structopt(long)]
    end: Option<String>,

    /// Mapbox style used for map image
    #[structopt(long = "style", default_value = "mapbox/dark-v10")]
    mapbox_style: String,

    /// Minimum opacity of any track pixel that has at least 1 track on it
    #[structopt(short, long, default_value = "0.25")]
    min: f64,

    /// Map running tracks
    #[structopt(long)]
    run: bool,

    /// Only map tracks that started after this date
    #[structopt(long)]
    start: Option<String>,

    /// Map walking tracks
    #[structopt(long)]
    walk: bool,
}

#[allow(clippy::too_many_lines)]
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

    if opt.factor <= 0.0 {
        eprintln!("factor must be greater than 0");
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

    let filters = if opt.bike || opt.run || opt.walk {
        let mut filters = Vec::new();
        if opt.bike {
            filters.push(heatmap::ActivityType::Bike);
        }
        if opt.run {
            filters.push(heatmap::ActivityType::Run);
        }
        if opt.walk {
            filters.push(heatmap::ActivityType::Walk);
        }
        Some(filters)
    } else {
        None
    };

    let trk_pts = heatmap::get_pts_from_files(&opt.file_list, &filters, &start, &end);

    if trk_pts.is_empty() {
        eprintln!("No valid files loaded");
        process::exit(2);
    }

    // calculate min and max points, or try to parse specified values
    let (min, max) = if let Some(corners) = opt.corners {
        let corners: Vec<&str> = corners.split(',').collect();
        if corners.len() != 4 {
            eprintln!("--box must be 4 comma sepearated values");
            process::exit(1);
        }
        let max_lat = parse_lat_lng(corners[0]);
        let max_lng = parse_lat_lng(corners[1]);
        let min_lat = parse_lat_lng(corners[2]);
        let min_lng = parse_lat_lng(corners[3]);
        if max_lat <= min_lat || max_lng <= min_lng {
            eprintln!("first coordinate of --box must be strictly greater than second coordinate");
            process::exit(1);
        }
        (
            heatmap::Point {
                lat: min_lat,
                lng: min_lng,
            },
            heatmap::Point {
                lat: max_lat,
                lng: max_lng,
            },
        )
    } else {
        heatmap::min_max(&trk_pts)
    };

    let pixels = 1280;
    let map_info = heatmap::calculate_map(pixels, &min, &max, 2.0);
    // get mapbox static API image based on center and zoom level from map_info
    let mapbox_response = reqwest::get(&format!(
        "https://api.mapbox.com/styles/v1/{}/static/{},{},{}/{4}x{4}@2x?access_token={5}",
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
        opt.factor,
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

fn parse_lat_lng(val: &str) -> f64 {
    if let Ok(v) = val.parse::<f64>() {
        v
    } else {
        eprintln!("--box values must be decimal numbers");
        process::exit(1);
    }
}
