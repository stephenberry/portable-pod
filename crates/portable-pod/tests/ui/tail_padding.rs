use portable_pod::Pod;
#[derive(Clone, Copy, Pod)]
#[repr(C, align(8))]
struct Aligned {
    a: u32,
}
fn main() {}
