//! `crate` is a key with a value; naming it bare is rejected with the spelling to use.
use portable_pod::Pod;

#[derive(Clone, Copy, Pod)]
#[repr(C)]
#[pod(crate)]
struct Wire {
    id: u64,
}

fn main() {}
