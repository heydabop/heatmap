extern crate reqwest;

use image::{png, ImageDecoder, Rgb, RgbImage};
use std::path::PathBuf;
use std::process::{self, Command};
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(name = "heatmap")]
struct Opt {
    /// MapBox API Token
    #[structopt(short = "t", long = "token")]
    access_token: String,

    /// RGB Color used for heatmap
    #[structopt(short, long, default_value = "0,0,255")]
    color: String,

    /// Directory containing .gpx files
    #[structopt(name = "DIR", parse(from_os_str))]
    directory: PathBuf,

    /// Ratio of tracks a pixel has to be part of to become opaque (higher values will result in more transparent tracks)
    #[structopt(short, long, default_value = "0.25")]
    ratio: f64,
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

    let trk_pts = heatmap::get_pts_dir(&opt.directory);

    if trk_pts.is_empty() {
        eprintln!("No valid files loaded");
        process::exit(2);
    }

    // calculate min and max points
    let mut min = heatmap::Point {
        lat: 90.0,
        lng: 180.0,
    };
    let mut max = heatmap::Point {
        lat: -90.0,
        lng: -180.0,
    };
    for v in &trk_pts {
        for pt in v {
            max.lat = max.lat.max(pt.center.lat);
            min.lat = min.lat.min(pt.center.lat);
            max.lng = max.lng.max(pt.center.lng);
            min.lng = min.lng.min(pt.center.lng);
        }
    }

    let pixels = 1280;
    let map_info = heatmap::calculate_map(pixels, &min, &max);
    // get mapbox static API image based on center and zoom level from map_info
    let mapbox_response = reqwest::get(&format!("https://api.mapbox.com/styles/v1/mapbox/streets-v11/static/{},{},{}/{3}x{3}?access_token={4}", map_info.center.lng, map_info.center.lat, map_info.zoom, pixels, opt.access_token)).expect("Error GETing mapbox image");
    if !mapbox_response.status().is_success() {
        panic!(
            "Non success response code {} from mapbox",
            mapbox_response.status()
        );
    }
    // load mapbox response into image buffer
    let decoder = png::PNGDecoder::new(mapbox_response).expect("Error decoding mapbox response");
    let map_image = RgbImage::from_raw(
        pixels,
        pixels,
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
    );

    let image_filename = format!(
        "heatmap_{}_{}.png",
        opt.directory
            .file_name()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default(),
        opt.ratio
    );
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
