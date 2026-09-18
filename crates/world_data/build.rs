use std::{collections::HashSet, env, fs, io, path::PathBuf};

use serde::Deserialize;

#[derive(Deserialize)]
struct ExportHeader {
    game_key: String,
    scene: SceneHeader,
}

#[derive(Deserialize)]
struct SceneHeader {
    scene_id: String,
    scene_kind: String,
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let exports_dir = manifest_dir.join("../../assets/maps");
    println!("cargo:rerun-if-changed={}", exports_dir.display());

    // One directory per game, named by the game key its exports carry. The
    // directory is the layout; the key inside each file is the truth, and the
    // two are checked against each other rather than one being trusted.
    let mut game_dirs = fs::read_dir(&exports_dir)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    game_dirs.retain(|path| path.is_dir());
    game_dirs.sort();
    if game_dirs.is_empty() {
        return Err(invalid(format!(
            "no game directories under {} - SceneMaker exports are synced into assets/maps/<game>/",
            exports_dir.display()
        ))
        .into());
    }

    let mut generated = String::from(
        "pub(crate) const EMBEDDED_WORLD_EXPORTS: &[(&str, &str, &str, &str, &str)] = &[\n",
    );
    let mut total = 0usize;
    for game_dir in game_dirs {
        println!("cargo:rerun-if-changed={}", game_dir.display());
        let game = game_dir
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid("non-UTF-8 game directory name".to_owned()))?
            .to_owned();

        let mut exports = fs::read_dir(&game_dir)?
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
            return Err(invalid(format!(
                "game directory {} holds no SceneMaker exports",
                game_dir.display()
            ))
            .into());
        }

        // Scene IDs are unique within a game and not across games: two games
        // may each author a "map01", which is the whole point of the split.
        let mut scene_ids = HashSet::new();
        for unresolved_path in exports {
            let path = fs::canonicalize(unresolved_path)?;
            println!("cargo:rerun-if-changed={}", path.display());
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| invalid("non-UTF-8 export name".to_owned()))?;
            let source = fs::read_to_string(&path)?;
            let header: ExportHeader = serde_json::from_str(&source).map_err(|error| {
                invalid(format!(
                    "cannot read SceneMaker header from {game}/{file_name}: {error}"
                ))
            })?;
            if header.game_key != game {
                return Err(invalid(format!(
                    "SceneMaker export {game}/{file_name} carries game key {:?}, \
                     which is not the game it is filed under",
                    header.game_key
                ))
                .into());
            }
            if header.scene.scene_id.is_empty() {
                return Err(invalid(format!(
                    "SceneMaker export {game}/{file_name} has an empty scene ID"
                ))
                .into());
            }
            if header.scene.scene_kind != "instance" && header.scene.scene_kind != "template" {
                return Err(invalid(format!(
                    "SceneMaker export {game}/{file_name} has unknown scene kind {:?}",
                    header.scene.scene_kind
                ))
                .into());
            }
            if !scene_ids.insert(header.scene.scene_id.clone()) {
                return Err(invalid(format!(
                    "game {game} duplicates SceneMaker scene ID {:?}",
                    header.scene.scene_id
                ))
                .into());
            }
            generated.push_str(&format!(
                "    ({game:?}, {file_name:?}, {:?}, {:?}, include_str!({:?})),\n",
                header.scene.scene_id,
                header.scene.scene_kind,
                path.display().to_string()
            ));
            total += 1;
        }
    }
    generated.push_str("];\n");

    if total == 0 {
        return Err(invalid(format!(
            "no SceneMaker exports found under {}",
            exports_dir.display()
        ))
        .into());
    }

    let output = PathBuf::from(env::var("OUT_DIR")?).join("embedded_world_exports.rs");
    fs::write(output, generated)?;
    Ok(())
}
