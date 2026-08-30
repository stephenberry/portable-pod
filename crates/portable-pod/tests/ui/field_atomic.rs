use portable_pod::Pod;
#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Counter {
    hits: core::sync::atomic::AtomicU32,
}
fn main() {}
