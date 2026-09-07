use std::{collections::HashSet, env, fs, io, path::PathBuf};

use serde::Deserialize;

#[derive(Deserialize)]
struct Catalog {
    assets: Vec<CatalogAsset>,
}

#[derive(Deserialize)]
struct CatalogAsset {
    asset_key: String,
    asset_type: String,
}

/// Where the PolyTools sync puts the package of an Asset type.
///
/// This mirrors `destination_for_type` in `scripts/sync_polytools_characters.sh`
/// and is the one piece of policy here: everything else follows the catalog.
fn destination_directory(asset_type: &str) -> Option<&'static str> {
    match asset_type {
        "character" => Some("characters"),
        "props" => Some("props"),
        "weapons" => Some("weapons"),
        "terrain" => Some("terrain"),
        "items" => Some("items"),
        "icon" => Some("icons"),
        "symbols" => Some("symbols"),
        _ => None,
    }
}

/// Embeds every Asset the catalog names, so that adding one upstream is a
/// synchronisation and not a code change.
///
/// `include_str!` needs a literal path at compile time, so this table cannot be
/// built by ordinary code. It carries no policy about which Assets are used -
/// it mirrors the catalog, and the loader decides what it needs.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let assets_dir = manifest_dir.join("../../assets");
    let catalog_path = fs::canonicalize(assets_dir.join("catalog.json"))?;
    println!("cargo:rerun-if-changed={}", catalog_path.display());

    let catalog: Catalog = serde_json::from_str(&fs::read_to_string(&catalog_path)?)?;
    if catalog.assets.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} names no Assets", catalog_path.display()),
        )
        .into());
    }

    let mut seen = HashSet::new();
    let mut assets = catalog.assets;
    assets.sort_by(|first, second| {
        (&first.asset_type, &first.asset_key).cmp(&(&second.asset_type, &second.asset_key))
    });

    let mut generated = String::from(
        "/// Every Asset package the world catalog names, embedded at build time.\n\
         pub(crate) const EMBEDDED_ASSET_MANIFESTS: &[(&str, &str, &str)] = &[\n",
    );
    for asset in assets {
        let Some(directory) = destination_directory(&asset.asset_type) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "catalogued Asset {} has unknown type {}",
                    asset.asset_key, asset.asset_type
                ),
            )
            .into());
        };
        if !seen.insert((asset.asset_type.clone(), asset.asset_key.clone())) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Asset {} is catalogued twice", asset.asset_key),
            )
            .into());
        }
        let package = assets_dir
            .join(directory)
            .join(&asset.asset_key)
            .join("manifest.json");
        let package = fs::canonicalize(&package).map_err(|error| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "catalogued Asset {} has no package at {}: {error}",
                    asset.asset_key,
                    package.display()
                ),
            )
        })?;
        println!("cargo:rerun-if-changed={}", package.display());
        generated.push_str(&format!(
            "    ({:?}, {:?}, include_str!({:?})),\n",
            asset.asset_type,
            asset.asset_key,
            package.display().to_string()
        ));
    }
    generated.push_str("];\n");

    let output = PathBuf::from(env::var("OUT_DIR")?).join("embedded_asset_manifests.rs");
    fs::write(output, generated)?;
    Ok(())
}
