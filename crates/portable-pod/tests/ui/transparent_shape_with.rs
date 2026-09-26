//! `transparent` makes the shape exactly the field's, which a `shape_with` would change.
use portable_pod::Pod;

#[derive(Clone, Copy, Pod)]
#[repr(transparent)]
#[pod(transparent, shape_with = 0x1234)]
struct KindCode(u8);

fn main() {}
