//! Print what an SVGA file contains: `cargo run --example inspect -- file.svga`
use std::{error::Error, process::ExitCode};
use svga::Animation;

fn inspect(path: &str) -> Result<(), Box<dyn Error>> {
    let animation = Animation::from_bytes(&std::fs::read(path)?)?;
    let movie = animation.movie();
    let params = &movie.params;
    println!("{path}: SVGA {} ({:?})", movie.version, animation.format());
    println!(
        "  {}x{} at {} fps, {} frames, {} sprites, {} audios",
        params.view_box_width,
        params.view_box_height,
        params.fps,
        params.frames,
        movie.sprites.len(),
        movie.audios.len()
    );
    for (image, usage) in movie.images.iter().zip(movie.image_usage()) {
        let drawn = usage
            .max_drawn_size
            .map_or("never drawn".to_owned(), |size| {
                format!("drawn up to {:.1}x{:.1}", size.width, size.height)
            });
        println!(
            "  {:<24} {:>9} bytes  {:?}  {} sprites, {drawn}",
            image.key, image.byte_len, image.kind, usage.sprites
        );
    }
    for key in movie.unused_image_keys() {
        println!("  unused: {key}");
    }
    Ok(())
}

fn main() -> ExitCode {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: inspect <file.svga>...");
        return ExitCode::FAILURE;
    }
    let failures = paths
        .iter()
        .filter(|path| {
            inspect(path)
                .inspect_err(|error| eprintln!("{path}: {error}"))
                .is_err()
        })
        .count();
    if failures == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
