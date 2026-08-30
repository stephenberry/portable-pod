use portable_pod::Pod;
#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Delta {
    by: isize,
}
fn main() {}
