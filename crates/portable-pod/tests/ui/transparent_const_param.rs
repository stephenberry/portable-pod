//! Under `transparent` the shape is the field's, and whether that depends on a const parameter
//! cannot be read off tokens, so every const parameter is refused. The three cases: one the field
//! does not mention, one it mentions through an alias that discards it, and one it plainly uses,
//! which is refused too until there is a sound test that tells it apart from the second.
use portable_pod::Pod;

type Ignore<const N: usize> = u32;

#[derive(Clone, Copy, Pod)]
#[repr(transparent)]
#[pod(transparent)]
struct Fixed<const FRAC: u8>(i64);

#[derive(Clone, Copy, Pod)]
#[repr(transparent)]
#[pod(transparent)]
struct Aliased<const N: usize>(Ignore<N>);

#[derive(Clone, Copy, Pod)]
#[repr(transparent)]
#[pod(transparent)]
struct Lanes<const N: usize>([u16; N]);

fn main() {}
