use portable_pod::Pod;
#[derive(Clone, Copy, Pod)]
#[repr(C, packed)]
struct Packed {
    a: u8,
    b: u32,
}
fn main() {}
