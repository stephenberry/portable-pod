//! An alignment pin that no longer matches: `align(16)` was dropped, and the layout is still
//! padding-free, so only the pin notices.
use portable_pod::Pod;

#[derive(Clone, Copy, Pod)]
#[repr(C)]
#[pod(size = 16, align = 16)]
struct Block {
    lanes: [u32; 4],
}

fn main() {}
