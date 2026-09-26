//! A transparent newtype over a type that is not `Pod`: one error, at the field, although the
//! newtype is used below.
use portable_pod::{Pod, bytes_of};

#[derive(Clone, Copy, Pod)]
#[repr(transparent)]
#[pod(transparent)]
struct Offset(usize);

fn main() {
    let _ = bytes_of(&Offset(1));
}
