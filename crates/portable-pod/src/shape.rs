//! The shape hash behind [`Pod::SHAPE`](crate::Pod::SHAPE): a 64-bit summary of a type's field
//! structure, for a format header to carry so that a reader can refuse bytes written against a
//! different layout instead of misreading them.
//!
//! **The algorithm on this page is a persistence format.** Shapes are meant to be written into
//! files, replays and wire headers, so every value it produces is frozen: the constants, the
//! encoding and the built-in tags below change only in a semver-major release of this crate.
//! `tests/shape.rs` pins exact values for every built-in and for each derive feature, and those
//! goldens are the freeze.
//!
//! # What a shape covers
//!
//! | Folded in | Not folded in |
//! | --- | --- |
//! | each field's name, in declaration order | the type's own name |
//! | each field type's shape | the `repr` (`C` or `transparent`) and any `align(N)` |
//! | each const generic parameter's value, in declaration order | alignment, which varies by target |
//! | the value of `#[pod(shape_with = …)]`, if given | field visibility and attributes |
//! | `size_of::<Self>()` | the names of generic parameters |
//! | | a const generic parameter's *type* |
//!
//! A field's name is its identifier as rustc holds it: NFC-normalized, as rustc normalizes every
//! identifier, and without a raw identifier's `r#` (`r#type` is `type`). A tuple field's name is
//! its index in decimal, `"0"`, `"1"`, ….
//!
//! Because a const parameter contributes its value but not its type, parameters of different types
//! whose values widen to the same 128 bits (below) are indistinguishable: `S<true>` with
//! `const B: bool` and `S<1>` with `const B: u8` share a shape, as do `-1` as an `i8` and
//! `u128::MAX`. Changing a parameter's type without changing what it means is therefore not a
//! change, and changing what it means should come with a new name anyway.
//!
//! So reordering, renaming or retyping a field changes the shape even where the size does not,
//! and so does wrapping a field in a newtype (`u64` to `Tick(u64)`). Renaming a *type* does not:
//! `struct Tick(u64)` and `struct Money(u64)` have the same shape, because a rename must not
//! invalidate every save file that stored one. A field whose type is `Money` where it used to be
//! `Tick` is therefore not detected; give such types a [`shape_with`](#extending-a-derived-shape)
//! if they must be told apart.
//!
//! A shape is `None` when any part of it is unknown: a type whose `Pod` impl was written by hand
//! and states no shape, or any struct or array containing one. `None` is contagious so that a
//! shape which is `Some` always covers the whole type.
//!
//! # The algorithm
//!
//! The state is a `u64`, and every step absorbs one 64-bit word `w`:
//!
//! ```text
//! absorb(s, w) = mix(s ^ w)
//! mix(z)       = z ^= z >> 30;  z *= 0xbf58476d1ce4e5b9;
//!                z ^= z >> 27;  z *= 0x94d049bb133111eb;
//!                z ^ (z >> 31)                               (wrapping multiplication)
//! ```
//!
//! `mix` is the SplitMix64 finalizer, a bijection on `u64` with full avalanche. Every shape starts
//! from `IV = 0x243f6a8885a308d3` (the first 64 fractional bits of π) and absorbs a sequence of
//! words, the first of which names the kind of shape:
//!
//! | Shape | Words absorbed, in order |
//! | --- | --- |
//! | [`scalar`]`(tag)` | `1`, `tag` |
//! | [`array`](fn@array)`(element, len)` | `2`, `element`, `len` |
//! | a struct ([`Fold`]) | `3`, then one record per field, parameter and extension below, then `7`, `size` |
//! | …a field ([`Fold::field`], or each of [`Fold::fields`]) | `4`, the name's length in bytes, the name's bytes, `shape` |
//! | …a parameter ([`Fold::param`]) | `5`, the low 64 bits of the value, the high 64 bits |
//! | …an extension ([`Fold::with`]) | `6`, `value` |
//!
//! A name's bytes are its UTF-8, packed eight to a word in little-endian order, with the last word
//! zero-filled; an empty name contributes no words after its length. Integers (`len`, `size`) are
//! absorbed as `u64`. A const parameter's value is first widened to 128 bits, as Rust's `as u128`
//! does: an unsigned integer zero-extends, a signed one sign-extends (two's complement), `bool` is
//! `0` or `1`, and `char` is its Unicode scalar value. The shape is the state after the last word.
//!
//! Every record opens with its own kind word and has a length fixed by what precedes it, so two
//! different structures always absorb different word sequences; two shapes agree only by a 64-bit
//! collision.
//!
//! The built-in tags are:
//!
//! | `u8` | `u16` | `u32` | `u64` | `u128` | `i8` | `i16` | `i32` | `i64` | `i128` | `()` | [`Bit`](crate::Bit) |
//! | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
//! | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 |
//!
//! Tags `0..=0xFFFF` are reserved for this crate.
//!
//! # Extending a derived shape
//!
//! `#[pod(shape_with = <expr>)]` folds one more `u64` constant expression into a derived shape,
//! after the fields and parameters. It is for meaning the fields cannot express. The usual case is
//! a wrapper holding an enum's discriminant as an integer, whose shape should change when the
//! enum's variant table does:
//!
//! ```
//! use portable_pod::Pod;
//!
//! #[derive(Clone, Copy, Pod)]
//! #[repr(transparent)]
//! #[pod(shape_with = 0x5ca1_ab1e)] // e.g. a hash of the variant names, kept beside the enum
//! struct KindCode(u8);
//!
//! assert_ne!(KindCode::SHAPE, None);
//! ```
//!
//! # Hand-written impls
//!
//! A hand-written `Pod` impl reports `None` unless it says otherwise. One that wants a shape can
//! compute it with these functions, which are the same ones the derive uses:
//!
//! ```
//! use portable_pod::{Pod, shape};
//!
//! #[derive(Clone, Copy)]
//! #[repr(transparent)]
//! struct Fixed(i32);
//!
//! // SAFETY: `repr(transparent)` over `i32`.
//! unsafe impl Pod for Fixed {
//!     // A primitive of this program's own: a tag outside the reserved range.
//!     const SHAPE: Option<u64> = shape::scalar(0x46_4958_4544);
//! }
//! ```

/// The initial state: the first 64 fractional bits of π.
const IV: u64 = 0x243f_6a88_85a3_08d3;

// Kind words. Each record of the encoding opens with one, which is what keeps the encoding
// unambiguous; see the module docs.
const KIND_SCALAR: u64 = 1;
const KIND_ARRAY: u64 = 2;
const KIND_STRUCT: u64 = 3;
const KIND_FIELD: u64 = 4;
const KIND_PARAM: u64 = 5;
const KIND_WITH: u64 = 6;
const KIND_END: u64 = 7;

/// The tags of the built-in scalars, in one place because they are frozen together.
pub(crate) mod tag {
    pub(crate) const U8: u64 = 1;
    pub(crate) const U16: u64 = 2;
    pub(crate) const U32: u64 = 3;
    pub(crate) const U64: u64 = 4;
    pub(crate) const U128: u64 = 5;
    pub(crate) const I8: u64 = 6;
    pub(crate) const I16: u64 = 7;
    pub(crate) const I32: u64 = 8;
    pub(crate) const I64: u64 = 9;
    pub(crate) const I128: u64 = 10;
    pub(crate) const UNIT: u64 = 11;
    pub(crate) const BIT: u64 = 12;
}

/// The SplitMix64 finalizer.
const fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Absorb one word into the state.
const fn absorb(state: u64, word: u64) -> u64 {
    mix(state ^ word)
}

/// The shape of a primitive identified by `tag`.
///
/// The built-in scalars use tags `1..=12` (see the [module docs](self)), and `0..=0xFFFF` is
/// reserved. A hand-written impl for a primitive of your own should use a tag outside that range,
/// and keep it fixed: it is persisted wherever the shape is.
#[must_use]
pub const fn scalar(tag: u64) -> Option<u64> {
    Some(absorb(absorb(IV, KIND_SCALAR), tag))
}

/// The shape of `[T; len]`, where `element` is `T`'s shape. `None` if `element` is.
#[must_use]
pub const fn array(element: Option<u64>, len: usize) -> Option<u64> {
    match element {
        Some(element) => Some(absorb(absorb(absorb(IV, KIND_ARRAY), element), len as u64)),
        None => None,
    }
}

/// A struct's shape, being folded: the fields in declaration order, then the const generic
/// parameters, then any extension, then [`finish`](Fold::finish).
///
/// This is what `#[derive(Pod)]` emits, so a hand-written impl that follows the same steps gets
/// the shape the derive would have given it:
///
/// ```
/// use portable_pod::{Pod, shape::Fold};
///
/// #[derive(Clone, Copy, Pod)]
/// #[repr(C)]
/// struct Header {
///     magic: u32,
///     version: u32,
/// }
///
/// let by_hand = Fold::new()
///     .field("magic", u32::SHAPE)
///     .field("version", u32::SHAPE)
///     .finish(8);
/// assert_eq!(Header::SHAPE, by_hand);
/// ```
///
/// Once any input is `None` the fold stays `None`, and so does its result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub struct Fold {
    state: Option<u64>,
}

impl Fold {
    /// An empty struct fold.
    pub const fn new() -> Fold {
        Fold {
            state: Some(absorb(IV, KIND_STRUCT)),
        }
    }

    /// Fold in a field: its name (a tuple field's index, written `"0"`, `"1"`, …) and its type's
    /// shape.
    pub const fn field(self, name: &str, shape: Option<u64>) -> Fold {
        let (Some(mut s), Some(shape)) = (self.state, shape) else {
            return Fold { state: None };
        };
        let bytes = name.as_bytes();
        s = absorb(absorb(s, KIND_FIELD), bytes.len() as u64);
        let mut i = 0;
        while i < bytes.len() {
            let mut word = 0u64;
            let mut k = 0;
            while k < 8 && i + k < bytes.len() {
                word |= (bytes[i + k] as u64) << (8 * k);
                k += 1;
            }
            s = absorb(s, word);
            i += 8;
        }
        Fold {
            state: Some(absorb(s, shape)),
        }
    }

    /// Fold in several fields, in order: the same as calling [`field`](Fold::field) for each.
    pub const fn fields(self, fields: &[(&str, Option<u64>)]) -> Fold {
        let mut fold = self;
        let mut i = 0;
        while i < fields.len() {
            fold = fold.field(fields[i].0, fields[i].1);
            i += 1;
        }
        fold
    }

    /// Fold in a const generic parameter's value.
    ///
    /// The derive passes each parameter cast `as u128`, which is lossless for every type a const
    /// parameter can have (an integer, `bool` or `char`; a signed value sign-extends).
    pub const fn param(self, value: u128) -> Fold {
        match self.state {
            Some(s) => Fold {
                state: Some(absorb(
                    absorb(absorb(s, KIND_PARAM), value as u64),
                    (value >> 64) as u64,
                )),
            },
            None => self,
        }
    }

    /// Fold in an extension value, what `#[pod(shape_with = …)]` supplies.
    pub const fn with(self, value: u64) -> Fold {
        match self.state {
            Some(s) => Fold {
                state: Some(absorb(absorb(s, KIND_WITH), value)),
            },
            None => self,
        }
    }

    /// Close the fold with the struct's size in bytes, giving its shape.
    #[must_use]
    pub const fn finish(self, size: usize) -> Option<u64> {
        match self.state {
            Some(s) => Some(absorb(absorb(s, KIND_END), size as u64)),
            None => None,
        }
    }
}

impl Default for Fold {
    fn default() -> Fold {
        Fold::new()
    }
}
