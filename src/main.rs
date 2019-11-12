#![feature(clamp)]

extern crate reqwest;

use image::{png, ImageDecoder, Rgb, RgbImage};
use std::env;
use std::fs;
use std::process::{self, Command};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <mapbox token> <gpx file>", args[0]);
        process::exit(1);
    }
    let access_token = &args[1];
    let filename = &args[2];
    let file = fs::read_to_string(&filename).expect("Unable to read file");

    let trk_pts = heatmap::get_pts(&file);

    let mut max_lat = -90.0;
    let mut min_lat = 90.0;
    let mut max_lng = -180.0;
    let mut min_lng = 180.0;
    for pt in &trk_pts {
        if pt.center.lat > max_lat {
            max_lat = pt.center.lat;
        }
        if pt.center.lat < min_lat {
            min_lat = pt.center.lat;
        }
        if pt.center.lng > max_lng {
            max_lng = pt.center.lng;
        }
        if pt.center.lng < min_lng {
            min_lng = pt.center.lng;
        }
    }

    println!("{}, {}\n{}, {}", max_lat, max_lng, min_lat, min_lng);

    let pixels = 1200;
    let map_info = heatmap::calculate_map(
        pixels,
        &heatmap::Point {
            lat: min_lat,
            lng: min_lng,
        },
        &heatmap::Point {
            lat: max_lat,
            lng: max_lng,
        },
    );

    let mapbox_response = reqwest::get(&format!("https://api.mapbox.com/styles/v1/mapbox/streets-v11/static/{},{},{}/{3}x{3}?access_token={4}", map_info.center.lng, map_info.center.lat, map_info.zoom, pixels, access_token)).expect("Error GETing mapbox image");
    let decoder = png::PNGDecoder::new(mapbox_response).expect("Error decoding mapbox response");
    let mut map = RgbImage::from_raw(
        pixels,
        pixels,
        decoder.read_image().expect("Erorr reading image into vec"),
    )
    .expect("Error reading RgbImage");

    let pixels = f64::from(pixels - 2);
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    for pt in &trk_pts {
        let x = ((pt.center.lng - map_info.min.lng) * map_info.scale.lng)
            .clamp(1.0, pixels)
            .round() as u32;
        let y = ((pt.center.lat - map_info.min.lat) * map_info.scale.lat)
            .clamp(1.0, pixels)
            .round() as u32;
        map.put_pixel(x, y, Rgb([255, 0, 0]));
    }

    let image_filename = format!("{}.png", filename);
    map.save(&image_filename).expect("Error saving final png");

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(&[&image_filename])
            .output()
            .unwrap_or_else(|e| panic!("Failed to open {}\n{}", image_filename, e));
    }
}
