//! A `<` in a pinned value would be read as generic arguments and swallow the next argument. It is
//! refused with the parenthesised spelling, rather than surfacing as a parse error in the
//! expansion.
use portable_pod::Pod;

#[derive(Clone, Copy, Pod)]
#[repr(C)]
#[pod(size = 1 << 4, align = 8)]
struct Wire {
    a: u64,
    b: u64,
}

fn main() {}
