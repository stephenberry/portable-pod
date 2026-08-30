use portable_pod::Pod;
#[derive(Clone, Copy, Pod)]
#[repr(C)]
union Overlap {
    a: u32,
    b: [u8; 4],
}
fn main() {}
