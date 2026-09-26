//! The field's type does not mention `FRAC`, so under `transparent` every `Fixed<FRAC>` would
//! share `i64`'s shape, where the default derive folds `FRAC` in and tells them apart.
use portable_pod::Pod;

#[derive(Clone, Copy, Pod)]
#[repr(transparent)]
#[pod(transparent)]
struct Fixed<const FRAC: u8>(i64);

fn main() {}
