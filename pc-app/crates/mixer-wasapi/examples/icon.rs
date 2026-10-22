//! Saves an executable's icon as PNG, to check icon extraction.
//!
//!     cargo run -p mixer-wasapi --example icon -- <exe> <out.png> [size]

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(exe), Some(out)) = (args.next(), args.next()) else {
        eprintln!("usage: icon <exe> <out.png> [size]");
        std::process::exit(2);
    };
    let size = args.next().and_then(|s| s.parse().ok()).unwrap_or(48);
    match mixer_wasapi::icon_png(&exe, size) {
        Some(png) => {
            std::fs::write(&out, &png).expect("write png");
            println!("wrote {} bytes to {out}", png.len());
        }
        None => println!("no icon in {exe}"),
    }
}
