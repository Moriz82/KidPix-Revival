//! Printing: the picture is written as a one-page PDF and handed to the system.

use crate::gfx::Image;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Minimal PDF: one page per image (RGB, Flate-compressed), each centred on a US-Letter page.
pub fn write_pdf_pages(pages: &[Image], path: &Path) -> std::io::Result<()> {
    let mut out: Vec<u8> = Vec::new();
    let mut offs: Vec<usize> = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n");
    let obj = |out: &mut Vec<u8>, offs: &mut Vec<usize>, body: &[u8]| {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", offs.len()).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    };
    let n = pages.len();
    // Objects: 1 catalog, 2 pages, then per page (page, content, image) = 3 + 3k ...
    let kids: Vec<String> = (0..n).map(|k| format!("{} 0 R", 3 + 3 * k)).collect();
    obj(&mut out, &mut offs, b"<< /Type /Catalog /Pages 2 0 R >>");
    obj(&mut out, &mut offs, format!("<< /Type /Pages /Kids [{}] /Count {n} >>", kids.join(" ")).as_bytes());
    for (k, img) in pages.iter().enumerate() {
        let landscape = img.width > img.height;
        let (pw, ph) = if landscape { (792.0, 612.0) } else { (612.0, 792.0) };
        let margin = 36.0;
        let s = ((pw - 2.0 * margin) / img.width as f32).min((ph - 2.0 * margin) / img.height as f32);
        let (dw, dh) = (img.width as f32 * s, img.height as f32 * s);
        let (dx, dy) = ((pw - dw) / 2.0, (ph - dh) / 2.0);
        let mut rgb = Vec::with_capacity(img.pixels.len() * 3);
        for &p in &img.pixels {
            rgb.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
        }
        let data = miniz_oxide::deflate::compress_to_vec_zlib(&rgb, 6);
        let content = format!("q {dw:.2} 0 0 {dh:.2} {dx:.2} {dy:.2} cm /Im0 Do Q");
        let (pg, ct, im) = (3 + 3 * k, 4 + 3 * k, 5 + 3 * k);
        obj(&mut out, &mut offs, format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw} {ph}] /Resources << /XObject << /Im0 {im} 0 R >> >> /Contents {ct} 0 R >>").as_bytes());
        let _ = pg;
        obj(&mut out, &mut offs, &[format!("<< /Length {} >>\nstream\n", content.len()).as_bytes(), content.as_bytes(), b"\nendstream"].concat());
        obj(
            &mut out,
            &mut offs,
            &[format!("<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n", img.width, img.height, data.len()).as_bytes(), &data, b"\nendstream"].concat(),
        );
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offs.len() + 1).as_bytes());
    for o in &offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", offs.len() + 1).as_bytes());
    std::fs::File::create(path)?.write_all(&out)
}


/// Sheets for a print mode: `n` = 1 full page, 2 or 3 = an n×n poster (sheet k shows
/// source tile (col (k-1)%n, row (k-1)/n) enlarged to the page — RPosterJob).
pub fn sheets(img: &Image, n: u8) -> Vec<Image> {
    let n = n.max(1) as u32;
    if n == 1 {
        return vec![img.clone()];
    }
    let (tw, th) = (img.width / n, img.height / n);
    let mut out = Vec::new();
    for row in 0..n {
        for col in 0..n {
            let tile = crate::gfx::crop(img, (col * tw) as i32, (row * th) as i32, tw as i32, th as i32);
            out.push(tile);
        }
    }
    out
}

/// Print with a mode (see `sheets`).
pub fn print_mode(img: &Image, n: u8) -> Result<PathBuf, String> {
    let pages = sheets(img, n);
    let path = std::env::temp_dir().join("KidPixRevival-print.pdf");
    write_pdf_pages(&pages, &path).map_err(|e| e.to_string())?;
    send(&path)?;
    Ok(path)
}

/// Print several sheets (one picture per page), e.g. a slide show's slides.
pub fn print_pages(pages: &[Image]) -> Result<PathBuf, String> {
    let path = std::env::temp_dir().join("KidPixRevival-print.pdf");
    write_pdf_pages(pages, &path).map_err(|e| e.to_string())?;
    send(&path)?;
    Ok(path)
}


fn send(path: &Path) -> Result<(), String> {
    let ok = if cfg!(target_os = "windows") {
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &format!("Start-Process -FilePath '{}' -Verb Print", path.display())])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    } else {
        std::process::Command::new("lp").arg(&path).status().map(|s| s.success()).unwrap_or(false)
    };
    if !ok {
        let opener = if cfg!(target_os = "macos") { "open" } else if cfg!(target_os = "windows") { "explorer" } else { "xdg-open" };
        std::process::Command::new(opener).arg(path).spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn pdf_is_written() {
        let img = crate::gfx::Image::filled(60, 40, 0xffff0000);
        let p = std::env::temp_dir().join("kpr-test.pdf");
        super::write_pdf_pages(std::slice::from_ref(&img), &p).unwrap();
        let b = std::fs::read(&p).unwrap();
        assert!(b.starts_with(b"%PDF-1.4") && b.ends_with(b"%%EOF\n"));
    }
}
