//! In-process media decoding: raster images, SVG and Mermaid diagrams to RGBA8.
//!
//! This code runs inside the helper process only (`render_helper`), never in the editor:
//! a decoder panic aborts the process in release builds. Every input is untrusted, so
//! decoding is limited (`image::Limits`), SVG never loads external resources, and text
//! uses only the two fonts embedded in the editor.

use std::hash::{Hash, Hasher};
use std::io::{Cursor, Read};
use std::path::Path;
use std::sync::Arc;

use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder, ImageError, ImageReader, Limits, RgbaImage};

use super::{MediaError, MediaKind, MediaPixels};

/// Longest side of any decoded raster input, in pixels.
const MAX_INPUT_SIDE: u32 = 16384;
/// Decoder allocation budget in bytes.
const MAX_DECODE_ALLOC: u64 = 128 * 1024 * 1024;
/// Longest side of the produced raster, in pixels.
const MAX_RASTER_SIDE: u32 = 4096;
/// Longest accepted SVG document, compressed or after inflating `.svgz`, in bytes.
const MAX_SVG_BYTES: u64 = 20 * 1024 * 1024;
/// How much of the input is inspected when sniffing an SVG signature.
const SVG_SNIFF_BYTES: usize = 1024;

const INTER_FONT: &[u8] = include_bytes!("../fonts/Inter-Regular.otf");
const MONO_FONT: &[u8] = include_bytes!("../fonts/JetBrainsMonoNerdFont-Regular.ttf");

/// Decodes `bytes` of the given kind into RGBA8 pixels (straight alpha).
///
/// `scale` is the UI scale and `max_raster_w` the width of the text column; see
/// `raster_target_size`. `stamp` of the result is always `None`, the loader sets it.
pub(crate) fn decode_in_process(
    kind: MediaKind,
    bytes: &[u8],
    scale: f32,
    max_raster_w: u32,
) -> Result<MediaPixels, MediaError> {
    match kind {
        // The cache can hand over an SVG that only its Content-Type marked as a raster.
        MediaKind::Raster if looks_like_svg(bytes) => decode_svg(bytes, scale, max_raster_w),
        MediaKind::Raster => decode_raster(bytes, scale, max_raster_w),
        MediaKind::Svg => decode_svg(bytes, scale, max_raster_w),
        MediaKind::Mermaid => decode_mermaid(bytes, scale, max_raster_w),
    }
}

/// Target raster size for an image of the given natural size: `natural * scale`, shrunk
/// proportionally to `max_raster_w` (0 means no width limit) and to `MAX_RASTER_SIDE`
/// on the long side, rounded to whole pixels and at least 1.
pub(crate) fn raster_target_size(
    natural_w: f32,
    natural_h: f32,
    scale: f32,
    max_raster_w: u32,
) -> (u32, u32) {
    let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    let mut w = natural_w * scale;
    let mut h = natural_h * scale;
    if max_raster_w > 0 && w > max_raster_w as f32 {
        let k = max_raster_w as f32 / w;
        w *= k;
        h *= k;
    }
    let long = w.max(h);
    if long > MAX_RASTER_SIDE as f32 {
        let k = MAX_RASTER_SIDE as f32 / long;
        w *= k;
        h *= k;
    }
    (round_side(w), round_side(h))
}

fn round_side(value: f32) -> u32 {
    if value.is_finite() {
        (value.round() as u32).clamp(1, MAX_RASTER_SIDE)
    } else {
        1
    }
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(SVG_SNIFF_BYTES)];
    head.windows(4).any(|window| window == b"<svg")
}

fn decode_raster(bytes: &[u8], scale: f32, max_raster_w: u32) -> Result<MediaPixels, MediaError> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| MediaError::Decode)?;
    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    limits.max_image_width = Some(MAX_INPUT_SIDE);
    limits.max_image_height = Some(MAX_INPUT_SIDE);
    reader.limits(limits);
    let map_error = |error: ImageError| match error {
        ImageError::Limits(_) => MediaError::TooManyPixels,
        _ => MediaError::Decode,
    };
    // `ImageReader::decode` reserves only the decoder's native buffer; `into_decoder` is
    // used so that the native buffer and the RGBA8 copy made below are both budgeted
    // before anything is allocated.
    let decoder = reader.into_decoder().map_err(map_error)?;
    let (width, height) = decoder.dimensions();
    let rgba_bytes = u64::from(width) * u64::from(height) * 4;
    if rgba_bytes > MAX_DECODE_ALLOC || decoder.total_bytes() > MAX_DECODE_ALLOC {
        return Err(MediaError::TooManyPixels);
    }
    // Animated formats (GIF, WebP) yield their first frame.
    let image = DynamicImage::from_decoder(decoder).map_err(map_error)?;
    let rgba = image.into_rgba8();
    let (target_w, target_h) = raster_target_size(width as f32, height as f32, scale, max_raster_w);
    // Sized natural x scale (upscaling included) so the raster matches the display size;
    // the target is already clamped to `max_raster_w` and `MAX_RASTER_SIDE`.
    let rgba = if (target_w, target_h) != (width, height) {
        image::imageops::resize(&rgba, target_w, target_h, FilterType::Triangle)
    } else {
        rgba
    };
    Ok(pixels(width as f32, height as f32, rgba))
}

fn pixels(natural_w: f32, natural_h: f32, image: RgbaImage) -> MediaPixels {
    MediaPixels {
        natural_w,
        natural_h,
        raster_w: image.width(),
        raster_h: image.height(),
        rgba: image.into_raw(),
        stamp: None,
    }
}

/// Font base with only the editor fonts; system fonts are never scanned.
fn editor_fontdb() -> (usvg::fontdb::Database, String) {
    let mut db = usvg::fontdb::Database::new();
    db.load_font_source(usvg::fontdb::Source::Binary(Arc::new(INTER_FONT)));
    let sans = family_of_last_face(&db).unwrap_or_else(|| "Inter".to_string());
    db.load_font_source(usvg::fontdb::Source::Binary(Arc::new(MONO_FONT)));
    let mono = family_of_last_face(&db).unwrap_or_else(|| sans.clone());
    db.set_sans_serif_family(sans.clone());
    db.set_serif_family(sans.clone());
    db.set_cursive_family(sans.clone());
    db.set_fantasy_family(sans.clone());
    db.set_monospace_family(mono);
    (db, sans)
}

fn family_of_last_face(db: &usvg::fontdb::Database) -> Option<String> {
    db.faces().last()?.families.first().map(|(name, _)| name.clone())
}

fn svg_options() -> usvg::Options<'static> {
    let (db, sans) = editor_fontdb();
    usvg::Options {
        resources_dir: None,
        font_family: sans,
        fontdb: Arc::new(db),
        // Embedded `data:` images are still resolved (only nested SVG draws, resvg is built
        // without raster images); every other `<image href>` is dropped, so neither local
        // files nor network addresses are ever read on behalf of a document.
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
            resolve_string: Box::new(|_, _| None),
        },
        ..usvg::Options::default()
    }
}

fn decode_svg(bytes: &[u8], scale: f32, max_raster_w: u32) -> Result<MediaPixels, MediaError> {
    decode_svg_with(bytes, &svg_options(), scale, max_raster_w)
}

fn decode_svg_with(
    bytes: &[u8],
    options: &usvg::Options<'_>,
    scale: f32,
    max_raster_w: u32,
) -> Result<MediaPixels, MediaError> {
    if bytes.len() as u64 > MAX_SVG_BYTES {
        return Err(MediaError::TooLarge);
    }
    // `usvg::Tree::from_data` would gunzip `.svgz` with an unbounded read, so the
    // inflating is done here with a hard cap and usvg only ever sees plain XML.
    let inflated;
    let xml = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(bytes)
            .take(MAX_SVG_BYTES + 1)
            .read_to_end(&mut out)
            .map_err(|_| MediaError::Decode)?;
        if out.len() as u64 > MAX_SVG_BYTES {
            return Err(MediaError::TooLarge);
        }
        inflated = out;
        &inflated[..]
    } else {
        bytes
    };
    let tree = usvg::Tree::from_data(xml, options).map_err(|_| MediaError::Decode)?;
    rasterize_svg(&tree, scale, max_raster_w)
}

fn rasterize_svg(
    tree: &usvg::Tree,
    scale: f32,
    max_raster_w: u32,
) -> Result<MediaPixels, MediaError> {
    let size = tree.size();
    let (natural_w, natural_h) = (size.width(), size.height());
    let (target_w, target_h) = raster_target_size(natural_w, natural_h, scale, max_raster_w);
    let mut pixmap = tiny_skia::Pixmap::new(target_w, target_h).ok_or(MediaError::Decode)?;
    let transform =
        tiny_skia::Transform::from_scale(target_w as f32 / natural_w, target_h as f32 / natural_h);
    resvg::render(tree, transform, &mut pixmap.as_mut());
    // tiny-skia stores premultiplied alpha; the GPU path blends with SRC_ALPHA /
    // ONE_MINUS_SRC_ALPHA, so the texture must hold straight alpha.
    let mut rgba = Vec::with_capacity(pixmap.data().len());
    for pixel in pixmap.pixels() {
        let color = pixel.demultiply();
        rgba.extend_from_slice(&[color.red(), color.green(), color.blue(), color.alpha()]);
    }
    Ok(MediaPixels {
        natural_w,
        natural_h,
        raster_w: target_w,
        raster_h: target_h,
        rgba,
        stamp: None,
    })
}

/// The single font family Mermaid text is laid out and drawn with. It must be exactly this
/// string (no fallback list): mermaid-rs-renderer keys its font lookup on the raw value.
const MERMAID_FONT_FAMILY: &str = "Inter";
/// Key mermaid-rs-renderer 0.3.1 hashes to name its disk font cache file
/// (`text_metrics.rs`: `FONT_CACHE_VERSION` + ":" + family).
const MMDR_FONT_CACHE_KEY: &str = "v2-font-family-case:Inter";

/// Pre-seeds the font cache of mermaid-rs-renderer under `cache_root/mmdr/font-cache`.
///
/// mermaid-rs-renderer measures text with a font it finds in `$XDG_CACHE_HOME/mmdr/
/// font-cache` and, on a miss, scans the system fonts and copies the match there. Seeding
/// the entry for `MERMAID_FONT_FAMILY` with the embedded Inter makes layout use the font
/// resvg draws with, skips the scan, and keeps the files inside our own cache directory.
/// The helper must be started with `XDG_CACHE_HOME=cache_root`; the crate is pinned to an
/// exact version because the file layout is private.
pub(crate) fn prepare_mermaid_font_cache(cache_root: &Path) -> std::io::Result<()> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    MMDR_FONT_CACHE_KEY.hash(&mut hasher);
    let name = format!("{:x}", hasher.finish());
    let dir = cache_root.join("mmdr").join("font-cache");
    std::fs::create_dir_all(&dir)?;
    let font_path = dir.join(format!("{name}.font"));
    let meta_path = dir.join(format!("{name}.meta"));
    let seeded = std::fs::metadata(&font_path).is_ok_and(|meta| meta.len() == INTER_FONT.len() as u64)
        && std::fs::read_to_string(&meta_path).is_ok_and(|text| text.trim() == "0");
    if seeded {
        return Ok(());
    }
    // The reader needs both files; write the meta first and publish the font by rename, so
    // a concurrent helper never sees a half-written font.
    std::fs::write(&meta_path, "0")?;
    let tmp_path = dir.join(format!("{name}.{}.tmp", std::process::id()));
    std::fs::write(&tmp_path, INTER_FONT)?;
    std::fs::rename(&tmp_path, &font_path)
}

/// Dark theme for the editor background: light text, grey lines, transparent canvas.
fn mermaid_options() -> mermaid_rs_renderer::RenderOptions {
    let mut theme = mermaid_rs_renderer::Theme::dark();
    theme.background = "transparent".to_string();
    theme.font_family = MERMAID_FONT_FAMILY.to_string();
    mermaid_rs_renderer::RenderOptions {
        theme,
        layout: mermaid_rs_renderer::LayoutConfig::default(),
    }
}

fn decode_mermaid(bytes: &[u8], scale: f32, max_raster_w: u32) -> Result<MediaPixels, MediaError> {
    let source = std::str::from_utf8(bytes)
        .map_err(|_| MediaError::Mermaid("диаграмма не в UTF-8".to_string()))?;
    if source.trim().is_empty() {
        return Err(MediaError::Mermaid("пустая диаграмма".to_string()));
    }
    let options = svg_options();
    let svg = mermaid_rs_renderer::render_strict(source, mermaid_options())
        .map_err(|error| MediaError::Mermaid(mermaid_error_text(&error.to_string())))?;
    let tree = usvg::Tree::from_data(svg.as_bytes(), &options)
        .map_err(|error| MediaError::Mermaid(mermaid_error_text(&error.to_string())))?;
    rasterize_svg(&tree, scale, max_raster_w)
}

/// One trimmed line, never empty: it is shown under the diagram header.
fn mermaid_error_text(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|line| !line.is_empty());
    line.unwrap_or("ошибка Mermaid").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, Rgba};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn encode(image: &RgbaImage, format: ImageFormat) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        // JPEG has no alpha channel, so encode every format from RGB.
        DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(image.clone()).into_rgb8())
            .write_to(&mut out, format)
            .unwrap();
        out.into_inner()
    }

    fn sample(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_fn(width, height, |x, y| Rgba([(x * 40) as u8, (y * 40) as u8, 200, 255]))
    }

    fn png_header(width: u32, height: u32) -> Vec<u8> {
        let mut ihdr = b"IHDR".to_vec();
        ihdr.extend_from_slice(&width.to_be_bytes());
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
        let mut crc = flate2::Crc::new();
        crc.update(&ihdr);
        let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        out.extend_from_slice(&13u32.to_be_bytes());
        out.extend_from_slice(&ihdr);
        out.extend_from_slice(&crc.sum().to_be_bytes());
        out
    }

    /// A PNG that declares an 8-bit image of the given size and PNG colour type; only the header and a
    /// stub IDAT chunk are present, so the file stays tiny whatever the dimensions.
    fn png_stub(width: u32, height: u32, color_type: u8) -> Vec<u8> {
        fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
            let mut crc = flate2::Crc::new();
            crc.update(kind);
            crc.update(data);
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            out.extend_from_slice(kind);
            out.extend_from_slice(data);
            out.extend_from_slice(&crc.sum().to_be_bytes());
        }
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&width.to_be_bytes());
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.extend_from_slice(&[8, color_type, 0, 0, 0]);
        let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        chunk(&mut out, b"IHDR", &ihdr);
        chunk(&mut out, b"IDAT", &[0x78, 0x9c, 0x63, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01]);
        out
    }

    #[test]
    fn png_natural_size_and_upscale() {
        let png = encode(&sample(3, 2), ImageFormat::Png);
        let pixels = decode_in_process(MediaKind::Raster, &png, 2.0, 1000).unwrap();
        assert_eq!((pixels.natural_w, pixels.natural_h), (3.0, 2.0));
        assert_eq!((pixels.raster_w, pixels.raster_h), (6, 4));
        assert_eq!(pixels.rgba.len(), 6 * 4 * 4);
        assert!(pixels.stamp.is_none());
        // The upscaled raster is still clamped to the text column width.
        let pixels = decode_in_process(MediaKind::Raster, &png, 2.0, 4).unwrap();
        assert_eq!((pixels.raster_w, pixels.raster_h), (4, 3));
    }

    #[test]
    fn rgba_copy_is_budgeted_before_conversion() {
        // 16384x8192 greyscale is exactly 128 MiB natively (within max_alloc) but 512 MiB
        // once converted to RGBA8.
        assert_eq!(
            decode_in_process(MediaKind::Raster, &png_stub(16384, 8192, 0), 1.0, 100).unwrap_err(),
            MediaError::TooManyPixels
        );
        // A full-size RGBA header is 1 GiB natively: the decoder budget catches it.
        assert_eq!(
            decode_in_process(MediaKind::Raster, &png_stub(16384, 16384, 6), 1.0, 100).unwrap_err(),
            MediaError::TooManyPixels
        );
    }

    #[test]
    fn raster_shrinks_proportionally_to_max_width() {
        let png = encode(&sample(200, 100), ImageFormat::Png);
        let pixels = decode_in_process(MediaKind::Raster, &png, 1.0, 50).unwrap();
        assert_eq!((pixels.natural_w, pixels.natural_h), (200.0, 100.0));
        assert_eq!((pixels.raster_w, pixels.raster_h), (50, 25));
        assert_eq!(pixels.rgba.len(), 50 * 25 * 4);
    }

    #[test]
    fn target_size_rules() {
        assert_eq!(raster_target_size(100.0, 50.0, 1.0, 0), (100, 50));
        assert_eq!(raster_target_size(100.0, 50.0, 2.0, 150), (150, 75));
        assert_eq!(raster_target_size(10.0, 20000.0, 1.0, 100), (2, 4096));
        assert_eq!(raster_target_size(1.0, 1.0, 0.1, 100), (1, 1));
        assert_eq!(raster_target_size(10.0, 10.0, f32::NAN, 100), (10, 10));
    }

    #[test]
    fn jpeg_gif_webp_decode() {
        for format in [ImageFormat::Jpeg, ImageFormat::Gif, ImageFormat::WebP] {
            let bytes = encode(&sample(8, 6), format);
            let pixels = decode_in_process(MediaKind::Raster, &bytes, 1.0, 100)
                .unwrap_or_else(|error| panic!("{format:?}: {error:?}"));
            assert_eq!((pixels.natural_w, pixels.natural_h), (8.0, 6.0), "{format:?}");
            assert_eq!(pixels.rgba.len(), 8 * 6 * 4, "{format:?}");
        }
    }

    #[test]
    fn garbage_and_truncated_raster_is_decode_error() {
        assert_eq!(
            decode_in_process(MediaKind::Raster, b"definitely not an image", 1.0, 100).unwrap_err(),
            MediaError::Decode
        );
        assert_eq!(
            decode_in_process(MediaKind::Raster, &[], 1.0, 100).unwrap_err(),
            MediaError::Decode
        );
        for format in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::Gif, ImageFormat::WebP] {
            let bytes = encode(&sample(16, 16), format);
            let cut = &bytes[..bytes.len() / 2];
            assert_eq!(
                decode_in_process(MediaKind::Raster, cut, 1.0, 100).unwrap_err(),
                MediaError::Decode,
                "{format:?}"
            );
        }
    }

    #[test]
    fn oversize_dimensions_are_rejected_before_allocation() {
        for (w, h) in [(100_000, 100_000), (16_385, 1), (1, 16_385)] {
            assert_eq!(
                decode_in_process(MediaKind::Raster, &png_header(w, h), 1.0, 100).unwrap_err(),
                MediaError::TooManyPixels,
                "{w}x{h}"
            );
        }
    }

    #[test]
    fn svg_natural_size() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="20"><rect width="10" height="20" fill="#00f"/></svg>"##;
        let pixels = decode_in_process(MediaKind::Svg, svg, 1.0, 1000).unwrap();
        assert_eq!((pixels.natural_w, pixels.natural_h), (10.0, 20.0));
        assert_eq!((pixels.raster_w, pixels.raster_h), (10, 20));
        let pixels = decode_in_process(MediaKind::Svg, svg, 2.0, 1000).unwrap();
        assert_eq!((pixels.raster_w, pixels.raster_h), (20, 40));
        assert_eq!(pixels.rgba.len(), 20 * 40 * 4);
    }

    #[test]
    fn svg_marked_as_raster_is_sniffed() {
        let svg = br#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"/>"#;
        let pixels = decode_in_process(MediaKind::Raster, svg, 1.0, 100).unwrap();
        assert_eq!((pixels.natural_w, pixels.natural_h), (4.0, 4.0));
    }

    #[test]
    fn svg_text_renders_with_embedded_fonts() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="30"><text x="5" y="22" font-size="20" fill="#000">Build</text></svg>"##;
        let pixels = decode_in_process(MediaKind::Svg, svg, 1.0, 1000).unwrap();
        let opaque = pixels.rgba.chunks_exact(4).filter(|px| px[3] == 255).count();
        assert!(opaque > 20, "text drew only {opaque} opaque pixels");
    }

    #[test]
    fn svg_alpha_is_straight() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4" fill="#ff0000" fill-opacity="0.5"/></svg>"##;
        let pixels = decode_in_process(MediaKind::Svg, svg, 1.0, 100).unwrap();
        let px = &pixels.rgba[..4];
        assert!((i32::from(px[3]) - 128).abs() <= 1, "alpha {}", px[3]);
        assert!(px[0] >= 250 && px[1] == 0 && px[2] == 0, "premultiplied colour: {px:?}");
    }

    #[test]
    fn svg_external_images_are_never_loaded() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        // A local SVG with an opaque rect: nested SVG is drawn even though resvg is built
        // without raster images, so an all-transparent result proves the href was dropped.
        let file = std::env::temp_dir().join(format!("rriter_svg_ext_{}.svg", std::process::id()));
        std::fs::write(
            &file,
            br##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8" fill="#f00"/></svg>"##,
        )
        .unwrap();
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="8" height="8"><image href="http://127.0.0.1:{port}/x.svg" width="8" height="8"/><image xlink:href="{}" width="8" height="8"/><image href="x.svg" width="8" height="8"/></svg>"#,
            file.display()
        );
        let draws = |pixels: &MediaPixels| pixels.rgba.chunks_exact(4).any(|px| px[3] != 0);

        // Control: the stock usvg resolver does load the local file, so the assertions
        // below cannot pass by accident.
        let mut stock = svg_options();
        stock.image_href_resolver.resolve_string = usvg::ImageHrefResolver::default_string_resolver();
        assert!(draws(&decode_svg_with(svg.as_bytes(), &stock, 1.0, 100).unwrap()));

        // The production resolver is asked for every href and refuses all of them.
        let asked = Arc::new(AtomicUsize::new(0));
        let answered = Arc::new(AtomicUsize::new(0));
        let mut options = svg_options();
        let production = std::mem::replace(
            &mut options.image_href_resolver.resolve_string,
            Box::new(|_, _| None),
        );
        let (asked_in, answered_in) = (asked.clone(), answered.clone());
        options.image_href_resolver.resolve_string = Box::new(move |href, opts| {
            asked_in.fetch_add(1, Ordering::SeqCst);
            let image = production(href, opts);
            if image.is_some() {
                answered_in.fetch_add(1, Ordering::SeqCst);
            }
            image
        });
        let pixels = decode_svg_with(svg.as_bytes(), &options, 1.0, 100).unwrap();
        // The same through the public entry point.
        let public = decode_in_process(MediaKind::Svg, svg.as_bytes(), 1.0, 100);
        std::fs::remove_file(&file).ok();
        assert_eq!(asked.load(Ordering::SeqCst), 3, "resolver was not asked for every href");
        assert_eq!(answered.load(Ordering::SeqCst), 0, "resolver returned an image");
        assert!(!draws(&pixels), "external image was drawn");
        assert!(!draws(&public.unwrap()), "external image was drawn via decode_in_process");
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(
            listener.accept().map(|_| ()).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock,
            "the SVG decoder opened a connection"
        );
    }

    fn gzip(bytes: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn svg_input_size_is_limited() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4"/></svg>"##;
        // A small svgz still works.
        let pixels = decode_in_process(MediaKind::Svg, &gzip(svg), 1.0, 100).unwrap();
        assert_eq!((pixels.natural_w, pixels.natural_h), (4.0, 4.0));
        // Plain input over the limit.
        let big = vec![b' '; MAX_SVG_BYTES as usize + 1];
        assert_eq!(decode_in_process(MediaKind::Svg, &big, 1.0, 100).unwrap_err(), MediaError::TooLarge);
        // Gzip bomb: a few KB that inflate past the limit.
        let bomb = gzip(&vec![b' '; MAX_SVG_BYTES as usize + 4096]);
        assert!(bomb.len() < 256 * 1024, "bomb is {} bytes", bomb.len());
        assert_eq!(decode_in_process(MediaKind::Svg, &bomb, 1.0, 100).unwrap_err(), MediaError::TooLarge);
        // Content-Type said raster but the bytes are a gzip stream: never decoded as image.
        assert!(decode_in_process(MediaKind::Raster, &bomb, 1.0, 100).is_err());
    }

    #[test]
    fn broken_svg_is_decode_error() {
        for bytes in [&b"<svg width=\"10\""[..], b"", b"\xff\xfe\x00garbage", b"<html></html>"] {
            assert_eq!(
                decode_in_process(MediaKind::Svg, bytes, 1.0, 100).unwrap_err(),
                MediaError::Decode
            );
        }
    }

    /// Points `XDG_CACHE_HOME` of this test process at a per-process directory seeded by
    /// `prepare_mermaid_font_cache`, like the helper launcher does for the child process.
    struct SeededMermaidCache {
        root: PathBuf,
        previous: Option<std::ffi::OsString>,
    }

    impl SeededMermaidCache {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("rriter_mmdr_cache_{}", std::process::id()));
            std::fs::remove_dir_all(&root).ok();
            prepare_mermaid_font_cache(&root).unwrap();
            let previous = std::env::var_os("XDG_CACHE_HOME");
            // SAFETY: edition-2024 `set_var`; this is the only test that renders Mermaid (the
            // variable is read on the first render of the process), and it restores the old
            // value on drop.
            unsafe { std::env::set_var("XDG_CACHE_HOME", &root) };
            Self { root, previous }
        }

        fn font_files(&self) -> Vec<String> {
            let dir = self.root.join("mmdr").join("font-cache");
            let mut names: Vec<String> = std::fs::read_dir(dir)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }
    }

    impl Drop for SeededMermaidCache {
        fn drop(&mut self) {
            // SAFETY: see `new`.
            unsafe {
                match &self.previous {
                    Some(value) => std::env::set_var("XDG_CACHE_HOME", value),
                    None => std::env::remove_var("XDG_CACHE_HOME"),
                }
            }
            std::fs::remove_dir_all(&self.root).ok();
        }
    }

    #[test]
    fn mermaid_uses_seeded_font_cache() {
        let cache = SeededMermaidCache::new();
        // Seeding: one .font (the embedded Inter) and its .meta, nothing else.
        let seeded = cache.font_files();
        assert_eq!(seeded.len(), 2, "{seeded:?}");
        let font = seeded.iter().find(|name| name.ends_with(".font")).unwrap();
        let on_disk = std::fs::read(cache.root.join("mmdr").join("font-cache").join(font)).unwrap();
        assert_eq!(on_disk, INTER_FONT);
        // Seeding twice is a no-op.
        prepare_mermaid_font_cache(&cache.root).unwrap();
        assert_eq!(cache.font_files(), seeded);
        // The family resvg resolves is the one mermaid lays out with.
        assert_eq!(editor_fontdb().1, MERMAID_FONT_FAMILY);

        mermaid_flowchart_renders();
        mermaid_errors_carry_text();
        // A cache miss would have made mmdr scan the system fonts and copy a font here.
        assert_eq!(cache.font_files(), seeded, "mmdr wrote a font of its own");
    }

    fn mermaid_flowchart_renders() {
        let pixels = decode_in_process(MediaKind::Mermaid, b"graph TD; A-->B", 1.0, 800).unwrap();
        assert!(pixels.natural_w > 0.0 && pixels.natural_h > 0.0);
        assert!(pixels.raster_w > 0 && pixels.raster_h > 0);
        assert_eq!(pixels.rgba.len(), (pixels.raster_w * pixels.raster_h * 4) as usize);
        assert!(pixels.rgba.chunks_exact(4).any(|px| px[3] > 0), "nothing was drawn");
        // The canvas is transparent: the corner carries no background.
        assert_eq!(pixels.rgba[3], 0);
    }

    fn mermaid_errors_carry_text() {
        for source in [&b""[..], b"   \n", b"this is not mermaid at all", b"graph TD; A--", b"\xff\xfe"] {
            match decode_in_process(MediaKind::Mermaid, source, 1.0, 800) {
                Err(MediaError::Mermaid(text)) => assert!(!text.trim().is_empty(), "{source:?}"),
                other => panic!("{source:?}: {other:?}"),
            }
        }
    }
}
