use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Cursor, Read};
use std::{env, error::Error, fs, process::exit};
use ::image::ImageReader;
use ipp::prelude::*;
use printpdf::*;

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

    let operation = IppOperationBuilder::get_printer_attributes(uri.clone()).build();

    // Create an IPP client and send the request
    println!("{}", uri.clone());
    let client = IppClient::new(uri.clone());
    let response = client.send(operation)?;

    // Check the response status
    if !response.header().status_code().is_success() {
        println!("Failed to fetch printer attributes. Status: {:?}", response.header().status_code());
        return Ok(());
    }

    // Print the printer capabilities
    println!("Printer Capabilities and Attributes:");
    for group in response.attributes().groups() {
        println!("Group: {:?}", group.tag());
        for (name, attribute) in group.attributes() {
            println!("  {}: {:?}", name, attribute.value());
        }
    }

    // Wrap the PDF data as an IPP payload
    let payload = IppPayload::new(Cursor::new(pdf_data));


    let mut media_col_map: BTreeMap<String, IppValue> = BTreeMap::new();
    media_col_map.insert("media-type".to_string(), IppValue::Enum(189));

    let media_col = IppValue::Collection(media_col_map);

    // let media_col = IppValue::Array(vec![
    //     IppValue::Keyword("189".into()), // SureLab Photo Paper Luster (250)
    // ]);


    // Build the print job request
    let builder = IppOperationBuilder::print_job(uri.clone(), payload)
        .user_name(env::var("USER").unwrap_or_else(|_| "noname".to_owned()))
        .job_title(&args[2])
        .attribute(IppAttribute::new("media", IppValue::Keyword("oe_epkg-nmgn_4x6in".into()))) // Paper type and size
        .attribute(IppAttribute::new("media-col", media_col))
        .attribute(IppAttribute::new("print-color-mode", IppValue::Keyword("color".into())))       // Color mode
        .attribute(IppAttribute::new("print-quality", IppValue::Enum(4)));

    

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
    let doc_width = Mm(102.0);
    let doc_height = Mm(152.0);

    let (doc, page1, layer1) = PdfDocument::new("Image Print Job", doc_width, doc_height, "Layer 1");
    let current_layer = doc.get_page(page1).get_layer(layer1);

    // Load the image using the `image_crate` Reader
    let img = ImageReader::open(image_path)?.decode()?;
    let (img_width, img_height) = (img.width() as f32, img.height() as f32);

    // Calculate the scale to fit the image within the document while maintaining aspect ratio
    let scale_x = doc_width.0 / img_width;
    let scale_y = doc_height.0 / img_height;
    let scale = scale_x.min(scale_y); // Use the smaller scale to fit within bounds

    // Calculate the scaled width and height
    let scaled_width = img_width * scale;
    let scaled_height = img_height * scale;

    // Center the image within the document
    let translate_x = (doc_width.0 - scaled_width) / 2.0;
    let translate_y = (doc_height.0 - scaled_height) / 2.0;

    // Convert the image into an RGB format suitable for `printpdf`
    let rgb_image = img.to_rgb8();

    // Create an ImageXObject for embedding
    let image = ImageXObject {
        width: Px(img_width as usize),
        height: Px(img_height as usize),
        color_space: ColorSpace::Rgb,
        bits_per_component: ColorBits::Bit8,
        interpolate: true,
        image_data: rgb_image.into_raw(),
        image_filter: None,
        clipping_bbox: None,
        smask: None,
    };

    // Add the image to the PDF layer with resizing and centering
    let image_layer = Image::from(image);
    image_layer.add_to_layer(
        current_layer.clone(),
        ImageTransform {
            translate_x: Some(Mm(translate_x)),
            translate_y: Some(Mm(translate_y)),
            rotate: None,
            scale_x: Some(scale as f32),
            scale_y: Some(scale as f32),
            dpi: Some(300.0),
        },
    );

    // Save the PDF to an in-memory buffer
    let mut buffer = Vec::new();
    doc.save(&mut BufWriter::new(Cursor::new(&mut buffer)))?;

    Ok(buffer)
}