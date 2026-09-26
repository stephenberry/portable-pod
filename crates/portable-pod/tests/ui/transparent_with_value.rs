//! `transparent` is a flag.
use portable_pod::Pod;

#[derive(Clone, Copy, Pod)]
#[repr(transparent)]
#[pod(transparent = true)]
struct Tick(u64);

fn main() {}
