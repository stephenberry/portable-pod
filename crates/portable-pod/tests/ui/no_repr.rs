use portable_pod::Pod;
#[derive(Clone, Copy, Pod)]
struct Unspecified {
    a: u32,
    b: u32,
}
fn main() {}
