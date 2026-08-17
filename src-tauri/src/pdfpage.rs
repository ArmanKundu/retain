//! Turning a page of a PDF into a picture.
//!
//! Retain stores the *text* of your papers, not the files — which is right for
//! searching and wrong for reading. A VCAA question is a diagram, a graph, a
//! table of results and four options laid out in a particular way; the text of
//! it reads as soup. If you're going to answer a past question you need to see
//! the page.
//!
//! # Why PDFKit
//!
//! It is a system framework, so nothing ships alongside the binary. Bundling
//! pdfium would mean carrying about ten megabytes of library per platform for a
//! feature that only ever runs on this one.
//!
//! It also answers both halves of the problem in the same place. Finding which
//! page a question sits on is `findString` — the stored text has no page
//! markers in it, so nothing in the database can tell you. And a page draws
//! itself into a bitmap. Any other approach needs one library to locate and a
//! second to render, and they have to agree about page numbering.
//!
//! # What this cannot do
//!
//! It needs the original file, at the path it was imported from. Retain records
//! `origin_path` but has never copied the PDF — so a paper you imported and
//! then moved has text and no picture. That is reported rather than papered
//! over: a blank image where a question should be is worse than a line saying
//! the file has moved.

#![cfg(target_os = "macos")]

use anyhow::{anyhow, Result};
use std::path::Path;

/// Scale factor for the rendered page.
///
/// Two, because these are read on a Retina display and a 1× render of a page of
/// exam text is unreadably soft. Higher is not better: at 3× a page is over a
/// megabyte of PNG and the cache stops being cheap.
const SCALE: f64 = 2.0;

/// The page a phrase appears on, counting from zero.
///
/// Not `PDFDocument.findString`, which is an exact search and does not survive
/// the round trip. The needle comes out of `pdf-extract`; the page's text comes
/// out of PDFKit; the two are different implementations and disagree about
/// spacing constantly — `(epubcheck)` against `( epubcheck )` is enough to miss.
/// An exact search meant almost every question silently had no picture, which
/// looks like the feature being broken rather than absent.
///
/// So both sides are flattened to letters and digits with single spaces, and
/// matched as a substring. Punctuation and spacing stop mattering, which is
/// exactly what they should do here.
#[cfg(target_os = "macos")]
pub fn find_page(pdf: &Path, needle: &str) -> Result<Option<usize>> {
    use objc2::AnyThread;
    use objc2_foundation::{NSString, NSURL};
    use objc2_pdf_kit::PDFDocument;

    let wanted = flatten(&normalise_needle(needle));
    if wanted.len() < 20 {
        // Too short to be unique in a twenty-page paper — it would match a
        // running header and put every question on page one.
        return Ok(None);
    }

    unsafe {
        let path = NSString::from_str(&pdf.to_string_lossy());
        let url = NSURL::fileURLWithPath(&path);
        let Some(doc) = PDFDocument::initWithURL(PDFDocument::alloc(), &url) else {
            return Err(anyhow!("Couldn't open that PDF."));
        };

        for index in 0..doc.pageCount() {
            let Some(page) = doc.pageAtIndex(index) else {
                continue;
            };
            let Some(text) = page.string() else {
                continue;
            };
            if flatten(&text.to_string()).contains(&wanted) {
                return Ok(Some(index));
            }
        }
    }

    Ok(None)
}

/// Letters, digits and single spaces. Everything else goes.
///
/// This is what makes two different text extractors agree: ligatures, hyphens
/// at line ends, the spacing around brackets and the difference between a
/// hyphen and an en dash are all things they disagree about and none of them
/// change which page a question is on.
fn flatten(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = true;

    for c in text.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
            space = false;
        } else if !space {
            out.push(' ');
            space = true;
        }
    }
    out.trim_end().to_string()
}

/// Render one page as PNG bytes.
#[cfg(target_os = "macos")]
pub fn render_page(pdf: &Path, page_index: usize) -> Result<Vec<u8>> {
    use objc2::AnyThread;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep};
    use objc2_foundation::{NSDictionary, NSSize, NSString, NSURL};
    use objc2_pdf_kit::{PDFDisplayBox, PDFDocument};

    unsafe {
        let path = NSString::from_str(&pdf.to_string_lossy());
        let url = NSURL::fileURLWithPath(&path);
        let Some(doc) = PDFDocument::initWithURL(PDFDocument::alloc(), &url) else {
            return Err(anyhow!("Couldn't open that PDF."));
        };

        let Some(page) = doc.pageAtIndex(page_index) else {
            return Err(anyhow!("That page isn't in the document."));
        };

        // The media box, not the crop box: a cropped render can cut off a
        // figure that sits in the margin, and VCAA papers put diagrams there.
        let bounds = page.boundsForBox(PDFDisplayBox::MediaBox);
        let size = NSSize::new(bounds.size.width * SCALE, bounds.size.height * SCALE);
        if size.width < 1.0 || size.height < 1.0 {
            return Err(anyhow!("That page has no size."));
        }

        let image = page.thumbnailOfSize_forBox(size, PDFDisplayBox::MediaBox);

        let tiff = image
            .TIFFRepresentation()
            .ok_or_else(|| anyhow!("Couldn't read the rendered page."))?;
        let rep = NSBitmapImageRep::imageRepWithData(&tiff)
            .ok_or_else(|| anyhow!("Couldn't convert the rendered page."))?;

        let png = rep
            .representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
            .ok_or_else(|| anyhow!("Couldn't encode the page as PNG."))?;

        Ok(png.to_vec())
    }
}

/// Collapse whitespace and trim, so a needle taken from extracted text still
/// matches the PDF's own layout.
///
/// The extractor inserts line breaks where the page had columns, and PDFKit
/// searches the page's real text. Without this, almost nothing matches.
pub fn normalise_needle(text: &str) -> String {
    text.split_whitespace()
        .take(12)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The needle comes from text an extractor produced, and it has to survive
    /// the round trip into PDFKit's own idea of the page's words.
    #[test]
    fn a_needle_is_flattened_to_one_line_of_words() {
        assert_eq!(
            normalise_needle("  Which of the following\nbest describes\tthe role  "),
            "Which of the following best describes the role"
        );
    }

    /// Two extractors disagree about brackets, ligatures and hyphens on every
    /// page. Flattening is what makes them agree.
    #[test]
    fn flattening_removes_everything_the_two_extractors_argue_about() {
        assert_eq!(flatten("( epubcheck )"), flatten("(epubcheck)"));
        assert_eq!(flatten("Adobe  Systems\n©"), "adobe systems");
        assert_eq!(flatten("well-known"), "well known");
        assert_eq!(flatten("  "), "");
    }

    /// Long enough to be unique in a paper, short enough that a stray
    /// difference near the end doesn't stop it matching.
    #[test]
    fn a_needle_is_capped_at_a_dozen_words() {
        let long = (1..=30).map(|n| n.to_string()).collect::<Vec<_>>().join(" ");
        assert_eq!(normalise_needle(&long).split(' ').count(), 12);
    }

    #[test]
    fn an_empty_needle_stays_empty_rather_than_becoming_whitespace() {
        assert_eq!(normalise_needle("   \n\t "), "");
    }

    /// Renders a real PDF and checks the bytes are a real PNG.
    ///
    /// Ignored by default because it needs a file: pass one with
    /// `RETAIN_TEST_PDF`. Every other test here is about the needle, and the
    /// needle working proves nothing about whether PDFKit draws anything.
    #[test]
    #[ignore]
    fn a_real_pdf_renders_to_a_real_png() {
        let Ok(path) = std::env::var("RETAIN_TEST_PDF") else {
            return;
        };
        let png = render_page(Path::new(&path), 0).expect("first page should render");

        // The PNG signature. A renderer that quietly returns an empty buffer,
        // or TIFF, would otherwise pass a length check.
        assert!(png.len() > 512, "suspiciously small: {} bytes", png.len());
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "not a PNG");
        println!("rendered page 0 -> {} bytes", png.len());
    }

    /// A page past the end is an error, not a panic or a blank image.
    #[test]
    #[ignore]
    fn a_page_that_does_not_exist_is_refused() {
        let Ok(path) = std::env::var("RETAIN_TEST_PDF") else {
            return;
        };
        assert!(render_page(Path::new(&path), 9_999).is_err());
    }

    /// The round trip that the whole feature rests on.
    ///
    /// The needle comes out of `pdf-extract`, which is a different
    /// implementation from PDFKit's text layer. If the two disagree about
    /// spacing or ligatures, nothing ever matches and every question silently
    /// has no picture — which would look like the feature being broken rather
    /// than absent.
    #[test]
    #[ignore]
    fn text_extracted_by_one_library_is_findable_by_the_other() {
        let Ok(path) = std::env::var("RETAIN_TEST_PDF") else {
            return;
        };
        let bytes = std::fs::read(&path).unwrap();
        let extracted = pdf_extract::extract_text_from_mem(&bytes).unwrap_or_default();

        // A phrase from the middle, so it isn't the title or a running header.
        let words: Vec<&str> = extracted.split_whitespace().collect();
        if words.len() < 40 {
            println!("not enough text in this PDF to test with");
            return;
        }
        let needle = words[10..26].join(" ");

        let page = find_page(Path::new(&path), &needle).expect("search should not error");
        println!("needle {needle:?} -> page {page:?}");
        assert!(page.is_some(), "extracted text was not findable in the PDF");
    }

    #[test]
    fn a_missing_file_is_an_error_rather_than_a_crash() {
        let err = render_page(Path::new("/nonexistent/nope.pdf"), 0);
        assert!(err.is_err());
    }
}
