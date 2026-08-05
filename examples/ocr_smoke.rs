use std::error::Error;

use ddddocr::DdddOcr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut ocr = DdddOcr::new("ddddocr.onnx")?;
    let image = std::fs::read("test.png")?;
    let text = ocr.classification(&image).await?;
    println!("OCR result: {text}");
    Ok(())
}
