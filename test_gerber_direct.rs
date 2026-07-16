use std::path::Path;

fn main() {
    let test_zip = Path::new("test_files/inputs/gerber.zip");
    let temp_dir = tempfile::tempdir().unwrap();
    let png_path = temp_dir.path().join("board.png");
    
    let settings = easymill::ConversionSettings::default();
    
    match easymill::gerber_inputs_to_png(&[test_zip.to_path_buf()], &png_path, settings) {
        Ok(result) => {
            println!("✓ Render succeeded!");
            println!("  Dimensions: {} × {}", result.width, result.height);
            println!("  Dark pixels: {}", result.dark_pixels);
            println!("  Output: {}", result.path.display());
        }
        Err(e) => {
            println!("✗ Render failed: {}", e);
        }
    }
}
