use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Cursor, Read, Write};
use std::{env, error::Error, fs, process::exit};

use ::image::io::Reader as ImageReader;
use ipp::prelude::*;
use printpdf::*;

// For a true 4×6-inch print:
const PAGE_WIDTH_MM: f32 = 101.6;   // 4 inches in mm
const PAGE_HEIGHT_MM: f32 = 152.4;  // 6 inches in mm
// Use the legacy media value that maps to a 4×6 output.
const DEFAULT_MEDIA: &str = "w288h432";
const PRINT_COLOR_MODE: &str = "color";
const PRINT_QUALITY: i32 = 4; // Normal quality

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} printer_uri filename [key=value ...]", args[0]);
        exit(1);
    }

    // Parse the printer URI.
    let uri: Uri = args[1].parse().map_err(|e| format!("Invalid printer URI: {}", e))?;

    // (Optional) Query and print the printer’s attributes.
    let printer_attrs = get_printer_attributes(&uri)?;
    println!("Printer attributes:");
    debug_print_printer_attributes(&printer_attrs);

    // Convert the input file (image or PDF) to PDF bytes.
    let pdf_data = if args[2].ends_with(".jpg")
        || args[2].ends_with(".jpeg")
        || args[2].ends_with(".png")
    {
        println!("Converting image to PDF...");
        convert_image_to_pdf(&args[2])?
    } else {
        println!("Using existing PDF file...");
        let mut file = fs::File::open(&args[2])?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        buffer
    };

    // Toggle this flag to switch between using raw bytes or saving to a temporary file.
    let use_file = false; // Set to true to save the PDF to "tmp.pdf" and print from file.
    let payload = create_payload_from_pdf(&pdf_data, use_file)?;

    // Build a media-col collection with dimensions in hundredths of a millimeter.
    let x_dimension = (PAGE_WIDTH_MM * 100.0).round() as i32;
    let y_dimension = (PAGE_HEIGHT_MM * 100.0).round() as i32;
    let mut media_size_map = BTreeMap::new();
    media_size_map.insert("x-dimension".to_string(), IppValue::Integer(x_dimension));
    media_size_map.insert("y-dimension".to_string(), IppValue::Integer(y_dimension));

    let mut media_col_map = BTreeMap::new();
    media_col_map.insert("media-size".to_string(), IppValue::Collection(media_size_map));
    let media_col = IppValue::Collection(media_col_map);

    // Build the print job request.
    let builder = IppOperationBuilder::print_job(uri.clone(), payload)
        .user_name(env::var("USER").unwrap_or_else(|_| "noname".to_owned()))
        .job_title(&args[2])
        .attribute(IppAttribute::new(
            "document-format",
            IppValue::MimeMediaType("application/pdf".into()),
        ))
        .attribute(IppAttribute::new("media", IppValue::Keyword(DEFAULT_MEDIA.into())))
        .attribute(IppAttribute::new("media-col", media_col))
        .attribute(IppAttribute::new(
            "print-scaling",
            IppValue::Keyword("auto".into()),
        ))
        .attribute(IppAttribute::new(
            "print-color-mode",
            IppValue::Keyword(PRINT_COLOR_MODE.into()),
        ))
        .attribute(IppAttribute::new("print-quality", IppValue::Enum(PRINT_QUALITY)));
    
    let operation = builder.build();
    let client = IppClient::new(uri);
    let response = client.send(operation)?;

    println!("IPP status code: {}", response.header().status_code());
    for group in response.attributes().groups() {
        println!("Group: {:?}", group.tag());
        for (name, attribute) in group.attributes() {
            println!("  {}: {:?}", name, attribute.value());
        }
    }

    Ok(())
}

/// Creates an IPP payload from the given PDF data. If `use_file` is true, the PDF data is
/// written to "tmp.pdf" and the file is used; otherwise, the PDF data is wrapped in a memory buffer.
fn create_payload_from_pdf(pdf_data: &[u8], use_file: bool) -> Result<IppPayload, Box<dyn Error>> {
    if use_file {
        let file_path = "tmp.pdf";
        let mut file = File::create(file_path)?;
        file.write_all(pdf_data)?;
        // Open the file for reading.
        let f = File::open(file_path)?;
        Ok(IppPayload::new(f))
    } else {
        Ok(IppPayload::new(Cursor::new(pdf_data.to_vec())))
    }
}

/// Retrieves the printer’s attributes using Get-Printer-Attributes.
fn get_printer_attributes(uri: &Uri) -> Result<IppAttributes, Box<dyn Error>> {
    let operation = IppOperationBuilder::get_printer_attributes(uri.clone()).build();
    let client = IppClient::new(uri.clone());
    let response = client.send(operation)?;
    if !response.header().status_code().is_success() {
        return Err(format!(
            "Failed to fetch printer attributes. Status: {:?}",
            response.header().status_code()
        )
        .into());
    }
    Ok(response.attributes().clone())
}

/// Prints printer attributes for debugging.
fn debug_print_printer_attributes(attrs: &IppAttributes) {
    for group in attrs.groups() {
        println!("Group tag: {:?}", group.tag());
        for (name, attribute) in group.attributes() {
            println!("  {}: {:?}", name, attribute.value());
        }
    }
}

/// Converts an image file to a PDF with a page size of 101.6 × 152.4 mm (4×6 inches).
fn convert_image_to_pdf(image_path: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    // Open and decode the image.
    let img = ImageReader::open(image_path)?.decode()?;
    // Create a PDF document with the 4×6-inch page size.
    let (doc, page1, layer1) = PdfDocument::new(
        "Image Print Job",
        Mm(PAGE_WIDTH_MM),
        Mm(PAGE_HEIGHT_MM),
        "Layer 1",
    );
    let current_layer = doc.get_page(page1).get_layer(layer1);

    // For simplicity, add the image without scaling adjustments.
    let rgb_image = img.to_rgb8();
    let image = ImageXObject {
        width: Px(img.width() as usize),
        height: Px(img.height() as usize),
        color_space: ColorSpace::Rgb,
        bits_per_component: ColorBits::Bit8,
        interpolate: true,
        image_data: rgb_image.into_raw(),
        image_filter: None,
        clipping_bbox: None,
        smask: None,
    };

    let image_layer = Image::from(image);
    image_layer.add_to_layer(
        current_layer,
        ImageTransform {
            translate_x: Some(Mm(0.0)),
            translate_y: Some(Mm(0.0)),
            rotate: None,
            scale_x: Some(1.0),
            scale_y: Some(1.0),
            dpi: Some(300.0),
        },
    );

    let mut buffer = Vec::new();
    doc.save(&mut BufWriter::new(Cursor::new(&mut buffer)))?;
    Ok(buffer)
}
