//! The property the crate exists for: a value's bytes are the same on every target.
//!
//! Nothing else in the suite tests this, because on a single architecture there is nothing to
//! disagree with. CI runs this file on `x86_64`, `aarch64`, and `wasm32-wasip1` (a 32-bit target,
//! which is what makes it a real test rather than a tautology), and all three must agree with the
//! constants below.
//!
//! If a change makes one of these fail, do not update the constant. The constant is the contract.

use portable_pod::{Bit, Pod, bytes_of};

/// Deliberately mixes every width, a flag, an array, and a nested struct.
#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Wire {
    id: u64,
    stamp: i64,
    magic: u32,
    count: i32,
    small: u16,
    tiny: i16,
    flag: Bit,
    kind: u8,
    pad: [u8; 2],
    inner: Inner,
}

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Inner {
    lo: u32,
    hi: u32,
}

fn sample() -> Wire {
    Wire {
        id: 0x0123_4567_89ab_cdef,
        stamp: -2,
        magic: 0xdead_beef,
        count: -7,
        small: 0xfeed,
        tiny: -3,
        flag: Bit::TRUE,
        kind: 9,
        pad: [0, 0],
        inner: Inner { lo: 1, hi: 2 },
    }
}

/// FNV-1a, inlined so this test pulls in no dependency and is itself target-independent.
fn digest(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

#[test]
fn layout_is_identical_on_every_target() {
    // Sizes and offsets must not depend on pointer width. A `usize` field would break these on a
    // 32-bit target, which is exactly what the trait exists to prevent.
    assert_eq!(core::mem::size_of::<Wire>(), 40, "Wire size");
    assert_eq!(core::mem::align_of::<Wire>(), 8, "Wire align");
    assert_eq!(core::mem::offset_of!(Wire, id), 0);
    assert_eq!(core::mem::offset_of!(Wire, stamp), 8);
    assert_eq!(core::mem::offset_of!(Wire, magic), 16);
    assert_eq!(core::mem::offset_of!(Wire, count), 20);
    assert_eq!(core::mem::offset_of!(Wire, small), 24);
    assert_eq!(core::mem::offset_of!(Wire, tiny), 26);
    assert_eq!(core::mem::offset_of!(Wire, flag), 28);
    assert_eq!(core::mem::offset_of!(Wire, kind), 29);
    assert_eq!(core::mem::offset_of!(Wire, pad), 30);
    assert_eq!(core::mem::offset_of!(Wire, inner), 32);
    assert_eq!(bytes_of(&sample()).len(), 40);
}

/// The bytes themselves, asserted unconditionally.
///
/// This used to carry `#[cfg(target_endian = "little")]`, which meant the one test that could
/// observe a byte-order disagreement switched itself off on precisely the target that would have
/// one. The crate now refuses to build for a big-endian target at all, so the guard is neither
/// needed nor wanted: every target that reaches this line owes the same bytes.
#[test]
fn bytes_are_identical_on_every_supported_target() {
    assert_eq!(
        digest(bytes_of(&sample())),
        0xa1cd_0028_b694_74da,
        "Wire digest"
    );
}

#[test]
fn zero_value_digest_depends_only_on_size() {
    let z = portable_pod::zeroed::<Wire>();
    assert!(
        bytes_of(&z).iter().all(|&b| b == 0),
        "no padding byte is left uninitialized"
    );
    assert_eq!(digest(bytes_of(&z)), digest(&[0u8; 40]));
}
