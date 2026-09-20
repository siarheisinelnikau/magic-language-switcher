use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=assets/app.manifest");
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let icon =
        PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory")).join("app.ico");
    fs::write(&icon, make_icon()).expect("write application icon");
    let manifest_version = format!(
        "{}.{}.{}.0",
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR"),
        env!("CARGO_PKG_VERSION_PATCH")
    );
    let manifest = include_str!("assets/app.manifest").replace("@VERSION@", &manifest_version);
    winresource::WindowsResource::new()
        .set_icon(icon.to_str().expect("icon path"))
        .set_manifest(&manifest)
        .set("ProductName", "Magic Language Switcher")
        .set("FileDescription", "Magic Language Switcher")
        .set(
            "LegalCopyright",
            "Copyright (c) 2026 siarheisinelnikau. MIT License.",
        )
        .set_language(0x0409)
        .compile()
        .expect("compile Windows resources (install the Windows SDK if rc.exe is missing)");
}

// A code-native icon: two white arrows on a blue rounded square, embedded in the EXE.
fn make_icon() -> Vec<u8> {
    let sizes = [16u32, 20, 24, 32, 48, 64, 128, 256];
    let images: Vec<_> = sizes.iter().map(|&size| make_icon_image(size)).collect();
    let mut result = vec![0, 0, 1, 0, sizes.len() as u8, 0];
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&size, image) in sizes.iter().zip(&images) {
        result.extend_from_slice(&[size as u8, size as u8, 0, 0, 1, 0, 32, 0]);
        result.extend_from_slice(&(image.len() as u32).to_le_bytes());
        result.extend_from_slice(&offset.to_le_bytes());
        offset += image.len() as u32;
    }
    for image in images {
        result.extend(image);
    }
    result
}

fn make_icon_image(size: u32) -> Vec<u8> {
    let mut pixels = Vec::new();
    for y in (0..size).rev() {
        for x in 0..size {
            let px = (x as f32 + 0.5) * 64.0 / size as f32;
            let py = (y as f32 + 0.5) * 64.0 / size as f32;
            let corner =
                ((px - px.clamp(12.0, 52.0)).powi(2) + (py - py.clamp(12.0, 52.0)).powi(2)).sqrt();
            let alpha = ((10.5 - corner).clamp(0.0, 1.0) * 255.0) as u8;
            let arrow = [
                (16., 22., 48., 22.),
                (40., 14., 48., 22.),
                (40., 30., 48., 22.),
                (48., 42., 16., 42.),
                (24., 34., 16., 42.),
                (24., 50., 16., 42.),
            ]
            .iter()
            .any(|&(ax, ay, bx, by)| segment_distance(px, py, ax, ay, bx, by) < 2.8);
            let pixel = if arrow {
                [255, 255, 255, alpha]
            } else {
                [216, 87, 39, alpha]
            };
            pixels.extend_from_slice(&pixel);
        }
    }
    let mask = vec![0u8; (size.div_ceil(32) * 4 * size) as usize];
    let mut result = Vec::new();
    for value in [40, size, size * 2] {
        result.extend_from_slice(&value.to_le_bytes());
    }
    result.extend_from_slice(&[1, 0, 32, 0]);
    for value in [0, pixels.len() as u32, 0, 0, 0, 0] {
        result.extend_from_slice(&value.to_le_bytes());
    }
    result.extend(pixels);
    result.extend(mask);
    result
}

fn segment_distance(x: f32, y: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let t = (((x - ax) * (bx - ax) + (y - ay) * (by - ay))
        / ((bx - ax).powi(2) + (by - ay).powi(2)))
    .clamp(0., 1.);
    ((x - ax - t * (bx - ax)).powi(2) + (y - ay - t * (by - ay)).powi(2)).sqrt()
}
