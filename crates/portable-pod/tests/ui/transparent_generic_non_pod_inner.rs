//! A generic transparent newtype keeps the field bound every generic type gets, so an
//! instantiation over a type that is not `Pod` has no impl, reported where it is used.
use portable_pod::{Pod, bytes_of};

#[derive(Clone, Copy, Pod)]
#[repr(transparent)]
#[pod(transparent)]
struct Wrapper<T>(T);

fn main() {
    let _ = bytes_of(&Wrapper(1u32));
    let _ = bytes_of(&Wrapper(true));
}
