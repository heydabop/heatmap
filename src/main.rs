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
