use std::env;
use std::fs;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <gpx file>", args[0]);
        process::exit(1);
    }
    let file = fs::read_to_string(&args[1]).expect("Unable to read file");

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

    let lat = min_lat + (max_lat - min_lat) / 2.0;
    let map_width_meters = heatmap::haversine(
        &heatmap::Point { lat, lng: min_lng },
        &heatmap::Point { lat, lng: max_lng },
    );
    println!("width: {}", map_width_meters);
    let map_width_pixels = 800.0;
    let meters_per_pixel = map_width_meters / map_width_pixels;
    println!("meters per pixel: {}", meters_per_pixel);

    let zoom_level = ((10_018_755.0 * lat.to_radians().cos()) / meters_per_pixel).ln()
        / std::f64::consts::LN_2
        - 7.0;

    let lng = min_lng + (max_lng - min_lng) / 2.0;

    println!("{}, {} zoom: {}", lat, lng, zoom_level);
}
