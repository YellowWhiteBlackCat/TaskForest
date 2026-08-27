//! Regenerate `packaging/macos/icon.icns` from the `icon_*.png` ladder.
//!
//! Pure-Rust (no iconutil/png2icns), so it runs on Linux/macOS/Windows.
//! Decode each PNG with the `png` crate, wrap it in an `icns::Image`, and let
//! `IconFamily::add_icon` pick the correct OSType from the pixel dimensions.
//! `from_pixel_size` accepts 16/32/64/128/256/512/1024 (64 -> RGBA32_64x64,
//! 1024 -> RGBA32_512x512_2x).

use icns::{Image, IconFamily, PixelFormat};
use png::ColorType;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

const ROOT_REL: &str = "packaging/macos";
const LADDER: [u32; 7] = [16, 32, 64, 128, 256, 512, 1024];

fn macos_dir() -> PathBuf {
    // manifest is at packaging/macos/build-icns/Cargo.toml -> ../ is packaging/macos
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().expect("CARGO_MANIFEST_DIR parent").to_path_buf()
}

fn decode_rgba(path: &PathBuf) -> (u32, u32, Vec<u8>) {
    let decoder = png::Decoder::new(BufReader::new(
        File::open(path).unwrap_or_else(|e| panic!("open {}: {}", path.display(), e)),
    ));
    let mut reader = decoder
        .read_info()
        .unwrap_or_else(|e| panic!("png read_info {}: {}", path.display(), e));
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .unwrap_or_else(|e| panic!("png decode {}: {}", path.display(), e));
    assert_eq!(
        info.color_type,
        ColorType::Rgba,
        "{} is not RGBA (got {:?})",
        path.display(),
        info.color_type
    );
    buf.truncate(info.buffer_size());
    (info.width, info.height, buf)
}

fn main() {
    let dir = macos_dir();
    let mut family = IconFamily::new();
    for &size in &LADDER {
        let png_path = dir.join(format!("icon_{size}.png"));
        let (w, h, data) = decode_rgba(&png_path);
        assert_eq!((w, h), (size, size), "dimension mismatch {}", png_path.display());
        let img = Image::from_data(PixelFormat::RGBA, w, h, data)
            .unwrap_or_else(|e| panic!("from_data {size}: {e}"));
        family
            .add_icon(&img)
            .unwrap_or_else(|e| panic!("add_icon {size}: {e}"));
        eprintln!("added {size}x{size} <- {}", png_path.display());
    }

    let out = dir.join("icon.icns");
    let mut file = File::create(&out).unwrap_or_else(|e| panic!("create {}: {}", out.display(), e));
    family
        .write(&mut file)
        .unwrap_or_else(|e| panic!("write {}: {}", out.display(), e));
    let len = std::fs::metadata(&out).unwrap().len();
    eprintln!("wrote {} ({} bytes)", out.display(), len);

    // Round-trip check: read back and report the embedded icon set.
    let mut rb = BufReader::new(File::open(&out).unwrap());
    let reread = IconFamily::read(&mut rb).expect("icns readback");
    let types: Vec<String> = reread
        .available_icons()
        .into_iter()
        .map(|t| format!("{}x{}", t.pixel_width(), t.pixel_height()))
        .collect();
    eprintln!("readback available_icons: {}", types.join(", "));
    let _ = ROOT_REL; // kept for documentation of the asset layout
}
