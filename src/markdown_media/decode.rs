//! In-process media decoding: raster images, SVG and Mermaid diagrams to RGBA8.
//!
//! This code runs inside the helper process only (`render_helper`), never in the editor:
//! a decoder panic aborts the process in release builds. Every input is untrusted, so
//! decoding is limited (`image::Limits`), SVG never loads external resources, and text
//! uses only the two fonts embedded in the editor.

use std::io::Cursor;
use std::sync::Arc;

use image::imageops::FilterType;
use image::{ImageError, ImageReader, Limits, RgbaImage};

use super::{MediaError, MediaKind, MediaPixels};

/// Longest side of any decoded raster input, in pixels.
const MAX_INPUT_SIDE: u32 = 16384;
/// Decoder allocation budget in bytes.
const MAX_DECODE_ALLOC: u64 = 128 * 1024 * 1024;
/// Longest side of the produced raster, in pixels.
const MAX_RASTER_SIDE: u32 = 4096;
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
    // Animated formats (GIF, WebP) yield their first frame.
    let image = reader.decode().map_err(|error| match error {
        ImageError::Limits(_) => MediaError::TooManyPixels,
        _ => MediaError::Decode,
    })?;
    let rgba = image.into_rgba8();
    let (width, height) = rgba.dimensions();
    let (target_w, target_h) = raster_target_size(width as f32, height as f32, scale, max_raster_w);
    let (raster_w, raster_h) = (target_w.min(width), target_h.min(height));
    let rgba = if raster_w < width || raster_h < height {
        image::imageops::resize(&rgba, raster_w, raster_h, FilterType::Triangle)
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
        // Embedded `data:` images keep working; every other `<image href>` is dropped, so
        // neither local files nor network addresses are ever read on behalf of a document.
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
            resolve_string: Box::new(|_, _| None),
        },
        ..usvg::Options::default()
    }
}

fn decode_svg(bytes: &[u8], scale: f32, max_raster_w: u32) -> Result<MediaPixels, MediaError> {
    let options = svg_options();
    let tree = usvg::Tree::from_data(bytes, &options).map_err(|_| MediaError::Decode)?;
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

/// Dark theme for the editor background: light text, grey lines, transparent canvas.
fn mermaid_options(font_family: &str) -> mermaid_rs_renderer::RenderOptions {
    let mut theme = mermaid_rs_renderer::Theme::dark();
    theme.background = "transparent".to_string();
    theme.font_family = font_family.to_string();
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
    let render_options = mermaid_options(&format!("{}, sans-serif", options.font_family));
    let svg = mermaid_rs_renderer::render_strict(source, render_options)
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

    #[test]
    fn png_natural_size_and_no_upscale() {
        let png = encode(&sample(3, 2), ImageFormat::Png);
        let pixels = decode_in_process(MediaKind::Raster, &png, 2.0, 1000).unwrap();
        assert_eq!((pixels.natural_w, pixels.natural_h), (3.0, 2.0));
        // The target is 6x4, but rasters are only ever shrunk; the GPU scales them up.
        assert_eq!(raster_target_size(3.0, 2.0, 2.0, 1000), (6, 4));
        assert_eq!((pixels.raster_w, pixels.raster_h), (3, 2));
        assert_eq!(pixels.rgba.len(), 3 * 2 * 4);
        assert!(pixels.stamp.is_none());
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
        let file = std::env::temp_dir().join(format!("rriter_svg_ext_{}.png", std::process::id()));
        std::fs::write(&file, encode(&sample(4, 4), ImageFormat::Png)).unwrap();
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="8" height="8"><image href="http://127.0.0.1:{port}/x.png" width="8" height="8"/><image xlink:href="{}" width="8" height="8"/><image href="x.png" width="8" height="8"/></svg>"#,
            file.display()
        );
        let pixels = decode_in_process(MediaKind::Svg, svg.as_bytes(), 1.0, 100).unwrap();
        std::fs::remove_file(&file).ok();
        assert!(pixels.rgba.chunks_exact(4).all(|px| px[3] == 0), "external image was drawn");
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(
            listener.accept().map(|_| ()).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock,
            "the SVG decoder opened a connection"
        );
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

    #[test]
    fn mermaid_flowchart_renders() {
        let pixels = decode_in_process(MediaKind::Mermaid, b"graph TD; A-->B", 1.0, 800).unwrap();
        assert!(pixels.natural_w > 0.0 && pixels.natural_h > 0.0);
        assert!(pixels.raster_w > 0 && pixels.raster_h > 0);
        assert_eq!(pixels.rgba.len(), (pixels.raster_w * pixels.raster_h * 4) as usize);
        assert!(pixels.rgba.chunks_exact(4).any(|px| px[3] > 0), "nothing was drawn");
        // The canvas is transparent: the corner carries no background.
        assert_eq!(pixels.rgba[3], 0);
    }

    #[test]
    fn mermaid_errors_carry_text() {
        for source in [&b""[..], b"   \n", b"this is not mermaid at all", b"graph TD; A--", b"\xff\xfe"] {
            match decode_in_process(MediaKind::Mermaid, source, 1.0, 800) {
                Err(MediaError::Mermaid(text)) => assert!(!text.trim().is_empty(), "{source:?}"),
                other => panic!("{source:?}: {other:?}"),
            }
        }
    }
}
