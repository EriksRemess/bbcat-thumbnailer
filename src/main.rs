//! Minimal freedesktop thumbnailer built on bbcat's public library API.
//!
//! Nautilus expands the `%i`, `%o`, and `%s` fields from the installed
//! `.thumbnailer` file into an input path, output path, and requested size.
//! This program decodes the input into bbcat's format-independent [`Screen`],
//! samples a square region from that screen, and writes a small PNG without a
//! second image-processing dependency.

use std::{
    env,
    error::Error,
    ffi::OsString,
    fs::{self, File},
    io::{BufWriter, Write},
    path::PathBuf,
};

use bbcat::{DecodeOptions, Screen};

const MAX_THUMBNAIL_SIZE: usize = 256;

fn main() {
    // Keep process termination at the outer edge. The rest of the program can
    // use ordinary Results, which makes the pipeline straightforward to test.
    if let Err(error) = run(env::args_os().skip(1)) {
        eprintln!("bbcat-thumbnailer: {error}");
        std::process::exit(1);
    }
}

fn run(arguments: impl Iterator<Item = OsString>) -> Result<(), Box<dyn Error>> {
    let (input, output, requested_size) = parse_arguments(arguments)?;

    // Thumbnailers receive untrusted files from the file manager. Read and
    // decode the complete input before creating the output so a decode error
    // cannot leave behind a plausible-looking partial thumbnail.
    let data =
        fs::read(&input).map_err(|error| format!("could not read {}: {error}", input.display()))?;

    // The file name is a hint rather than the source of truth. bbcat still
    // detects content signatures first, but extensions distinguish formats
    // such as ADF, DDW, and RIPscrip that do not all have unique signatures.
    let document = bbcat::decode_with_options(
        &data,
        DecodeOptions {
            file_name: Some(&input),
            width: None,
        },
    )
    .map_err(|error| format!("could not decode {}: {error}", input.display()))?;

    // Nautilus can ask for several cache sizes. This example intentionally
    // caps them all at 256 pixels to keep generation cheap and predictable.
    let thumbnail = encode_thumbnail(&document.screen, requested_size.min(MAX_THUMBNAIL_SIZE))?;

    // The freedesktop thumbnail contract requires PNG data at the exact output
    // path supplied by the caller; the path itself need not end in `.png`.
    let file = File::create(&output)
        .map_err(|error| format!("could not create {}: {error}", output.display()))?;
    let mut output_file = BufWriter::new(file);
    output_file
        .write_all(&thumbnail)
        .map_err(|error| format!("could not write {}: {error}", output.display()))?;
    output_file
        .flush()
        .map_err(|error| format!("could not finish {}: {error}", output.display()))?;
    Ok(())
}

fn parse_arguments(
    mut arguments: impl Iterator<Item = OsString>,
) -> Result<(PathBuf, PathBuf, usize), Box<dyn Error>> {
    // Paths remain OsString values so non-UTF-8 Unix filenames still work.
    // Only SIZE is textual and therefore needs UTF-8 parsing.
    let input = arguments.next().ok_or("missing INPUT argument")?;
    let output = arguments.next().ok_or("missing OUTPUT argument")?;
    let requested_size = arguments.next().ok_or("missing SIZE argument")?;
    if arguments.next().is_some() {
        return Err("expected INPUT OUTPUT SIZE".into());
    }
    let requested_size = requested_size
        .to_str()
        .ok_or("SIZE is not valid UTF-8")?
        .parse::<usize>()
        .map_err(|_| "SIZE must be a positive integer")?;
    if requested_size == 0 {
        return Err("SIZE must be a positive integer".into());
    }
    Ok((input.into(), output.into(), requested_size))
}

fn encode_thumbnail(screen: &Screen, maximum_size: usize) -> Result<Vec<u8>, String> {
    let maximum_size = maximum_size.min(MAX_THUMBNAIL_SIZE);
    if maximum_size == 0 {
        return Err("thumbnail size must be non-zero".to_owned());
    }
    let (source_width, source_height) = screen
        .pixel_dimensions()
        .ok_or("rendered image dimensions overflow")?;
    if source_width == 0 || source_height == 0 {
        return Err("rendered image is empty".to_owned());
    }

    // A bbcat Screen is either a character grid with a bitmap font or an
    // indexed raster (RIPscrip). pixel_dimensions() hides that distinction.
    let (left, top, crop_size) = square_crop(source_width, source_height);
    let target_size = crop_size.min(maximum_size);

    // PNG stores a filter selector before every scanline. We use truecolor RGB
    // (three bytes per pixel) and filter 0, so the buffer layout is easy to see
    // and the small encoder below does not need a filtering implementation.
    let scanline_length = 1_usize
        .checked_add(target_size.checked_mul(3).ok_or("PNG row size overflow")?)
        .ok_or("PNG row size overflow")?;
    let mut scanlines = Vec::with_capacity(
        target_size
            .checked_mul(scanline_length)
            .ok_or("PNG buffer size overflow")?,
    );

    for y in 0..target_size {
        scanlines.push(0); // PNG filter: None

        // Nearest-neighbor sampling is a deliberate fit for bitmap-font art:
        // it preserves hard palette edges instead of blurring glyph pixels.
        let source_y = top + scaled_coordinate(y, crop_size, target_size);
        for x in 0..target_size {
            let source_x = left + scaled_coordinate(x, crop_size, target_size);
            scanlines.extend_from_slice(&pixel_color(screen, source_x, source_y)?);
        }
    }

    Ok(rgb_png(target_size, target_size, &scanlines))
}

fn square_crop(source_width: usize, source_height: usize) -> (usize, usize, usize) {
    // ANSI art is read from its top-left origin. Preserve its beginning rather
    // than selecting an arbitrary middle section of a long or wide canvas.
    (0, 0, source_width.min(source_height))
}

fn scaled_coordinate(position: usize, source_length: usize, target_length: usize) -> usize {
    // u128 intermediates avoid overflowing when dimensions are multiplied.
    ((position as u128 * source_length as u128) / target_length as u128) as usize
}

fn pixel_color(screen: &Screen, x: usize, y: usize) -> Result<[u8; 3], String> {
    // RIPscrip is already rasterized by bbcat. Its bytes are palette indexes,
    // and Screen::color resolves both embedded and standard VGA palettes.
    if let Some(raster) = screen.raster() {
        let index = y
            .checked_mul(raster.width)
            .and_then(|offset| offset.checked_add(x))
            .and_then(|offset| raster.pixels.get(offset))
            .ok_or("raster pixel is outside the rendered image")?;
        return Ok(screen.color(*index));
    }

    // Character formats need one more mapping: pixel -> cell -> font row. Font
    // bytes are glyph-major, with the most-significant bit at the left edge.
    let (glyph_width, glyph_height) = screen.glyph_dimensions();
    let cell = screen
        .cell(x / glyph_width, y / glyph_height)
        .ok_or("character pixel is outside the rendered image")?;
    let glyph_row = y % glyph_height;
    let glyph_offset = usize::from(cell.character)
        .checked_mul(glyph_height)
        .and_then(|offset| offset.checked_add(glyph_row))
        .ok_or("font glyph index overflow")?;
    let bits = screen
        .font()
        .and_then(|font| font.get(glyph_offset))
        .ok_or("character references a missing font glyph")?;
    let glyph_x = x % glyph_width;
    let foreground = match glyph_x {
        0..=7 => bits & (0x80 >> glyph_x) != 0,
        // Nine-pixel VGA fonts repeat column eight for box-drawing glyphs so
        // horizontal lines meet exactly as they did on VGA hardware.
        8 if (0xc0..=0xdf).contains(&cell.character) => bits & 1 != 0,
        _ => false,
    };
    Ok(screen.color(if foreground {
        cell.foreground
    } else {
        cell.background
    }))
}

fn rgb_png(width: usize, height: usize, scanlines: &[u8]) -> Vec<u8> {
    // A PNG is an eight-byte signature followed by length/type/data/CRC chunks.
    // IHDR selects 8-bit truecolor, IDAT carries zlib data, and IEND terminates
    // the stream. At 256x256, stored DEFLATE is small enough for thumbnails and
    // keeps bbcat as this example's only Rust dependency.
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&(width as u32).to_be_bytes());
    ihdr.extend_from_slice(&(height as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit truecolor RGB
    chunk(&mut png, b"IHDR", &ihdr);
    chunk(&mut png, b"IDAT", &zlib_store(scanlines));
    chunk(&mut png, b"IEND", &[]);
    png
}

fn zlib_store(data: &[u8]) -> Vec<u8> {
    // PNG wraps DEFLATE in zlib. Stored blocks add framing and checksums but do
    // not compress; each block is limited to 65,535 bytes by the format.
    let mut output = Vec::with_capacity(data.len() + data.len() / 65_535 * 5 + 11);
    output.extend_from_slice(&[0x78, 0x01]);
    if data.is_empty() {
        output.extend_from_slice(&[1, 0, 0, 0xff, 0xff]);
    } else {
        for (index, block) in data.chunks(65_535).enumerate() {
            output.push(u8::from(index + 1 == data.len().div_ceil(65_535)));
            let length = block.len() as u16;
            output.extend_from_slice(&length.to_le_bytes());
            output.extend_from_slice(&(!length).to_le_bytes());
            output.extend_from_slice(block);
        }
    }
    output.extend_from_slice(&adler32(data).to_be_bytes());
    output
}

fn adler32(data: &[u8]) -> u32 {
    // zlib ends with Adler-32 over the uncompressed scanline bytes.
    let (mut a, mut b) = (1_u32, 0_u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn chunk(output: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    // PNG's CRC covers the four-byte chunk name and payload, not its length.
    output.extend_from_slice(&(data.len() as u32).to_be_bytes());
    output.extend_from_slice(kind);
    output.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(data);
    output.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    // This is the reflected CRC-32 polynomial required by PNG.
    let mut crc = 0xffff_ffff_u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn parses_thumbnailer_arguments() {
        let arguments = ["drawing.ans", "thumbnail.png", "128"]
            .into_iter()
            .map(OsString::from);
        let (input, output, size) = parse_arguments(arguments).unwrap();

        assert_eq!(input, Path::new("drawing.ans"));
        assert_eq!(output, Path::new("thumbnail.png"));
        assert_eq!(size, 128);
    }

    #[test]
    fn rejects_zero_size() {
        let arguments = ["drawing.ans", "thumbnail.png", "0"]
            .into_iter()
            .map(OsString::from);
        assert_eq!(
            parse_arguments(arguments).unwrap_err().to_string(),
            "SIZE must be a positive integer"
        );
    }

    #[test]
    fn scales_and_crops_a_character_screen() {
        let document = bbcat::decode_with_options(
            b"ABCDEFGHIJ\r\n0123456789",
            DecodeOptions {
                file_name: Some(Path::new("drawing.ans")),
                width: Some(10),
            },
        )
        .unwrap();

        let png = encode_thumbnail(&document.screen, 16).unwrap();

        assert_eq!(png_dimensions(&png), (16, 16));
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn anchors_long_and_wide_crops_at_the_artwork_origin() {
        assert_eq!(square_crop(640, 1_600), (0, 0, 640));
        assert_eq!(square_crop(1_600, 400), (0, 0, 400));
    }

    #[test]
    fn caps_large_thumbnails_without_upscaling_small_ones() {
        let large = bbcat::decode(b"!|*|c0F|L00000A0A|#").unwrap();
        assert_eq!(
            png_dimensions(&encode_thumbnail(&large.screen, 512).unwrap()),
            (256, 256)
        );

        let small = bbcat::decode_with_options(
            b"X",
            DecodeOptions {
                file_name: None,
                width: Some(1),
            },
        )
        .unwrap();
        assert_eq!(
            png_dimensions(&encode_thumbnail(&small.screen, 256).unwrap()),
            (8, 8)
        );
    }

    fn png_dimensions(png: &[u8]) -> (u32, u32) {
        (
            u32::from_be_bytes(png[16..20].try_into().unwrap()),
            u32::from_be_bytes(png[20..24].try_into().unwrap()),
        )
    }
}
