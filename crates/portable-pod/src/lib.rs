//! Plain-old-data types whose bytes are **identical on every machine**.
//!
//! [`Pod`] is deliberately narrower than [`bytemuck::Pod`]. It guarantees not just that a value
//! can be viewed as bytes, but that those bytes are the same on every target: same values, same
//! order, same length, regardless of pointer width.
//!
//! # Why not `bytemuck`
//!
//! `bytemuck` implements `Pod` for `usize`. That is correct for its purpose (GPU buffer casting,
//! in-process reinterpretation) and wrong the moment bytes leave the machine. A struct holding a
//! `usize` is four bytes on a 32-bit target and eight on a 64-bit one, so:
//!
//! * a checksum differs between a server and a client that should agree,
//! * a save file written on one target fails to load on another,
//! * a lockstep or rollback simulation desyncs between a native and a WebAssembly build,
//! * a wire format silently changes size when the target does.
//!
//! None of these are caught by a test suite that runs on one architecture. **If your bytes never
//! leave the machine, use `bytemuck`** — it is more capable and more widely used. This crate is
//! for the case where they do.
//!
//! # The contract
//!
//! See [`Pod`] for the normative statement. In short, a `Pod` type is `Copy`, valid for *every*
//! bit pattern, free of padding, and free of anything whose representation depends on the target
//! (pointers, references, `usize`/`isize`, atomics, interior mutability).
//!
//! # Deriving it
//!
//! Do not write `unsafe impl Pod` by hand if you can avoid it. The derive proves three of the
//! four clauses mechanically:
//!
//! ```
//! use portable_pod::{Pod, bytes_of};
//!
//! #[derive(Clone, Copy, Pod)]
//! #[repr(C)]
//! struct Header {
//!     magic: u32,
//!     version: u32,
//! }
//!
//! assert_eq!(bytes_of(&Header { magic: 1, version: 2 }).len(), 8);
//! ```
//!
//! A type that violates the contract fails to compile, naming the field at fault. This works for
//! generic types too, per instantiation:
//!
//! ```
//! # use portable_pod::Pod;
//! #[derive(Clone, Copy, Pod)]
//! #[repr(C)]
//! struct Ring<const N: usize> {
//!     slots: [u32; N],
//!     len: u32,
//! }
//! ```
//!
//! A generic type does not have to declare `Copy` on its own parameters. The derive discharges
//! the `Pod: Copy` obligation itself, so bounds can live on the impls:
//!
//! ```
//! # use portable_pod::Pod;
//! #[derive(Clone, Copy, Pod)]
//! #[repr(C)]
//! struct Table<K, V, const CAP: usize> {
//!     keys: [K; CAP],
//!     vals: [V; CAP],
//!     len: u32,
//!     _pad: u32,
//! }
//! ```
//!
//! # Re-exporting `Pod` from your own crate
//!
//! A library that gives its users one vocabulary will want to re-export the trait. The trait
//! re-exports like anything else, but the **derive** needs to be told where it went: its
//! expansion names `::portable_pod::Pod`, which does not resolve in a crate that depends on your
//! library rather than on this one. Name the path with `#[pod(crate = ...)]`:
//!
//! ```
//! use portable_pod::Pod;
//!
//! // Stand-in for the re-exporting library: `pub use portable_pod::Pod;` is all it needs.
//! mod my_engine {
//!     pub mod mem {
//!         pub use portable_pod::Pod;
//!     }
//! }
//!
//! #[derive(Clone, Copy, Pod)]
//! #[repr(C)]
//! #[pod(crate = crate::my_engine::mem)]
//! struct Wire {
//!     id: u64,
//!     kind: u32,
//!     flags: u32,
//! }
//!
//! fn main() {}
//! ```
//!
//! The value is a path, not a string. Only the `Pod` trait has to be reachable there; it is the
//! only item the expansion names.
//!
//! # Pinning the layout
//!
//! The derive proves a layout has no padding. It cannot know whether it is the layout you meant:
//! add a field, widen one, or drop an `align(16)`, and the type is still padding-free, while every
//! checksum, save file and wire message written against the old layout has silently changed
//! meaning. Where that matters, state the layout outright:
//!
//! ```
//! # use portable_pod::Pod;
//! #[derive(Clone, Copy, Pod)]
//! #[repr(C)]
//! #[pod(size = 16, align = 8)]
//! struct Record {
//!     id: u64,
//!     kind: u32,
//!     flags: u32,
//! }
//! ```
//!
//! A pin that holds costs nothing at run time. One that does not is a compile error at the pinned
//! value, naming what the type actually is:
//!
//! ```text
//! error[E0308]: mismatched types
//!  --> src/wire.rs:3:14
//!   |
//! 3 | #[pod(size = 12)]
//!   |              ^^ expected an array with a size of 12, found one with a size of 16
//! ```
//!
//! Each value is a `usize` constant expression, so it can name constants. On a generic type it can
//! name the type's own parameters, and the pin becomes an identity checked per instantiation, like
//! the padding proof ([When the check fires](#when-the-check-fires)):
//!
//! ```
//! # use portable_pod::Pod;
//! #[derive(Clone, Copy, Pod)]
//! #[repr(C)]
//! #[pod(size = 4 * N + 4)]
//! struct Ring<const N: usize> {
//!     slots: [u32; N],
//!     len: u32,
//! }
//! ```
//!
//! A `<` inside a value would be read as generic arguments, so parenthesise a shift:
//! `size = (1 << 4)`.
//!
//! **Alignment is the one layout property that can differ between targets.** A padding-free type
//! has the same size everywhere, but a `u64` is 8-aligned on 64-bit targets and 4-aligned on
//! 32-bit x86, so `align = 8` on a struct of `u64`s holds on one and fails on the other. That
//! failure is the pin doing its job. If the alignment has to hold everywhere, fix it with
//! `#[repr(C, align(8))]`, which the pin then confirms.
//!
//! # Shapes: refusing bytes written against another layout
//!
//! A pin catches a change of size. It cannot catch two `u32` fields swapped, or a `u32` retyped as
//! an `i32`: the size is the same and the old bytes read back as the wrong values. For that, every
//! `Pod` type has [`Pod::SHAPE`], a 64-bit hash of its field names and types that a file, replay or
//! wire header can carry and a reader can compare:
//!
//! ```
//! use portable_pod::Pod;
//!
//! #[derive(Clone, Copy, Pod)]
//! #[repr(C)]
//! struct Record {
//!     id: u64,
//!     kind: u32,
//!     flags: u32,
//! }
//!
//! const RECORD_SHAPE: Option<u64> = Record::SHAPE; // written into the header, checked on load
//! # assert!(RECORD_SHAPE.is_some());
//! ```
//!
//! The value is the same on every target, and it is a **persistence format**: the algorithm,
//! documented exactly in [`shape`], changes only in a semver-major release. A type's own name is
//! not part of it, so renaming a type does not invalidate stored data.
//!
//! # What "portable" does and does not mean
//!
//! The guarantee is about **layout**: size, field offsets, the absence of padding, and the
//! absence of anything whose width depends on the target. A `Pod` type occupies the same number
//! of bytes, with the same fields at the same offsets, on every target.
//!
//! **Endianness is a boundary of the guarantee rather than a hole in it.** [`bytes_of`] hands you
//! the native representation, so on a big-endian target a `u32`'s bytes would disagree with every
//! little-endian peer — which is the same across-machine disagreement this crate exists to
//! prevent, and just as invisible to a test suite running on one target. So rather than document
//! that as a caveat, **the crate refuses to compile for a big-endian target.**
//!
//! Every mainstream target is little-endian (x86, ARM, RISC-V, and WebAssembly, which the
//! specification fixes as little-endian), so the exclusion costs nothing in practice; what it
//! rules out is `s390x`, `powerpc64`, and the big-endian configurations of bi-endian
//! architectures. Inside the boundary the promise is unconditional: a `Pod` value's bytes are
//! identical on every target this crate supports.
//!
//! If you want the layout guarantees alone — padding-free `repr(C)`, no target-dependent widths —
//! and accept bytes that are native-endian and not comparable across machines, enable the
//! `allow-big-endian` feature.
//!
//! Note what is *not* symmetric here: byte order can be recovered by swapping at the boundary,
//! whereas a `usize` has no fixed *size* to swap. That is why the crate excludes `usize`
//! outright but excludes only the big-endian *targets* rather than `u32`.
//!
//! # When the check fires
//!
//! The derive proves layout with a `const` assertion, and *when* that assertion runs depends on
//! whether the type is generic. This matters when choosing what your CI runs.
//!
//! | Type | Proof | Caught by |
//! | --- | --- | --- |
//! | concrete (`struct Header { .. }`) | forced unconditionally at the definition | `cargo check` |
//! | generic (`struct Ring<const N: usize> { .. }`) | an associated const, per instantiation | `cargo build` / `cargo test`, and only for instantiations that reach this crate's API |
//!
//! A generic type has no single layout to check: `Ring<3>` and `Ring<4>` are different structs.
//! Each instantiation is therefore proved separately, when it is monomorphized, which makes the
//! failure a *post-monomorphization* error. Type-checking alone does not monomorphize, so
//! **`cargo check` and `cargo clippy` will not catch a padded generic instantiation.** Run
//! `cargo build` or `cargo test`.
//!
//! Note the precise scope: an instantiation is checked when it **reaches this crate** — when it
//! is passed to [`bytes_of`], [`zeroed`], [`read_pod`], or another entry point, or when it is a
//! field of a type that is. A generic type that is constructed, copied, and read without ever
//! touching this crate's API is never checked. Call [`assert_layout`] to check one deliberately:
//!
//! ```
//! # use portable_pod::{Pod, assert_layout};
//! # #[derive(Clone, Copy, Pod)]
//! # #[repr(C)]
//! # struct Ring<const N: usize> { slots: [u32; N], len: u32 }
//! // in a test, name the instantiations your program relies on
//! assert_layout::<Ring<3>>();
//! assert_layout::<Ring<7>>();
//! ```
//!
//! Within that scope you still get something a hand-written test cannot offer: every
//! instantiation that reaches the API is checked, including ones nobody thought to test.
//!
//! [`bytemuck::Pod`]: https://docs.rs/bytemuck/latest/bytemuck/trait.Pod.html

#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

// This crate refuses to build for a big-endian target, and that is a deliberate narrowing rather
// than an oversight.
//
// The argument for excluding `usize` is that a value which disagrees with itself across machines
// is worse than one that is merely unsound on a single machine, because no single-target test can
// see the disagreement. Byte order is that failure exactly. Left unguarded it is *silently* that
// failure: on `s390x` every layout proof still passes, `bytes_of` hands back big-endian bytes, and
// the little-endian peer reading the file gets garbage. The `portability` suite would not catch
// it either — its digest assertion used to be `#[cfg(target_endian = "little")]`, so the one test
// that would have noticed disabled itself on precisely the target where it was needed.
//
// `allow-big-endian` opts out, for a caller who wants the layout proofs alone and accepts
// native-endian bytes. Cargo features unify, so enabling it anywhere in a dependency graph enables
// it for every crate in that graph.
#[cfg(all(target_endian = "big", not(feature = "allow-big-endian")))]
compile_error!(
    "portable-pod refuses to build for a big-endian target. `bytes_of` would yield big-endian \
     bytes that disagree with every little-endian peer, and no test running only on this target \
     could observe the disagreement. This crate's promise is that a `Pod` value's bytes are \
     identical everywhere, and that promise does not hold here. If you want the layout \
     guarantees alone -- padding-free `repr(C)`, no target-dependent widths -- and accept \
     native-endian bytes that are not comparable across machines, enable the crate's \
     `allow-big-endian` feature."
);

#[cfg(feature = "alloc")]
extern crate alloc;

// So the derive's `::portable_pod::…` paths resolve inside this crate's own tests and doctests.
extern crate self as portable_pod;

// Compile the README's examples as doctests. Without this the README is prose that nobody
// checks, and its code drifts from the crate silently -- which is how it came to contain a
// rustdoc hidden-line marker (`# use ...`) that rendered literally on GitHub.
#[cfg(doctest)]
#[doc = include_str!("../../../README.md")]
struct ReadmeExamples;

mod bit;
#[cfg(feature = "alloc")]
mod boxed;
mod bytes;
pub mod shape;

pub use bit::Bit;
pub use bytes::{bytes_of, bytes_of_mut, bytes_of_slice, bytes_of_slice_mut, read_pod, zeroed};

#[cfg(feature = "alloc")]
pub use boxed::{boxed_zeroed, boxed_zeroed_with};

/// Derive [`Pod`], proving the contract at compile time.
///
/// Requires `#[repr(C)]`, `#[repr(transparent)]`, or `#[repr(C, align(N))]`, and that every
/// field is itself `Pod`. Rejects enums, unions, `#[repr(packed)]`, and any layout with padding.
///
/// There is no opt-out for padding; see the macro's own documentation for why.
///
/// It also fills in [`Pod::SHAPE`] from the fields' names and shapes and any const generic
/// parameters; see [`shape`] for exactly what that covers.
///
/// Accepts one attribute, `#[pod(...)]`, with four optional arguments:
///
/// * `crate = <path>` names the path that exports [`Pod`] when it is reached through a re-export
///   rather than as `::portable_pod`.
/// * `size = <expr>` and `align = <expr>` pin the type's size and alignment in bytes, so a layout
///   change fails the build. See [Pinning the layout](crate#pinning-the-layout).
/// * `shape_with = <expr>` folds one more `u64` constant into [`Pod::SHAPE`]. See
///   [Extending a derived shape](shape#extending-a-derived-shape).
#[cfg(feature = "derive")]
pub use portable_pod_derive::Pod;

/// A type whose in-memory **layout** is identical on every target: same size, same field
/// offsets, no padding, and no field whose width depends on the target.
///
/// Byte *order* within an integer is still the target's; see
/// [What "portable" does and does not mean](crate#what-portable-does-and-does-not-mean).
///
/// # Safety
///
/// Implement this only for a type satisfying **all four** clauses. The derive proves clauses 2,
/// 3, and 4 for you; write it by hand only for a primitive that bottoms out the induction.
///
/// 1. **`Copy + 'static`.** No destructor, no borrowed data. Enforced by the supertrait bound.
///
/// 2. **Any bit pattern is valid.** *Every* byte sequence of length `size_of::<Self>()` must
///    denote a valid value — not merely the all-zero one. This is what makes it sound to
///    reconstruct a value from untrusted input. It excludes `bool` (only `0` and `1` are valid;
///    use [`Bit`]), `char`, `NonZero*`, references, and enums, none of which are implemented
///    here.
///
/// 3. **No padding.** The type must contain no uninitialized gap, so reading its bytes is
///    deterministic and never observes uninitialized memory. Requires an explicit `repr`:
///    `#[repr(Rust)]` may reorder fields and its layout is not stable across compilations.
///
/// 4. **Position-independent.** No pointers, references, `usize`, `isize`, atomics, or interior
///    mutability. The byte representation must depend on the *value* and nothing else — not the
///    address, not the pointer width, not the allocation.
///
/// Clause 4 is what separates this trait from `bytemuck::Pod`, which covers `usize`. Violating
/// it does not produce undefined behavior on one machine; it produces a value that disagrees
/// with itself across machines, which is worse, because no single-target test can see it.
///
/// # Floats
///
/// `f32` and `f64` are deliberately **not** `Pod`. They are any-bit-pattern in the strict sense,
/// but NaN payload and signaling behavior are not stable across targets and toolchains, which
/// defeats the one property this crate exists to provide. A fixed-point type built on integers
/// is `Pod` and is the intended answer. If you want float POD, you want `bytemuck`.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not `Pod`, so its bytes are not portable across machines",
    note = "not valid for every bit pattern: `bool` (use `portable_pod::Bit`), `char`, `NonZero*`, enums, references",
    note = "not position-independent: `usize`, `isize`, pointers, references, atomics, `Cell`/`RefCell` \u{2014} use a fixed-width integer such as `u32` or `u64`",
    note = "deliberately excluded: `f32`/`f64`, because NaN payloads are not stable across targets",
    note = "for your own struct, add `#[derive(Pod)]` and `#[repr(C)]`"
)]
pub unsafe trait Pod: Copy + 'static {
    /// Compile-time layout proof, filled in by the derive. Not part of the public API.
    ///
    /// This is an associated const rather than a free-standing assertion because an associated
    /// const is monomorphized *per instantiation*: `Ring<3>` and `Ring<7>` are checked
    /// independently, and neither has to be named in a test. Every safe entry point in this
    /// crate forces it, so using a value is what triggers the check.
    ///
    /// The default is the empty proof, which is what a hand-written impl of a primitive wants.
    #[doc(hidden)]
    const __LAYOUT_OK: () = ();

    /// A 64-bit hash of this type's field structure, or `None` if it is not known.
    ///
    /// For a format header to carry. A reader compares it against the shape it was built with and
    /// refuses the bytes on a mismatch, which catches the layout changes a size check cannot: two
    /// fields of the same width swapped, a field renamed, or a `u32` retyped as an `i32`.
    ///
    /// ```
    /// use portable_pod::Pod;
    ///
    /// #[derive(Clone, Copy, Pod)]
    /// #[repr(C)]
    /// struct V1 { lo: u32, hi: u32 }
    ///
    /// #[derive(Clone, Copy, Pod)]
    /// #[repr(C)]
    /// struct V2 { hi: u32, lo: u32 } // the same size, and every old file misread
    ///
    /// assert_ne!(V1::SHAPE, V2::SHAPE);
    /// ```
    ///
    /// * The integer scalars, `()` and [`Bit`] each have a distinct shape, `[T; N]` combines
    ///   `T`'s with `N`, and `#[derive(Pod)]` folds in each field's name and shape in declaration
    ///   order, each const generic parameter's value, and `size_of::<Self>()`. A type's own name
    ///   is **not** included, so a rename does not invalidate stored data, and `Tick(u64)` and
    ///   `Money(u64)` share a shape. Alignment and `repr` are not included either.
    /// * `None` is what a hand-written impl reports unless it states a shape, and it is
    ///   contagious: a struct or array containing a `None` is `None` too, so a `Some` always
    ///   covers the whole type. [`shape`] has the functions a hand-written impl can use instead.
    /// * Reading `SHAPE` is not a layout check. A generic instantiation's padding proof runs when
    ///   it reaches [`bytes_of`] or another entry point, as always, and the bytes a shape
    ///   describes come from one.
    ///
    /// **The value is a persistence format.** It is the same on every target, and the algorithm
    /// that produces it, documented exactly in [`shape`], changes only in a semver-major release.
    const SHAPE: Option<u64> = None;

    /// Where a derived shape fold starts, reached as `<() as Pod>::__SHAPE_FOLD`. Not part of the
    /// public API.
    ///
    /// This exists so the derive's expansion names nothing but the trait. `#[pod(crate = ...)]`
    /// promises a re-exporting crate that `Pod` is the one item it has to re-export, and a path to
    /// [`shape::Fold`] would break that promise; a value of that type reached through the trait
    /// lets the expansion call its methods without naming it. See DESIGN.md §12.
    #[doc(hidden)]
    const __SHAPE_FOLD: shape::Fold = shape::Fold::new();
}

/// Force `T`'s layout proof to be evaluated for this instantiation.
///
/// Every public entry point calls this. For a concrete type the proof has already run at the
/// definition; for a generic one this is what triggers it, which is why using a value is what
/// surfaces a padded instantiation. See the crate docs, "When the check fires".
#[inline(always)]
fn prove_layout<T: Pod>() {
    // Not a no-op: naming the associated const is precisely what forces const evaluation, and
    // dropping this line would silently disable the layout check for every generic type.
    #[allow(clippy::let_unit_value)]
    let _ = <T as Pod>::__LAYOUT_OK;
}

/// Check `T`'s layout now, failing to compile if it has padding.
///
/// Only needed for **generic** types, and only when an instantiation might never reach another
/// entry point. A concrete type is checked at its definition, and a generic one is checked when
/// it is passed to [`bytes_of`] and friends — but a `Ring<3>` that your program only ever
/// constructs and reads directly is checked by nothing. Naming it here closes that gap:
///
/// ```
/// # use portable_pod::{Pod, assert_layout};
/// # #[derive(Clone, Copy, Pod)]
/// # #[repr(C)]
/// # struct Ring<const N: usize> { slots: [u32; N], len: u32 }
/// assert_layout::<Ring<3>>();
/// ```
///
/// This is a compile-time check that happens to be spelled as a function call; at run time it
/// does nothing.
#[inline(always)]
pub fn assert_layout<T: Pod>() {
    prove_layout::<T>();
}

macro_rules! impl_pod_scalar {
    ($($t:ty => $tag:expr),* $(,)?) => { $(
        // SAFETY: an integer scalar is `Copy`, has no padding, is valid for every bit pattern,
        // and has a width fixed by the type rather than by the target. `usize`/`isize` are
        // excluded precisely because they fail that last point (clause 4).
        unsafe impl Pod for $t {
            const SHAPE: Option<u64> = shape::scalar($tag);
        }
    )* };
}
impl_pod_scalar!(
    u8 => shape::tag::U8,
    u16 => shape::tag::U16,
    u32 => shape::tag::U32,
    u64 => shape::tag::U64,
    u128 => shape::tag::U128,
    i8 => shape::tag::I8,
    i16 => shape::tag::I16,
    i32 => shape::tag::I32,
    i64 => shape::tag::I64,
    i128 => shape::tag::I128,
    () => shape::tag::UNIT,
);

// SAFETY: an array of `Pod` is contiguous with no padding between elements (element stride is
// `size_of::<T>()` exactly), so it inherits every clause from `T`.
unsafe impl<T: Pod, const N: usize> Pod for [T; N] {
    // Forward the element's layout proof rather than taking the empty default. Without this,
    // wrapping a type in an array *erased* its proof: `[Inner<1>; 2]` would take the default
    // `()` here, and nothing would ever force `<Inner<1> as Pod>::__LAYOUT_OK`.
    const __LAYOUT_OK: () = <T as Pod>::__LAYOUT_OK;
    const SHAPE: Option<u64> = shape::array(<T as Pod>::SHAPE, N);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars_round_trip_through_bytes() {
        let v: u64 = 0x0123_4567_89ab_cdef;
        assert_eq!(read_pod::<u64>(bytes_of(&v)), Some(v));
    }

    #[test]
    fn arrays_are_pod() {
        let a: [u32; 3] = [1, 2, 3];
        assert_eq!(bytes_of(&a).len(), 12);
        assert_eq!(read_pod::<[u32; 3]>(bytes_of(&a)), Some(a));
    }

    #[test]
    fn nested_arrays_are_pod() {
        let a: [[u16; 2]; 2] = [[1, 2], [3, 4]];
        assert_eq!(bytes_of(&a).len(), 8);
    }
}
