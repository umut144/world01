use std::{env, fs, io, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let exports_dir = manifest_dir.join("../../assets/maps");
    println!("cargo:rerun-if-changed={}", exports_dir.display());

    let mut exports = fs::read_dir(&exports_dir)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    exports.retain(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".scene_export.json"))
    });
    exports.sort();
    if exports.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no SceneMaker exports found in {}", exports_dir.display()),
        )
        .into());
    }

    let mut generated =
        String::from("pub(crate) const EMBEDDED_WORLD_EXPORTS: &[(&str, &str)] = &[\n");
    for path in exports {
        println!("cargo:rerun-if-changed={}", path.display());
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "non-UTF-8 export name"))?;
        generated.push_str(&format!(
            "    ({file_name:?}, include_str!({:?})),\n",
            path.display().to_string()
        ));
    }
    generated.push_str("];\n");

    let output = PathBuf::from(env::var("OUT_DIR")?).join("embedded_world_exports.rs");
    fs::write(output, generated)?;
    Ok(())
}
