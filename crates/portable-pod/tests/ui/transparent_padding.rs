//! `transparent` forwards the field's shape, not an exemption from the layout proof. A newtype
//! that over-aligns its field has tail padding, and one over a padded instantiation inherits that
//! instantiation's failed proof.
use portable_pod::Pod;

#[derive(Clone, Copy, Pod)]
#[repr(C, align(8))]
#[pod(transparent)]
struct Widened(u32);

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Inner<const N: usize> {
    a: u8,
    b: [u32; N],
}

#[derive(Clone, Copy, Pod)]
#[repr(transparent)]
#[pod(transparent)]
struct Holder(Inner<1>);

fn main() {}
