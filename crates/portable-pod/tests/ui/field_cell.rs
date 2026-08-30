use portable_pod::Pod;
#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Interior {
    slot: core::cell::Cell<u32>,
}
fn main() {}
