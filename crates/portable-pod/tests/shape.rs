//! `Pod::SHAPE` and the `shape` module.
//!
//! **The goldens below are the freeze.** Shapes are persisted by consumers, in save files, replays
//! and wire headers, so every value here is part of this crate's format: if a change makes one of
//! them fail, the change is wrong, not the golden. They move only in a semver-major release, and
//! then all together.
//!
//! CI runs this file on 64-bit hosts and on `wasm32-wasip1`, a 32-bit target, and all of them must
//! agree with the same constants.

use portable_pod::shape::{self, Fold};
use portable_pod::{Bit, Pod};

// ---- Types under test ------------------------------------------------------------------------

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Header {
    magic: u32,
    version: u32,
}

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Nested {
    head: Header,
    flags: [Bit; 8],
}

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Pair(u32, u32);

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Unit;

/// A fixed-point number whose fraction width is a const parameter: `Fixed<32>` and `Fixed<24>`
/// have the same fields and the same size, and must not share a shape.
#[derive(Clone, Copy, Pod)]
#[repr(transparent)]
struct Fixed<const FRAC: u8> {
    raw: i64,
}

/// A type whose `Pod` impl is hand-written and states no shape.
#[derive(Clone, Copy)]
#[repr(transparent)]
struct Opaque(u32);

// SAFETY: `repr(transparent)` over `u32`.
unsafe impl Pod for Opaque {}

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct HoldsOpaque {
    id: u32,
    opaque: Opaque,
}

/// An enum code wrapper whose shape carries its variant table.
#[derive(Clone, Copy, Pod)]
#[repr(transparent)]
#[pod(shape_with = 0x0123_4567_89ab_cdef)]
struct KindCode(u8);

// ---- The freeze ------------------------------------------------------------------------------

/// Exact values, in the order the tag table in `portable_pod::shape` lists the built-ins.
#[test]
fn golden_scalars() {
    assert_shape(u8::SHAPE, 0x6e3b_8baf_baa6_c96a);
    assert_shape(u16::SHAPE, 0x6142_4735_d519_0a94);
    assert_shape(u32::SHAPE, 0xe207_701b_bdeb_e879);
    assert_shape(u64::SHAPE, 0x116b_3822_8851_2a79);
    assert_shape(u128::SHAPE, 0x407c_93fa_2fa2_1d6f);
    assert_shape(i8::SHAPE, 0x131a_175d_b706_2130);
    assert_shape(i16::SHAPE, 0xd73e_e235_7d94_82d5);
    assert_shape(i32::SHAPE, 0xc7b2_ad6c_a962_8fe8);
    assert_shape(i64::SHAPE, 0x05cb_e160_6bc0_23a1);
    assert_shape(i128::SHAPE, 0x518a_806c_65f9_cbec);
    assert_shape(<()>::SHAPE, 0x808a_eead_b79c_d0e7);
    assert_shape(Bit::SHAPE, 0x2fd8_0f87_8472_9e84);
}

#[test]
fn golden_array() {
    assert_shape(<[u32; 4]>::SHAPE, 0x12e5_2656_3fdb_21f3);
}

#[test]
fn golden_nested_struct() {
    assert_shape(Header::SHAPE, 0x7f7b_c78c_0e53_9971);
    assert_shape(Nested::SHAPE, 0x8b60_d865_b9df_cc16);
}

#[test]
fn golden_tuple_struct() {
    assert_shape(Pair::SHAPE, 0xc856_3c26_a376_94f2);
}

#[test]
fn golden_unit_struct() {
    assert_shape(Unit::SHAPE, 0xb9d6_6b6e_f374_3364);
}

#[test]
fn golden_const_generic() {
    assert_shape(Fixed::<32>::SHAPE, 0xac7a_c6e1_0d7c_d550);
    assert_shape(Fixed::<24>::SHAPE, 0x63c6_02f3_0a69_2e64);
}

#[test]
fn golden_shape_with() {
    assert_shape(KindCode::SHAPE, 0xdaba_6eb8_5b8d_d731);
}

#[test]
fn golden_none_field() {
    assert_eq!(Opaque::SHAPE, None);
    assert_eq!(HoldsOpaque::SHAPE, None);
}

#[track_caller]
fn assert_shape(actual: Option<u64>, golden: u64) {
    let actual = actual.expect("a shape, not `None`");
    assert_eq!(
        actual, golden,
        "shape {actual:#018x} differs from the golden {golden:#018x}; see the top of this file"
    );
}

// ---- The documentation is the algorithm --------------------------------------------------------

/// The algorithm as `portable_pod::shape`'s docs state it, written from that text alone and
/// sharing no code with the crate. If these agree with the goldens, the docs are exact, which is
/// what lets another implementation, in another language if need be, reproduce a shape.
mod reference {
    pub fn hash(words: &[u64]) -> u64 {
        let mut s: u64 = 0x243f_6a88_85a3_08d3;
        for &w in words {
            let mut z = s ^ w;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            s = z ^ (z >> 31);
        }
        s
    }

    /// A field record: `4`, the name's length, its bytes eight to a little-endian word, the shape.
    pub fn field(name: &str, shape: u64) -> Vec<u64> {
        let mut words = vec![4, name.len() as u64];
        for chunk in name.as_bytes().chunks(8) {
            let mut padded = [0u8; 8];
            padded[..chunk.len()].copy_from_slice(chunk);
            words.push(u64::from_le_bytes(padded));
        }
        words.push(shape);
        words
    }

    /// A struct: `3`, its records, then `7` and the size.
    pub fn structure(records: &[Vec<u64>], size: u64) -> u64 {
        let mut words = vec![3];
        for r in records {
            words.extend_from_slice(r);
        }
        words.extend_from_slice(&[7, size]);
        hash(&words)
    }
}

#[test]
fn the_documented_algorithm_reproduces_the_goldens() {
    use reference::{field, hash, structure};
    let tags: [(Option<u64>, u64); 12] = [
        (u8::SHAPE, 1),
        (u16::SHAPE, 2),
        (u32::SHAPE, 3),
        (u64::SHAPE, 4),
        (u128::SHAPE, 5),
        (i8::SHAPE, 6),
        (i16::SHAPE, 7),
        (i32::SHAPE, 8),
        (i64::SHAPE, 9),
        (i128::SHAPE, 10),
        (<()>::SHAPE, 11),
        (Bit::SHAPE, 12),
    ];
    for (shape, tag) in tags {
        assert_eq!(shape, Some(hash(&[1, tag])), "tag {tag}");
    }

    let u32_ = hash(&[1, 3]);
    assert_eq!(<[u32; 4]>::SHAPE, Some(hash(&[2, u32_, 4])));

    let header = structure(&[field("magic", u32_), field("version", u32_)], 8);
    assert_eq!(Header::SHAPE, Some(header));
    let flags = hash(&[2, hash(&[1, 12]), 8]);
    assert_eq!(
        Nested::SHAPE,
        Some(structure(
            &[field("head", header), field("flags", flags)],
            16
        ))
    );
    assert_eq!(
        Pair::SHAPE,
        Some(structure(&[field("0", u32_), field("1", u32_)], 8))
    );
    assert_eq!(Unit::SHAPE, Some(structure(&[], 0)));

    // A parameter record: `5`, then the value's low and high 64 bits.
    let i64_ = hash(&[1, 9]);
    assert_eq!(
        Fixed::<32>::SHAPE,
        Some(structure(&[field("raw", i64_), vec![5, 32, 0]], 8))
    );

    // An extension record: `6`, then the value.
    let u8_ = hash(&[1, 1]);
    assert_eq!(
        KindCode::SHAPE,
        Some(structure(
            &[field("0", u8_), vec![6, 0x0123_4567_89ab_cdef]],
            1
        ))
    );

    // A name longer than one word, and one that is an exact multiple of eight bytes.
    assert_eq!(
        Fold::new()
            .field("a_longer_name", u32::SHAPE)
            .field("eightchr", u32::SHAPE)
            .finish(8),
        Some(structure(
            &[field("a_longer_name", u32_), field("eightchr", u32_)],
            8
        ))
    );
}

// ---- What a shape does and does not see --------------------------------------------------------

#[test]
fn the_derive_is_the_documented_fold() {
    assert_eq!(
        Header::SHAPE,
        Fold::new()
            .field("magic", u32::SHAPE)
            .field("version", u32::SHAPE)
            .finish(8)
    );
    assert_eq!(
        Fixed::<32>::SHAPE,
        Fold::new().field("raw", i64::SHAPE).param(32).finish(8)
    );
    assert_eq!(
        KindCode::SHAPE,
        Fold::new()
            .field("0", u8::SHAPE)
            .with(0x0123_4567_89ab_cdef)
            .finish(1)
    );
    assert_eq!(
        <[Header; 3]>::SHAPE,
        shape::array(Header::SHAPE, 3),
        "arrays of structs compose"
    );
}

mod same_size_changes {
    use super::*;

    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    pub struct Original {
        pub lo: u32,
        pub hi: u32,
    }

    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    pub struct Reordered {
        pub hi: u32,
        pub lo: u32,
    }

    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    pub struct Renamed {
        pub low: u32,
        pub hi: u32,
    }

    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    pub struct Retyped {
        pub lo: i32,
        pub hi: u32,
    }

    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    pub struct AsBytes {
        pub lo: [u8; 4],
        pub hi: u32,
    }

    /// The type's own name is not part of its shape.
    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    pub struct Original2 {
        pub lo: u32,
        pub hi: u32,
    }

    #[test]
    fn every_same_size_edit_changes_the_shape_and_a_rename_does_not() {
        let original = Original::SHAPE;
        assert_ne!(original, Reordered::SHAPE);
        assert_ne!(original, Renamed::SHAPE);
        assert_ne!(original, Retyped::SHAPE);
        assert_ne!(original, AsBytes::SHAPE);
        assert_eq!(original, Original2::SHAPE);
    }
}

#[test]
fn a_rename_is_not_a_change_but_a_newtype_is() {
    #[derive(Clone, Copy, Pod)]
    #[repr(transparent)]
    struct Tick(u64);
    #[derive(Clone, Copy, Pod)]
    #[repr(transparent)]
    struct Money(u64);

    // Documented: the type name is not folded, so these two cannot be told apart.
    assert_eq!(Tick::SHAPE, Money::SHAPE);
    // Wrapping is a change: `Tick` is a struct with a field `0`, not a `u64`.
    assert_ne!(Tick::SHAPE, u64::SHAPE);
}

#[test]
fn arrays_fold_their_length_and_nest_in_order() {
    assert_ne!(<[u32; 3]>::SHAPE, <[u32; 4]>::SHAPE);
    assert_ne!(<[[u8; 2]; 3]>::SHAPE, <[[u8; 3]; 2]>::SHAPE);
    assert_ne!(<[u8; 4]>::SHAPE, u32::SHAPE);
    assert_ne!(<[u32; 1]>::SHAPE, u32::SHAPE);
    assert_ne!(Bit::SHAPE, u8::SHAPE);
}

#[test]
fn none_is_contagious() {
    assert_eq!(<[Opaque; 2]>::SHAPE, None);
    assert_eq!(shape::array(None, 2), None);
    assert_eq!(Fold::new().field("a", None).finish(4), None);
    // Once tainted, later steps cannot restore it.
    assert_eq!(
        Fold::new()
            .field("a", None)
            .field("b", u32::SHAPE)
            .param(1)
            .with(2)
            .finish(8),
        None
    );

    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    struct Deep {
        inner: HoldsOpaque,
        arr: [HoldsOpaque; 2],
    }
    assert_eq!(Deep::SHAPE, None);
}

mod generics {
    use super::*;

    /// A type parameter: the shape follows the instantiation through the field.
    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    pub struct Wrap<T> {
        pub v: T,
    }

    /// Every kind of const parameter stable Rust allows, including a negative one.
    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    pub struct Params<const B: bool, const C: char, const I: i8, const W: u128> {
        pub x: u8,
    }

    /// A `shape_with` that names the type's own parameter.
    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    #[pod(shape_with = ((VERSION as u64) << 8))]
    pub struct Versioned<const VERSION: u16> {
        pub x: u32,
    }

    #[test]
    fn type_parameters_reach_the_shape_through_their_fields() {
        assert_eq!(
            Wrap::<u32>::SHAPE,
            Fold::new().field("v", u32::SHAPE).finish(4)
        );
        assert_ne!(Wrap::<u32>::SHAPE, Wrap::<i32>::SHAPE);
        assert_eq!(Wrap::<Opaque>::SHAPE, None);
    }

    #[test]
    fn every_const_parameter_kind_is_folded_losslessly() {
        let base = Params::<true, 'a', -1, 0>::SHAPE;
        assert_ne!(base, Params::<false, 'a', -1, 0>::SHAPE);
        assert_ne!(base, Params::<true, 'b', -1, 0>::SHAPE);
        assert_ne!(base, Params::<true, 'a', 1, 0>::SHAPE);
        // The high half of a `u128` is not truncated away.
        assert_ne!(
            Params::<true, 'a', -1, { 1 << 64 }>::SHAPE,
            Params::<true, 'a', -1, 0>::SHAPE
        );
        // Declaration order, each cast `as u128`; `-1i8` sign-extends.
        assert_eq!(
            base,
            Fold::new()
                .field("x", u8::SHAPE)
                .param(1)
                .param('a' as u128)
                .param(u128::MAX)
                .param(0)
                .finish(1)
        );
    }

    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    pub struct AsBool<const P: bool> {
        pub x: u8,
    }

    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    pub struct AsU8<const P: u8> {
        pub x: u8,
    }

    /// Documented in `portable_pod::shape`: a parameter contributes its value widened to 128 bits,
    /// not its type.
    #[test]
    fn a_const_parameters_type_is_not_folded() {
        assert_eq!(AsBool::<true>::SHAPE, AsU8::<1>::SHAPE);
        assert_eq!(AsBool::<false>::SHAPE, AsU8::<0>::SHAPE);
    }

    #[test]
    fn shape_with_can_name_parameters() {
        assert_eq!(
            Versioned::<3>::SHAPE,
            Fold::new()
                .field("x", u32::SHAPE)
                .param(3)
                .with(3 << 8)
                .finish(4)
        );
    }
}

#[test]
fn a_raw_identifier_is_its_plain_name() {
    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    struct Keyword {
        r#type: u32,
    }
    assert_eq!(
        Keyword::SHAPE,
        Fold::new().field("type", u32::SHAPE).finish(4)
    );
}

/// The derive reaches the fold through the trait, so a crate that re-exports only `Pod` still
/// gets shapes.
mod reexport {
    pub use portable_pod::Pod;
}

#[test]
fn shapes_work_through_a_reexport() {
    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    #[pod(crate = crate::reexport)]
    struct Via {
        magic: u32,
        version: u32,
    }
    assert_eq!(Via::SHAPE, Header::SHAPE);
}

/// A shape is usable where a header needs it: in a `const`.
#[test]
fn shapes_are_constants() {
    const HEADER: Option<u64> = Header::SHAPE;
    const TABLE: [Option<u64>; 2] = [Header::SHAPE, Nested::SHAPE];
    assert_eq!(HEADER, TABLE[0]);
}

/// The expansion names every type by its absolute path, so a scope that rebinds the primitive and
/// prelude names it uses changes nothing. For these types compiling is most of the assertion.
#[allow(non_camel_case_types, dead_code)]
mod shadowed_names {
    use portable_pod::Pod;

    type u64 = u32;
    type u128 = u8;
    type usize = u16;
    pub struct Option;
    pub struct Some;
    pub struct None;

    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    #[pod(size = 8, shape_with = 7)]
    pub struct Concrete {
        pub a: u64,
        pub b: u64,
    }

    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    #[pod(size = 4 * N, shape_with = 7)]
    pub struct Generic<const N: core::primitive::usize> {
        pub a: [u64; N],
    }

    #[test]
    fn shapes_are_unaffected() {
        use portable_pod::shape::Fold;
        let expected = Fold::new()
            .field("a", <u32 as Pod>::SHAPE)
            .field("b", <u32 as Pod>::SHAPE)
            .with(7)
            .finish(8);
        assert_eq!(<Concrete as Pod>::SHAPE, expected);
        assert_eq!(
            <Generic<2> as Pod>::SHAPE,
            Fold::new()
                .field("a", <[u32; 2] as Pod>::SHAPE)
                .param(2)
                .with(7)
                .finish(8)
        );
    }
}
