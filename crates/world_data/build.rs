use std::{collections::HashSet, env, fs, io, path::PathBuf};

use serde::Deserialize;

#[derive(Deserialize)]
struct ExportHeader {
    scene: SceneHeader,
}

#[derive(Deserialize)]
struct SceneHeader {
    scene_id: String,
    scene_kind: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let exports_dir = manifest_dir.join("../../assets/maps");
    println!("cargo:rerun-if-changed={}", exports_dir.display());

    let mut exports = fs::read_dir(&exports_dir)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    exports.retain(|path| {
        path.is_file()
            && path
                .file_name()
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

    let mut scene_ids = HashSet::new();
    let mut generated =
        String::from("pub(crate) const EMBEDDED_WORLD_EXPORTS: &[(&str, &str, &str, &str)] = &[\n");
    for unresolved_path in exports {
        let path = fs::canonicalize(unresolved_path)?;
        println!("cargo:rerun-if-changed={}", path.display());
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "non-UTF-8 export name"))?;
        let source = fs::read_to_string(&path)?;
        let header: ExportHeader = serde_json::from_str(&source).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("cannot read SceneMaker header from {file_name}: {error}"),
            )
        })?;
        if header.scene.scene_id.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("SceneMaker export {file_name} has an empty scene ID"),
            )
            .into());
        }
        if header.scene.scene_kind != "instance" && header.scene.scene_kind != "template" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "SceneMaker export {file_name} has unknown scene kind {:?}",
                    header.scene.scene_kind
                ),
            )
            .into());
        }
        if !scene_ids.insert(header.scene.scene_id.clone()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "SceneMaker scene ID {:?} is duplicated",
                    header.scene.scene_id
                ),
            )
            .into());
        }
        generated.push_str(&format!(
            "    ({file_name:?}, {:?}, {:?}, include_str!({:?})),\n",
            header.scene.scene_id,
            header.scene.scene_kind,
            path.display().to_string()
        ));
    }
    generated.push_str("];\n");

    let output = PathBuf::from(env::var("OUT_DIR")?).join("embedded_world_exports.rs");
    fs::write(output, generated)?;
    Ok(())
}
