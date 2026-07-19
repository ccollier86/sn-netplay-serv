use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpecManifest {
    schema_version: u16,
    protocol_version: u16,
    input_codec: String,
    input_payload_bytes: usize,
    predictor: String,
    files: Vec<ManifestFile>,
}

#[derive(Deserialize)]
struct ManifestFile {
    path: String,
    sha256: String,
}

#[test]
fn canonical_v5_manifest_covers_and_hashes_every_spec_file() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("spec/netplay-v5");
    let manifest = fs::read(root.join("manifest.json")).expect("read manifest");
    let manifest: SpecManifest = serde_json::from_slice(&manifest).expect("decode manifest");

    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.protocol_version, 5);
    assert_eq!(manifest.input_codec, super::V5_INPUT_CODEC_ID);
    assert_eq!(manifest.input_payload_bytes, 10);
    assert_eq!(manifest.predictor, super::V5_INPUT_PREDICTOR_ID);

    let listed = manifest
        .files
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        listed.len(),
        manifest.files.len(),
        "duplicate manifest path"
    );
    assert_eq!(listed, spec_paths(&root));

    for entry in manifest.files {
        assert!(!entry.path.starts_with('/'));
        assert!(!entry.path.contains(".."));
        let bytes = fs::read(root.join(&entry.path)).expect("read listed fixture");
        assert_eq!(format!("{:x}", Sha256::digest(bytes)), entry.sha256);
    }
}

fn spec_paths(root: &Path) -> BTreeSet<String> {
    let mut paths = BTreeSet::from(["README.md".to_string()]);
    for entry in fs::read_dir(root.join("fixtures")).expect("read fixtures") {
        let entry = entry.expect("fixture entry");
        assert!(entry.file_type().expect("fixture type").is_file());
        paths.insert(format!("fixtures/{}", entry.file_name().to_string_lossy()));
    }
    paths
}
