extern crate reqwest;

use image::{png, ImageDecoder, RgbImage};
use std::env;
use std::process::{self, Command};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <mapbox token> <gpx dir>", args[0]);
        process::exit(1);
    }
    let access_token = &args[1];
    let directory = &args[2];

    let trk_pts = heatmap::get_pts_dir(&directory);

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
    let mapbox_response = reqwest::get(&format!("https://api.mapbox.com/styles/v1/mapbox/streets-v11/static/{},{},{}/{3}x{3}?access_token={4}", map_info.center.lng, map_info.center.lat, map_info.zoom, pixels, access_token)).expect("Error GETing mapbox image");
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
    let heatmap_image = heatmap::overlay_image(map_image, &map_info, &trk_pts);

    let image_filename = "heatmap.png";
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
