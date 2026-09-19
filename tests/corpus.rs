//! Opt-in checks against a directory of real `.svga` files, which are not part
//! of this repository.
//!
//! ```text
//! SVGA_SAMPLES=<dir> [SVGA_PROBE=<libsvga svga_probe binary>] \
//!     cargo test --release --test corpus -- --ignored --nocapture
//! ```
use std::{io::Read, path::PathBuf, process::Command};
use svga::{Animation, Compression, Document, Format};

fn samples() -> Vec<PathBuf> {
    let Some(directory) = std::env::var_os("SVGA_SAMPLES") else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "svga"))
        .collect();
    paths.sort();
    paths
}

/// Inflated independently of the crate, so the comparison means something.
fn inflate(bytes: &[u8]) -> Vec<u8> {
    let mut proto = Vec::new();
    flate2::bufread::ZlibDecoder::new(bytes)
        .read_to_end(&mut proto)
        .unwrap();
    proto
}

fn summary(animation: &Animation) -> String {
    let movie = animation.movie();
    format!(
        "{} {}x{} fps={} frames={} images={} sprites={} audios={}",
        movie.version,
        movie.params.view_box_width,
        movie.params.view_box_height,
        movie.params.fps,
        movie.params.frames,
        movie.images.len(),
        movie.sprites.len(),
        movie.audios.len()
    )
}

#[test]
#[ignore = "needs SVGA_SAMPLES pointing at a directory of .svga files"]
fn unmodified_files_re_encode_to_the_same_payload() {
    let (mut zlib, mut zip) = (0, 0);
    for path in samples() {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let original = std::fs::read(&path).unwrap();
        let animation = Animation::from_bytes(&original).unwrap_or_else(|e| panic!("{name}: {e}"));
        let converted = animation.to_document().unwrap();
        assert_eq!(
            converted.movie().unwrap().sprites,
            animation.movie().sprites,
            "{name}"
        );
        if animation.format() != Format::Zlib {
            zip += 1;
            assert!(animation.movie().missing_image_keys().is_empty(), "{name}");
            continue;
        }
        zlib += 1;
        let proto = inflate(&original);
        let document = animation.document().unwrap();
        assert!(document.to_proto() == proto, "{name}: payload changed");
        let encoded = document.to_bytes(Compression::Fast).unwrap();
        assert!(
            inflate(&encoded) == proto,
            "{name}: re-encoded payload changed"
        );
        // Replacing an image with itself is a no-op at the byte level too.
        let same = document
            .images()
            .try_fold(document.clone(), |edited, image| {
                edited.replace_image(image.key().unwrap(), image.value())
            });
        assert!(
            same.unwrap().to_proto() == proto,
            "{name}: identity edit changed bytes"
        );
        assert!(Document::from_proto(proto).is_ok());
        for extra in [1, 7, 8, 17] {
            let padded = [original.clone(), vec![0; extra]].concat();
            let error = Document::from_bytes(&padded).unwrap_err();
            assert_eq!(error.code(), "svga_trailing_bytes", "{name}: +{extra}");
        }
        let error = Document::from_bytes(&original[..original.len() - 1]).unwrap_err();
        assert_eq!(
            error.code(),
            "svga_corrupt_zlib_stream",
            "{name}: truncated"
        );
    }
    println!("{zlib} zlib files byte-identical, {zip} zip files read");
}

#[test]
#[ignore = "needs SVGA_SAMPLES and SVGA_PROBE (libsvga's svga_probe binary)"]
fn the_typed_view_matches_libsvga() {
    let Some(probe) = std::env::var_os("SVGA_PROBE") else {
        return;
    };
    let mut compared = 0;
    for path in samples() {
        let output = Command::new(&probe).arg(&path).output().unwrap();
        // The probe reports on stderr.
        let stdout = [output.stdout, output.stderr].concat();
        let stdout = String::from_utf8_lossy(&stdout).into_owned();
        let line = stdout.lines().next().unwrap_or_default();
        let Some((_, expected)) = line.rsplit_once(".svga: ") else {
            panic!("unexpected probe output for {}: {stdout}", path.display());
        };
        let animation = Animation::from_bytes(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(summary(&animation), expected, "{}", path.display());
        compared += 1;
    }
    println!("{compared} files agree with svga_probe");
}
