use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// Writes the fixture PDF; a failed directory or file write is returned, not swallowed.
pub fn try_write_fixture_pdf(dir: &Path) -> std::io::Result<PathBuf> {
    write_fixture_pdf_with_target_height(dir, "rriter-fixture.pdf", 792.0, 792.0)
}

#[cfg(test)]
pub fn write_fixture_pdf(dir: &Path) -> PathBuf {
    try_write_fixture_pdf(dir).expect("write fixture pdf")
}

#[cfg(test)]
pub fn write_fixture_pdf_mixed(dir: &Path) -> PathBuf {
    write_fixture_pdf_with_target_height(dir, "rriter-fixture-mixed.pdf", 612.0, 500.0)
        .expect("write mixed fixture pdf")
}

fn write_fixture_pdf_with_target_height(
    dir: &Path,
    filename: &str,
    target_height: f32,
    destination_y: f32,
) -> std::io::Result<PathBuf> {
    let page_one = b"BT /F1 24 Tf 72 700 Td (Hello PDF viewer) Tj 0 -40 Td (Go to second) Tj ET";
    let page_two = b"BT /F1 24 Tf 72 700 Td (Second page target) Tj ET";
    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [4 0 R 10 0 R 12 0 R] /Count 3 >>".to_owned(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents 5 0 R /Annots [6 0 R 7 0 R 8 0 R 9 0 R 13 0 R] >>".to_owned(),
        stream_object(page_one),
        "<< /Type /Annot /Subtype /Link /Rect [72 690 300 726] /Border [0 0 0] /A << /S /URI /URI (https://example.com/) >> >>".to_owned(),
        format!("<< /Type /Annot /Subtype /Link /Rect [72 650 240 686] /Border [0 0 0] /A << /S /GoTo /D [10 0 R /XYZ 0 {destination_y} 0] >> >>"),
        "<< /Type /Annot /Subtype /Link /Rect [72 600 200 630] /Border [0 0 0] /A << /S /URI /URI (mailto:x@example.com) >> >>".to_owned(),
        "<< /Type /Annot /Subtype /Link /Rect [72 560 200 590] /Border [0 0 0] /Dest [12 0 R /Fit] >>".to_owned(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 {target_height}] /Resources << /Font << /F1 3 0 R >> >> /Contents 11 0 R >>"),
        stream_object(page_two),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_owned(),
        "<< /Type /Annot /Subtype /Link /Rect [72 520 200 550] /Border [0 0 0] /A << /S /GoTo /D [99 /XYZ 0 792 0] >> >>".to_owned(),
    ];
    write_pdf(dir, filename, &objects)
}

/// One page with a text highlight the way LibreOffice writes it: a yellow
/// filled path (`1 1 0 rg … re f`) under black text. The yellow box spans
/// x 72..540, y 300..700 (PDF points, origin bottom-left); the text sits at
/// x 150..~420, y 450..~560, so the box corners are highlight without text.
#[cfg(test)]
pub fn write_fixture_pdf_highlight(dir: &Path) -> PathBuf {
    let content = b"1 1 0 rg 72 300 468 400 re f 0 0 0 rg BT /F1 150 Tf 150 450 Td (HH) Tj ET";
    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [4 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents 5 0 R >>".to_owned(),
        stream_object(content),
    ];
    write_pdf(dir, "rriter-fixture-highlight.pdf", &objects).expect("write highlight fixture pdf")
}

fn write_pdf(dir: &Path, filename: &str, objects: &[String]) -> std::io::Result<PathBuf> {
    let path = dir.join(filename);
    let mut pdf = String::from("%PDF-1.4\n");
    let mut offsets = Vec::with_capacity(objects.len() + 1);
    offsets.push(0usize);
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        let _ = writeln!(pdf, "{} 0 obj\n{}\nendobj", index + 1, object);
    }
    let xref_offset = pdf.len();
    let _ = writeln!(pdf, "xref\n0 {}\n0000000000 65535 f ", objects.len() + 1);
    for offset in offsets.iter().skip(1) {
        let _ = writeln!(pdf, "{offset:010} 00000 n ");
    }
    let _ = writeln!(
        pdf,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF",
        objects.len() + 1
    );
    fs::create_dir_all(dir)?;
    fs::write(&path, pdf.as_bytes())?;
    Ok(path)
}

fn stream_object(bytes: &[u8]) -> String {
    let mut object = format!("<< /Length {} >>\nstream\n", bytes.len());
    object.push_str(std::str::from_utf8(bytes).unwrap_or_default());
    object.push_str("\nendstream");
    object
}

#[cfg(test)]
pub fn write_garbage(dir: &Path) -> PathBuf {
    let path = dir.join("rriter-garbage.pdf");
    let _ = fs::create_dir_all(dir);
    let _ = fs::write(&path, b"this is not a pdf");
    path
}

#[cfg(test)]
pub fn write_empty(dir: &Path) -> PathBuf {
    let path = dir.join("rriter-empty.pdf");
    let _ = fs::create_dir_all(dir);
    let _ = fs::write(&path, []);
    path
}
