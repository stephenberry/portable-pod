use portable_pod::Pod;
#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Raw {
    at: *const u32,
}
fn main() {}
