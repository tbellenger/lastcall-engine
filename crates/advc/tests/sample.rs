use std::path::PathBuf;

#[test]
fn gatehouse_validates_and_matches_snapshots() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sample");
    advc::validate_dir(&root).unwrap();
    advc::test_dir(&root, false).unwrap();
}
