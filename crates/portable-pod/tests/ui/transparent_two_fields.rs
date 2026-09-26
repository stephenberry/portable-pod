//! `#[pod(transparent)]` makes the type's shape its one field's, so a second field is refused
//! rather than silently left out of the shape.
use portable_pod::Pod;

#[derive(Clone, Copy, Pod)]
#[repr(C)]
#[pod(transparent)]
struct Span {
    start: u32,
    len: u32,
}

fn main() {}
