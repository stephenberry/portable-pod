//! A size pin that no longer matches. The layout is padding-free, so the padding proof passes; the
//! pin is what notices that a field was widened. The error must name both sizes, and point at the
//! pinned value rather than at the derive.
use portable_pod::Pod;

#[derive(Clone, Copy, Pod)]
#[repr(C)]
#[pod(size = 12)]
struct Record {
    id: u64,
    kind: u64,
}

fn main() {}
