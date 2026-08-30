# portable-pod

Plain-old-data types whose bytes are **identical on every machine**.

```rust
use portable_pod::{Pod, bytes_of};

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Header {
    magic: u32,
    version: u32,
}

assert_eq!(bytes_of(&Header { magic: 1, version: 2 }).len(), 8);
```

## Should you use this, or `bytemuck`?

**If your bytes never leave the machine, use [`bytemuck`](https://docs.rs/bytemuck).** It is more capable, more widely used, and this crate is not trying to replace it.

Use this one if your bytes are written to a file, sent over a network, hashed into a checksum that another machine must reproduce, or compared against a run on a different target. There is one specific reason:

```rust,ignore
unsafe impl Pod for usize {}   // bytemuck does this. portable-pod does not.
```

A struct containing a `usize` is four bytes wide on a 32-bit target and eight on a 64-bit one. So:

- a checksum differs between a server and a client that are supposed to agree,
- a save file written on one target fails to load on another,
- a wire format silently changes size when the target does.

None of these are caught by a test suite that runs on one architecture. They surface as cross-platform divergence, which is the most expensive place for a bug to surface.

`portable-pod` adds one property on top of "can be viewed as bytes": **position-independence**. No pointers, no references, no `usize`/`isize`, no atomics, no interior mutability. A value's representation depends on the value and nothing else.

## The derive does the work

Don't hand-write `unsafe impl Pod`. The usual version of that is a human doing arithmetic in a comment:

```rust,ignore
// SAFETY: repr(C), Copy, no padding: pos(16) + vel(16) + flags(4) + _pad(4) = 40 ...
unsafe impl Pod for Body {}
```

The comment goes stale when someone adds a field. `#[derive(Pod)]` proves three of the four clauses mechanically, and the diagnostic points at the field at fault:

```text
error[E0277]: `usize` is not `Pod`, so its bytes are not portable across machines
 --> src/wire.rs:8:13
  |
8 |     offset: usize,
  |             ^^^^^ the trait `Pod` is not implemented for `usize`
  |
  = note: not position-independent: `usize`, `isize`, pointers, references, atomics,
          `Cell`/`RefCell` — use a fixed-width integer such as `u32` or `u64`
```

Padding is proved absent by one equation, `size_of::<Self>() == sum of field sizes`, which
under `repr(C)` rules out internal and tail padding alike:

```text
error[E0080]: evaluation panicked: `Padded` has padding, so it cannot be `Pod`:
`size_of::<Padded>()` exceeds the sum of its field sizes, and reading a padding byte observes
uninitialized memory. Under `repr(C)` each field goes at the next offset that is a multiple of
its alignment, and the size is rounded up to the struct's alignment, so a gap sits before any
field more aligned than the offset it would otherwise take, and after the last field. Fields in
declaration order: a: u8, b: u32. Reorder them widest-first, or insert explicit zeroed padding
fields. Always-zero padding is not an escape: a typed copy leaves padding uninitialized however
the value was built.
```

This works for generic types too, **per instantiation** — `Ring<3>` and `Ring<4>` are proved separately, and neither has to be named in a test:

```rust
use portable_pod::Pod;

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Ring<const N: usize> {
    slots: [u32; N],
    len: u32,
}
```

## Two things to know

**Big-endian targets are refused, not caveated.** `bytes_of` gives you the native representation, so on a big-endian target a `u32`'s bytes would disagree with every little-endian peer — the same across-machine disagreement this crate exists to prevent, and just as invisible to a test suite running on one target. So the crate does not compile there:

```text
error: portable-pod refuses to build for a big-endian target. `bytes_of` would yield
big-endian bytes that disagree with every little-endian peer, and no test running only on
this target could observe the disagreement. [...] enable the crate's `allow-big-endian` feature.
```

Every mainstream target is little-endian, and WebAssembly is little-endian by specification, so this costs nothing in practice; what it excludes is `s390x`, `powerpc64`, and the big-endian configurations of bi-endian architectures like `aarch64_be-unknown-linux-gnu`. Inside that boundary the promise is unconditional. If you want the layout guarantees alone and accept native-endian bytes, enable `allow-big-endian`.

**`cargo check` does not catch padded generic types, and the check has a scope.** A generic type's proof is an associated const evaluated per monomorphization, so the failure is a post-monomorphization error: concrete types are caught by `cargo check`, generic ones need `cargo build` or `cargo test`.

More important is *which* instantiations are checked: those that **reach this crate** — passed to `bytes_of`, `zeroed`, `read_pod`, or contained in a type that is. A `Ring<3>` your program only constructs and reads directly is checked by nothing. Call `assert_layout::<Ring<3>>()` in a test to check one deliberately.

```rust
use portable_pod::{assert_layout, Pod};

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Ring<const N: usize> { slots: [u32; N], len: u32 }

assert_layout::<Ring<3>>();
assert_layout::<Ring<7>>();
```

## `bool` is not `Pod`

Only `0` and `1` are valid `bool`s, so materializing any other byte as one is undefined behavior — which makes `bool` unsound to reconstruct from bytes you did not write. Use `Bit`, a one-byte flag defined for all 256 patterns:

```rust
use portable_pod::Bit;

let flag = Bit::new(true);
assert!(flag.get());
```

A hostile input that injects `0x7f` produces a *different value*, never undefined behavior. That is the whole distinction.

## Floats are excluded deliberately

`f32` and `f64` are any-bit-pattern in the strict sense, but NaN payloads and signaling behavior are not stable across targets and toolchains, which defeats the property this crate exists to provide. Use a fixed-point type built on integers, or use `bytemuck`.

## Features

| Feature | Default | What it does |
| --- | --- | --- |
| `derive` | yes | `#[derive(Pod)]` |
| `alloc` | yes | `boxed_zeroed`, `boxed_zeroed_with` |

The crate is `no_std` unconditionally, and it has **zero dependencies with no footnote attached** — not "zero at runtime", not "zero unless you use the derive". A plain `cargo add portable-pod`, default features and all, builds this crate and its own derive and nothing else. The derive is written against `proc_macro`, which ships with the compiler like `core` and `alloc`, so there is no `syn`, no `quote`, no `proc-macro2`.

CI asserts this in all three configurations, so it cannot quietly regress: `cargo tree -e normal` is one line for `-p portable-pod --no-default-features`, one line for `-p portable-pod-derive`, and two for `-p portable-pod --all-features`.

## License

MIT OR Apache-2.0, at your option.
