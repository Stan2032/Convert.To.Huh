//! Which formats can the linked build of image-rs actually read and write?
//! Capability claims come from probing the engine, never from its README
//! (docs/RESEARCH.md §3.5).
fn main() {
    use image::ImageFormat::*;
    for f in [Png, Jpeg, Gif, Bmp, Tiff, WebP, Ico, Tga, Qoi, Pnm, Farbfeld] {
        println!("{:>10?}  read={:<5} write={}", f, f.reading_enabled(), f.writing_enabled());
    }
}
