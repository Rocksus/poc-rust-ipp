use std::io::BufWriter;
use std::{env, error::Error, fs, process::exit};
use ::image::ImageReader;
use ipp::prelude::*;
use printpdf::*;
use std::io::Cursor;
use std::io::Read;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().collect();

    if args.len() < 3 {
        println!("Usage: {} uri filename [key=value ...]", args[0]);
        exit(1);
    }

    // Parse the printer URI
    let uri: Uri = args[1].parse()?;

    // Convert the input file to a PDF
    let pdf_data = if args[2].ends_with(".jpg") || args[2].ends_with(".jpeg") || args[2].ends_with(".png") {
        println!("Converting image to PDF...");
        convert_image_to_pdf(&args[2])?
    } else {
        // If the file is already a PDF, load it as-is
        println!("Using existing PDF file...");
        let mut file = fs::File::open(&args[2])?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        buffer
    };

    // Wrap the PDF data as an IPP payload
    let payload = IppPayload::new(Cursor::new(pdf_data));

    // Build the print job request
    let mut builder = IppOperationBuilder::print_job(uri.clone(), payload)
        .user_name(env::var("USER").unwrap_or_else(|_| "noname".to_owned()))
        .job_title(&args[2]);

    for arg in &args[3..] {
        if let Some((k, v)) = arg.split_once('=') {
            builder = builder.attribute(IppAttribute::new(k, v.parse()?));
        }
    }

    let operation = builder.build();
    let client = IppClient::new(uri);
    let response = client.send(operation)?;

    // Print the response
    println!("IPP status code: {}", response.header().status_code());

    let attrs = response
        .attributes()
        .groups_of(DelimiterTag::JobAttributes)
        .flat_map(|g| g.attributes().values());

    for attr in attrs {
        println!("{}: {}", attr.name(), attr.value());
    }

    Ok(())
}

/// Converts an image file to a PDF and returns the PDF data as a Vec<u8>.
fn convert_image_to_pdf(image_path: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    // Define the PDF page size (4R photo dimensions: 102 mm x 152 mm)
    let (doc, page1, layer1) = PdfDocument::new("Image Print Job", Mm(102.0), Mm(152.0), "Layer 1");
    let current_layer    = doc.get_page(page1).get_layer(layer1);

    // Load the image using the `image_crate` Reader
    let img = ImageReader::open(image_path)?.decode()?;

    // Convert the image into an RGB format suitable for `printpdf`
    let rgb_image = img.to_rgb8();

    // Create an ImageXObject for embedding
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

    // Add the image to the PDF layer
    let image_layer = Image::from(image);
    image_layer.add_to_layer(current_layer.clone(), ImageTransform::default());

    // Save the PDF to an in-memory buffer
    let mut buffer = Vec::new();
    doc.save(&mut BufWriter::new(Cursor::new(&mut buffer)))?;

    Ok(buffer)
}