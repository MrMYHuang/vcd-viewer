use std::path::Path;

#[test]
fn test_icon_assets_exist_and_valid() {
    assert!(Path::new("assets/icon-256.png").exists(), "icon-256.png missing");
    assert!(Path::new("assets/favicon.png").exists(), "favicon.png missing");
    assert!(Path::new("assets/icon.ico").exists(), "icon.ico missing");
    assert!(Path::new("assets/favicon.ico").exists(), "favicon.ico missing");

    let png_bytes = std::fs::read("assets/icon-256.png").expect("read icon-256.png");
    let img = image::load_from_memory(&png_bytes).expect("decode icon-256.png");
    assert_eq!(img.width(), 256);
    assert_eq!(img.height(), 256);
}
