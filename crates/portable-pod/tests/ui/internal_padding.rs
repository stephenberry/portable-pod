use portable_pod::Pod;
#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Padded {
    a: u8,
    b: u32,
}
fn main() {}
