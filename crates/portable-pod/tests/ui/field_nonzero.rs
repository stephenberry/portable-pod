use portable_pod::Pod;
#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Handle {
    id: core::num::NonZeroU32,
}
fn main() {}
