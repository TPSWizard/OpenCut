//! Loading a LUT file from disk into the LUT effect's parameter: `.cube`
//! and `.3dl` as text, HALD CLUT images through the `image` crate.

use std::path::Path;

/// The `lut` parameter for the file, named after its stem.
pub(crate) fn load_lut_file(path: &Path) -> Result<String, String> {
    let name = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_else(|| "LUT".to_owned());
    let extension = path.extension().and_then(|ext| ext.to_str()).map(|ext| ext.to_ascii_lowercase()).unwrap_or_default();
    let table = match extension.as_str() {
        "cube" => editor::parse_cube(&std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?)?,
        "3dl" => editor::parse_3dl(&std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?)?,
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "tif" | "tiff" => {
            let image = image::open(path).map_err(|error| format!("{}: {error}", path.display()))?.to_rgba8();
            editor::from_hald(image.as_raw(), image.width(), image.height())?
        }
        _ => return Err("A LUT is a .cube or .3dl file or a HALD CLUT image".to_owned()),
    };
    Ok(editor::encode_lut_param(&name, &table))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_a_cube_file_and_rejects_other_files() {
        let dir = tempfile::tempdir().unwrap();
        let cube = dir.path().join("swap.cube");
        std::fs::write(&cube, "LUT_3D_SIZE 2\n0 0 0\n0 0 1\n0 1 0\n0 1 1\n1 0 0\n1 0 1\n1 1 0\n1 1 1\n").unwrap();
        let param = load_lut_file(&cube).unwrap();
        assert!(param.starts_with("swap|2|"), "{param}");
        let (name, table) = editor::decode_lut_param(&param).unwrap();
        assert_eq!(name, "swap");
        assert_eq!(table.sample(1.0, 0.0, 0.0), [0.0, 0.0, 1.0]);
        let other = dir.path().join("notes.txt");
        std::fs::write(&other, "x").unwrap();
        assert!(load_lut_file(&other).is_err());
    }
}
