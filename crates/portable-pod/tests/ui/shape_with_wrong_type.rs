//! `shape_with` takes a `u64` constant expression. A value of another type is reported inside the
//! attribute, at the value, not at the derive.
use portable_pod::Pod;

const TABLE: u32 = 7;

#[derive(Clone, Copy, Pod)]
#[repr(transparent)]
#[pod(shape_with = TABLE)]
struct KindCode(u8);

fn main() {}
