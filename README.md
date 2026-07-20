# Using bbcat in a Nautilus thumbnailer

[`bbcat`](https://bbcat.dev/) is a Rust library for decoding ANSI and BBS
artwork. This repository shows one way to embed it: a Nautilus thumbnailer that
turns supported artwork into small square previews.

The important part of the example is the boundary between bbcat and the host
application. bbcat recognizes and decodes ANSI/ASC/DIZ, NFO, DarkDraw DDW,
ArtWorx ADF, RIPscrip, and XBin data. The application decides what to do with
the decoded artwork—in this case, crop it, scale it, and hand a PNG to Nautilus.

## Using bbcat in this example

The application reads a file and asks bbcat to decode it:

```rust
let data = std::fs::read(&input)?;
let document = bbcat::decode_with_options(
    &data,
    bbcat::DecodeOptions {
        file_name: Some(&input),
        width: None,
    },
)?;
```

The filename helps bbcat distinguish formats whose contents do not carry a
unique signature. The returned `Document` has a common `Screen` regardless of
the original format, so the rest of the application does not need separate
ANSI, ADF, DDW, RIPscrip, and XBin rendering paths.

A `Screen` can contain either:

- character cells with colors, a bitmap font, and a palette; or
- an indexed pixel raster produced from graphics such as RIPscrip.

For an application that wants the complete rendered image unchanged,
`document.encode_png(1)` is enough. This thumbnailer needs a custom square crop
and arbitrary downscaling, so it uses the lower-level `Screen` API to sample
individual rendered pixels. That demonstrates both the convenient document
encoder and the more flexible screen representation available to bbcat users.

## The thumbnailer scenario

Nautilus starts the program with:

```console
bbcat-thumbnailer INPUT OUTPUT SIZE
```

The example then:

1. Reads `INPUT` and decodes it with bbcat.
2. Selects a square at the artwork's top-left origin, keeping the beginning of
   tall art and the left side of wide art.
3. Maps each thumbnail pixel back to a pixel in bbcat's `Screen`.
4. Resolves glyph bits or raster indexes through the screen's palette.
5. Writes an RGB PNG to `OUTPUT`, capped at 256 × 256 pixels.

The PNG writer uses only the Rust standard library, leaving bbcat as the
example's only Rust dependency.

## Build and try it

```console
make
cargo test --locked
```

The Makefile expects Cargo at `$HOME/.cargo/bin/cargo`. If yours is elsewhere,
pass it explicitly:

```console
make CARGO=/path/to/cargo
```

Try the program directly before installing it:

```console
target/release/bbcat-thumbnailer artwork.ans /tmp/artwork.png 256
```

Open `/tmp/artwork.png` to inspect the result.

## Install for Nautilus

```console
make install
nautilus -q
```

The project builds as your normal user. Installation asks for `sudo` only when
copying the finished binary and registration files to `/usr/local`. GNOME runs
thumbnailers in a sandbox that can read `/usr`, but not executables under your
home directory.

Open a folder containing supported artwork after Nautilus restarts. Uninstall
with:

```console
make uninstall
```

## Refresh cached thumbnails

Nautilus remembers successful thumbnails and failed attempts. Clear the cache
after changing the rendering code if older previews remain visible:

```console
find ~/.cache/thumbnails -type f -name '*.png' -delete
nautilus -q
```

This removes generated thumbnails only; it does not change the artwork.

## Follow the code

Start with [`run`](src/main.rs), which shows file handling and
`bbcat::decode_with_options`. Continue with `encode_thumbnail` and
`pixel_color` to see how the application reads bbcat's `Screen`. The remaining
functions form a deliberately small PNG encoder.

The [`data/`](data) directory contains the freedesktop thumbnailer registration
and MIME definitions. Those files are specific to this Nautilus scenario;
`src/main.rs` contains the reusable bbcat integration ideas.
