use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Cursor, Read, Write};
use std::{env, error::Error, fs, process::exit};
use ::image::ImageReader;
use ipp::operation::IppOperation;
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

    // save pdf
    // let mut output_file = File::create("res.pdf")?;

    // output_file.write_all(&pdf_data);
    

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
        .job_title(&args[2]);
        // .attribute(IppAttribute::new("media", IppValue::Keyword("custom_104.99x162.56mm_104.99x162.56mm".into()))) // Paper type and size
        // .attribute(IppAttribute::new("media-col", media_col))
        // .attribute(IppAttribute::new("print-color-mode", IppValue::Keyword("color".into())))       // Color mode
        // .attribute(IppAttribute::new("print-quality", IppValue::Enum(4)));

    

    // let operation = builder.build();

    // let req = operation.into_ipp_request();

    // println!("Operation: {:?}", req.to_bytes().to_ascii_lowercase());



    let client = IppClient::new(uri);
    let response = client.send(builder.build())?;

    // Print the response
    println!("IPP status code: {}", response.header().status_code());

    // let attrs = response
    //     .attributes()
    //     .groups_of(DelimiterTag::JobAttributes)
    //     .flat_map(|g| g.attributes().values());

    // for attr in attrs {
    //     println!("{}: {}", attr.name(), attr.value());
    // }

    Ok(())
}

/// Converts an image file to a PDF and returns the PDF data as a Vec<u8>.
fn convert_image_to_pdf(image_path: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    // Open and decode the image
    let img = ImageReader::open(image_path)?.decode()?;
    // Get the image dimensions in pixels as f64
    let (img_width_px, img_height_px) = (img.width() as f32, img.height() as f32);
    
    // Assume a default DPI (dots per inch)
    let dpi: f32 = 300.0;
    // Convert pixel dimensions to physical size in millimeters:
    // 1 inch = 25.4 mm
    let page_width_mm = (img_width_px / dpi) * 25.4;
    let page_height_mm = (img_height_px / dpi) * 25.4;
    
    // Create a PDF document with a page sized exactly to the image's physical dimensions
    let (doc, page1, layer1) = PdfDocument::new(
        "Image Print Job", 
        Mm(page_width_mm), 
        Mm(page_height_mm), 
        "Layer 1"
    );
    let current_layer = doc.get_page(page1).get_layer(layer1);
    
    // Convert the image to RGB8 (a format suitable for embedding in PDF)
    let rgb_image = img.to_rgb8();
    
    // Create an ImageXObject from the raw image data
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
    
    // Wrap the ImageXObject into an Image and add it to the PDF layer,
    // positioned at the origin (0,0) with a scale factor of 1 (i.e. full size)
    let image_layer = Image::from(image);
    image_layer.add_to_layer(
        current_layer.clone(),
        ImageTransform {
            translate_x: Some(Mm(0.0)),
            translate_y: Some(Mm(0.0)),
            rotate: None,
            scale_x: Some(1.0),
            scale_y: Some(1.0),
            dpi: Some(dpi),
        },
    );
    
    // Save the PDF document into an in-memory buffer and return it
    let mut buffer = Vec::new();
    doc.save(&mut BufWriter::new(Cursor::new(&mut buffer)))?;
    Ok(buffer)
}