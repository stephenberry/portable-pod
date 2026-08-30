use portable_pod::Pod;
#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Glyph {
    ch: char,
}
fn main() {}
