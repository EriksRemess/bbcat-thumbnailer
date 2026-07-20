# bbcat thumbnailer example

`bbcat-thumbnailer` is a small example of using the
[`bbcat`](https://bbcat.dev/) Rust library in a desktop integration. It renders
ANSI and BBS artwork as square PNG thumbnails for Nautilus and other
freedesktop-compatible file managers.

The example supports ANSI/ASC/DIZ, NFO, DarkDraw DDW, ArtWorx ADF, RIPscrip,
and XBin files. Tall art is cropped from the top and wide art from the left,
then reduced to at most 256 × 256 pixels. Small artwork is not enlarged.

## Build and test

```console
make
cargo test --locked
```

The Makefile uses `$HOME/.cargo/bin/cargo` by default. Set `CARGO` to override
it.

## Install

```console
make install
nautilus -q
```

Compilation runs as the current user. The install stage asks for `sudo` only
when copying the binary and desktop data to `/usr/local` and updating the MIME
database. `/usr/local` is required because GNOME hides home directories from
thumbnailer processes. Remove the installation with:

```console
make uninstall
```

Nautilus caches both successful and failed thumbnails. Clear old versions
after changing the renderer, then reopen the folder:

```console
find ~/.cache/thumbnails -type f -name '*.png' -delete
nautilus -q
```

## Debian package

Build a package for the current machine with:

```console
make deb
```

The package is written to `dist/`. For committed builds its version includes
the commit date and abbreviated hash; builds made before the first commit use
a timestamped `local` version. Runtime library dependencies are derived from
the compiled binary with `dpkg-shlibdeps`. Building packages requires the
standard Debian `dpkg` and `dpkg-dev` tools.

## How it works

Nautilus calls the executable as:

```console
bbcat-thumbnailer INPUT OUTPUT SIZE
```

The program passes the input bytes and filename hint to
`bbcat::decode_with_options`. bbcat normalizes every supported format into a
`Screen`: character formats expose cells, a bitmap font, and palettes, while
RIPscrip exposes an indexed raster. The example samples either representation
into one RGB scanline buffer and writes the final PNG with a small
standard-library-only encoder. This keeps `bbcat` as its only Rust dependency.
