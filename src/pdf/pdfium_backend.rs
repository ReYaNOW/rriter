use std::path::Path;

use pdfium_render::prelude::*;

use super::{LinkTarget, PageChar, PageGeom, PageLink, PageText, PdfError, PtRect};

pub(super) struct Doc<'a> {
    pub doc: PdfDocument<'a>,
    pub pages: Vec<PageGeom>,
}

const DARK_PAPER: PdfColor = PdfColor::new(30, 30, 30, 255);

pub(super) fn bind(path: &Path) -> Result<Pdfium, String> {
    Pdfium::bind_to_library(path)
        .map(Pdfium::new)
        .map_err(|error| error.to_string())
}

pub(super) fn open<'a>(pdfium: &'a Pdfium, path: &Path) -> Result<Doc<'a>, PdfError> {
    let doc = pdfium.load_pdf_from_file(path, None).map_err(|error| match error {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            PdfError::PasswordRequired
        }
        PdfiumError::PdfiumLibraryInternalError(
            PdfiumInternalError::FormatError | PdfiumInternalError::FileError,
        )
        | PdfiumError::IoError(_) => PdfError::Invalid(error.to_string()),
        _ => PdfError::Engine(error.to_string()),
    })?;
    let pages = doc
        .pages()
        .iter()
        .map(|page| PageGeom {
            width_pt: page.width().value,
            height_pt: page.height().value,
        })
        .collect::<Vec<_>>();
    if pages.is_empty() {
        return Err(PdfError::Invalid("в документе нет страниц".to_owned()));
    }
    Ok(Doc { doc, pages })
}

pub(super) fn render(
    doc: &Doc<'_>,
    page_index: usize,
    width_px: u32,
    dark: bool,
) -> Result<(u32, u32, Vec<u8>), String> {
    if width_px == 0 {
        return Err("ширина рендера равна нулю".to_owned());
    }
    let geom = doc.pages.get(page_index).ok_or_else(|| "страница вне документа".to_owned())?;
    let height_px = (width_px as f32 * geom.height_pt / geom.width_pt).round();
    if !height_px.is_finite() || height_px < 1.0 || width_px > i32::MAX as u32 || height_px > i32::MAX as f32 {
        return Err("размер страницы не поддерживается pdfium".to_owned());
    }
    let width = width_px as i32;
    let height = height_px as i32;
    let mut page = doc
        .doc
        .pages()
        .get(page_index as i32)
        .map_err(|error| error.to_string())?;
    if dark {
        page.set_content_regeneration_strategy(PdfPageContentRegenerationStrategy::Manual);
        for mut object in page.objects().iter() {
            recolor_object(&mut object);
        }
    }
    let config = PdfRenderConfig::new()
        .set_fixed_size(width, height)
        .set_clear_color(if dark { DARK_PAPER } else { PdfColor::WHITE });
    let bitmap = page
        .render_with_config(&config)
        .map_err(|error| error.to_string())?;
    let actual_width = u32::try_from(bitmap.width()).map_err(|error| error.to_string())?;
    let actual_height = u32::try_from(bitmap.height()).map_err(|error| error.to_string())?;
    let mut rgba = bitmap.as_rgba_bytes();
    for pixel in rgba.chunks_exact_mut(4) {
        pixel[3] = 255;
    }
    Ok((actual_width, actual_height, rgba))
}

fn recolor_object(object: &mut PdfPageObject<'_>) {
    match object {
        PdfPageObject::Path(path) => {
            if let Ok(color) = path.fill_color() {
                let (red, green, blue) = invert_lightness(color.red(), color.green(), color.blue());
                let _ = path.set_fill_color(PdfColor::new(red, green, blue, color.alpha()));
            }
            if let Ok(color) = path.stroke_color() {
                let (red, green, blue) = invert_lightness(color.red(), color.green(), color.blue());
                let _ = path.set_stroke_color(PdfColor::new(red, green, blue, color.alpha()));
            }
        }
        PdfPageObject::Text(text) => {
            if let Ok(color) = text.fill_color() {
                let (red, green, blue) = invert_lightness(color.red(), color.green(), color.blue());
                let _ = text.set_fill_color(PdfColor::new(red, green, blue, color.alpha()));
            }
            if let Ok(color) = text.stroke_color() {
                let (red, green, blue) = invert_lightness(color.red(), color.green(), color.blue());
                let _ = text.set_stroke_color(PdfColor::new(red, green, blue, color.alpha()));
            }
        }
        PdfPageObject::XObjectForm(form) => {
            for mut nested in form.iter() {
                recolor_object(&mut nested);
            }
        }
        _ => {}
    }
}

pub(super) fn text(doc: &Doc<'_>, page_index: usize) -> Result<(PageText, Vec<PageLink>), String> {
    let geom = doc.pages.get(page_index).ok_or_else(|| "страница вне документа".to_owned())?;
    let page = doc
        .doc
        .pages()
        .get(page_index as i32)
        .map_err(|error| error.to_string())?;
    let text = page.text().map_err(|error| error.to_string())?;
    let mut chars = Vec::new();
    for item in text.chars().iter() {
        let Some(ch) = item.unicode_char() else {
            continue;
        };
        let bounds = item
            .loose_bounds()
            .or_else(|_| item.tight_bounds())
            .map_err(|error| error.to_string())?;
        chars.push(PageChar {
            ch,
            rect: flip_rect(
                geom.height_pt,
                bounds.left().value,
                bounds.bottom().value,
                bounds.width().value,
                bounds.height().value,
            ),
        });
    }
    let chars = normalize_chars(chars);
    let mut links = Vec::new();
    for link in page.links().iter() {
        let Ok(rect) = link.rect() else {
            continue;
        };
        let rect = flip_rect(
            geom.height_pt,
            rect.left().value,
            rect.bottom().value,
            rect.width().value,
            rect.height().value,
        );
        let target = match link.action() {
            Some(PdfAction::Uri(uri)) => uri.uri().ok().and_then(|uri| {
                (uri.starts_with("http://") || uri.starts_with("https://"))
                    .then_some(LinkTarget::Uri(uri))
            }),
            Some(PdfAction::LocalDestination(action)) => {
                action.destination().ok().and_then(|destination| link_target(destination, geom.height_pt))
            }
            _ => link.destination().and_then(|destination| link_target(destination, geom.height_pt)),
        };
        if let Some(target) = target {
            links.push(PageLink { rect, target });
        }
    }
    Ok((PageText { chars }, links))
}

fn link_target(destination: PdfDestination<'_>, page_height: f32) -> Option<LinkTarget> {
    let y_pt = match destination.view_settings().ok()? {
        PdfDestinationViewSettings::SpecificCoordinatesAndZoom(_, Some(y), _) => {
            Some(flip_y(page_height, y.value))
        }
        _ => None,
    };
    let page = usize::try_from(destination.page_index().ok()?).ok()?;
    Some(LinkTarget::Page { page, y_pt })
}

fn normalize_chars(chars: Vec<PageChar>) -> Vec<PageChar> {
    let mut normalized = Vec::with_capacity(chars.len());
    let mut chars = chars.into_iter().peekable();
    while let Some(item) = chars.next() {
        if item.ch == '\r' {
            if chars.peek().is_some_and(|next| next.ch == '\n') {
                let _ = chars.next();
            }
            normalized.push(PageChar { ch: '\n', rect: PtRect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 } });
        } else if item.ch == '\n' || item.ch >= '\u{20}' {
            normalized.push(item);
        }
    }
    normalized
}

fn flip_y(height_pt: f32, y_top_from_bottom: f32) -> f32 {
    height_pt - y_top_from_bottom
}

fn flip_rect(height_pt: f32, x: f32, y_bottom: f32, w: f32, h: f32) -> PtRect {
    PtRect { x, y: height_pt - (y_bottom + h), w, h }
}

fn invert_lightness(red: u8, green: u8, blue: u8) -> (u8, u8, u8) {
    let red = f32::from(red) / 255.0;
    let green = f32::from(green) / 255.0;
    let blue = f32::from(blue) / 255.0;
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let lightness = (max + min) * 0.5;
    let delta = max - min;
    if delta == 0.0 {
        let value = ((1.0 - lightness) * 255.0).round() as u8;
        return (value, value, value);
    }
    let saturation = delta / (1.0 - (2.0 * lightness - 1.0).abs());
    let hue = if max == red {
        ((green - blue) / delta).rem_euclid(6.0)
    } else if max == green {
        (blue - red) / delta + 2.0
    } else {
        (red - green) / delta + 4.0
    } / 6.0;
    let inverted_lightness = 1.0 - lightness;
    let chroma = (1.0 - (2.0 * inverted_lightness - 1.0).abs()) * saturation;
    let section = hue * 6.0;
    let x = chroma * (1.0 - (section.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match section as u8 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let offset = inverted_lightness - chroma * 0.5;
    (
        ((r + offset) * 255.0).round() as u8,
        ((g + offset) * 255.0).round() as u8,
        ((b + offset) * 255.0).round() as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::{flip_rect, flip_y, invert_lightness, normalize_chars};
    use crate::pdf::{PageChar, PtRect};

    fn item(ch: char, x: f32) -> PageChar {
        PageChar { ch, rect: PtRect { x, y: 2.0, w: 3.0, h: 4.0 } }
    }

    #[test]
    fn normalizes_line_breaks_and_removes_control_markers() {
        let chars = normalize_chars(vec![item('a', 1.0), item('\r', 2.0), item('\n', 3.0), item('b', 4.0), item('\u{2}', 5.0), item('c', 6.0), item('\u{1}', 7.0)]);
        assert_eq!(chars.iter().map(|item| item.ch).collect::<String>(), "a\nbc");
        assert_eq!(chars[0].rect, item('a', 1.0).rect);
        assert_eq!(chars[1].rect, PtRect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 });
        assert_eq!(chars[2].rect, item('b', 4.0).rect);
        assert_eq!(chars[3].rect, item('c', 6.0).rect);
    }

    #[test]
    fn flips_points_rectangles_and_hsl_lightness() {
        assert_eq!(flip_y(792.0, 700.0), 92.0);
        assert_eq!(flip_rect(792.0, 72.0, 700.0, 10.0, 20.0).y, 72.0);
        assert_eq!(invert_lightness(0, 0, 0), (255, 255, 255));
        assert_eq!(invert_lightness(255, 0, 0), (255, 0, 0));
        assert_eq!(invert_lightness(200, 200, 200), (55, 55, 55));
    }
}
