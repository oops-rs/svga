# svga

A pure-Rust codec for the [SVGA](https://github.com/svga/SVGA-Format) animation
format: decode it, inspect it, edit it without loss, and write it back.

It is to SVGA what the `png` crate is to PNG. It does not render, composite or
play anything, and it never decides what is worth changing in a file; it gives
tools the means to do so safely.

- Reads SVGA 2.x (zlib + protobuf), zipped 2.x (`movie.binary`) and SVGA 1.x
  (zip + `movie.spec` JSON), all into one typed model.
- Edits 2.x files losslessly. Everything you do not touch is written back byte
  for byte: unknown fields, field order, float bit patterns.
- Treats input as hostile: size limits, checked arithmetic, no panics on
  malformed data, and errors with stable machine-readable codes.
- Pure Rust by default, so it builds for `wasm32` and on docs.rs.

## Usage

```toml
[dependencies]
svga = "0.1"
```

Read anything and look at it:

```rust
let animation = svga::Animation::from_bytes(&std::fs::read("gift.svga")?)?;
let movie = animation.movie();
println!(
    "SVGA {}: {}x{}, {} fps, {} sprites",
    movie.version,
    movie.params.view_box_width,
    movie.params.view_box_height,
    movie.params.fps,
    movie.sprites.len(),
);

// Which embedded images are dead weight, and how large is each one drawn?
println!("unused: {:?}", movie.unused_image_keys());
for usage in movie.image_usage() {
    println!("{}: {:?}", usage.key, usage.max_drawn_size);
}
```

Swap an embedded image and write the file back:

```rust
use svga::{Compression, Document};

let document = Document::from_bytes(&std::fs::read("gift.svga")?)?;
for image in document.images() {
    println!("{:?}: {} bytes, {:?}", image.key(), image.value().len(), image.kind());
}
let edited = document
    .replace_image("img_12", &optimized_png)?
    .remove_image("img_99")?;
std::fs::write("gift.min.svga", edited.to_bytes(Compression::Best)?)?;
```

Edits return a new `Document` and leave the original alone; untouched fields
share their bytes, so this is cheap.

`cargo run --example inspect -- file.svga` prints a summary of any file.

## Two layers, and why

Decoding a protobuf into structs and encoding the structs back silently drops
every field the structs do not know about. So editing never goes that way:

- **`Document`** is the lossless layer. It keeps each top-level field as its
  original bytes and only understands the `images` map. An unedited document
  re-encodes to exactly the payload it was read from. All edits happen here.
- **`Movie`** is a typed, read-only view decoded from a document on demand:
  params, sprites, frames, layouts, transforms, shapes, audio.

`Document::from_movie` builds a *new* document from a typed movie. That is how
SVGA 1.x converts to 2.x (`Animation::to_document`), and it is the one place
where output comes from structs rather than from stored bytes.

## Errors and limits

Every error has an `ErrorKind` (`Malformed`, `Unsupported`, `LimitExceeded`,
`InvalidEdit`, `Encode`) and a stable code such as `svga_truncated_varint` or
`svga_trailing_bytes`. Match on those, not on the display text.

Readers take optional `Limits`. The defaults accept files up to 64 MiB that
inflate to at most 256 MiB, 4096 zip entries, and 4 million decoded elements:

```rust
let limits = svga::Limits::default().with_max_inflated_bytes(32 * 1024 * 1024);
let document = svga::Document::from_bytes_with(&bytes, &limits)?;
```

A 2.x file must be exactly one zlib stream; trailing bytes are an error.

## Features

| Feature      | Default | Effect |
|--------------|---------|--------|
| `zip`        | yes     | Read zip containers: SVGA 1.x and zipped 2.x. Without it they are refused as unsupported. |
| `libdeflate` | no      | `Compression::Best` uses libdeflate (C) at its highest level for the smallest output. Without it, the best pure-Rust level is used. |

## Out of scope

Rendering, compositing and playback; PNG or audio optimization; any lossy
transform. Image values are handed over as bytes and never decoded.

## Testing against real files

The test suite builds its fixtures by hand, so it needs no assets. To check a
directory of real `.svga` files (byte-identical re-encoding, and optionally
agreement with [libsvga](https://github.com/wenext-limited/libsvga)'s
`svga_probe`):

```sh
SVGA_SAMPLES=path/to/files SVGA_PROBE=path/to/svga_probe \
    cargo test --release --test corpus -- --ignored --nocapture
```

Minimum supported Rust version: 1.89.

## License

MIT
