// The whole reason this crate exists. `usize` is four bytes on a 32-bit target and eight on a
// 64-bit one, so a struct containing one does not have a portable representation.
// `bytemuck::Pod` accepts this; `portable_pod::Pod` must not.
use portable_pod::Pod;
#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Span {
    offset: usize,
    len: usize,
}
fn main() {}
