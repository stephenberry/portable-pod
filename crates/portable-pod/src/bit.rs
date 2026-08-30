use crate::Pod;

/// A one-byte flag whose value is defined for **every** bit pattern.
///
/// [`Pod`] clause 2 rules out `bool`: only `0` and `1` are valid bools, and materializing any
/// other byte as a `bool` is undefined behavior. That makes `bool` unsound to reconstruct from
/// bytes you did not write — a save file, a network packet, a memory-mapped buffer.
///
/// `Bit` is the replacement. Zero reads as `false`, any nonzero byte reads as `true`, so all 256
/// patterns denote a value. Writers always store the canonical `0` or `1`, so ordinary bytes stay
/// canonical and comparisons and checksums behave. A hostile input that injects `0x7f` therefore
/// produces a *different value*, never undefined behavior — which is the entire distinction.
///
/// ```
/// use portable_pod::Bit;
///
/// let flag = Bit::new(true);
/// assert!(flag.get());
/// assert_eq!(Bit::from(false), Bit::FALSE);
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Bit(u8);

impl Bit {
    /// The cleared flag.
    pub const FALSE: Self = Bit(0);
    /// The set flag.
    pub const TRUE: Self = Bit(1);

    /// A flag from a `bool`, storing the canonical byte (`0` or `1`).
    #[inline]
    #[must_use]
    pub const fn new(set: bool) -> Self {
        Bit(set as u8)
    }

    /// Whether the flag is set. Defined for every byte: any nonzero value reads as `true`.
    #[inline]
    #[must_use]
    pub const fn get(self) -> bool {
        self.0 != 0
    }

    /// Set the flag, storing the canonical byte (`0` or `1`).
    #[inline]
    pub fn set(&mut self, set: bool) {
        self.0 = set as u8;
    }

    /// The raw byte, which is not necessarily `0` or `1` if this value came from foreign bytes.
    #[inline]
    #[must_use]
    pub const fn to_byte(self) -> u8 {
        self.0
    }
}

impl From<bool> for Bit {
    #[inline]
    fn from(set: bool) -> Self {
        Bit::new(set)
    }
}

impl From<Bit> for bool {
    #[inline]
    fn from(bit: Bit) -> Self {
        bit.get()
    }
}

impl core::fmt::Debug for Bit {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&self.get(), f)
    }
}

// SAFETY: `#[repr(transparent)]` over `u8`, so `Copy`, exactly one byte, no padding, and no
// target-dependent width. Crucially every one of the 256 bit patterns is a valid `Bit`: the value
// is read via `self.0 != 0`, which is defined for all of them. That any-bit-pattern property is
// what `bool` lacks and is the entire reason this type exists.
unsafe impl Pod for Bit {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{bytes_of, read_pod};

    #[test]
    fn canonical_writes() {
        assert_eq!(Bit::new(true).to_byte(), 1);
        assert_eq!(Bit::new(false).to_byte(), 0);
    }

    #[test]
    fn every_byte_is_a_valid_value() {
        // The property `bool` lacks. All 256 patterns must round-trip without UB.
        for b in 0u8..=255 {
            let bit = read_pod::<Bit>(&[b]).expect("one byte is one Bit");
            assert_eq!(bit.get(), b != 0);
            assert_eq!(bit.to_byte(), b);
        }
    }

    #[test]
    fn noncanonical_input_is_a_different_value_not_ub() {
        let odd = read_pod::<Bit>(&[0x7f]).unwrap();
        assert!(odd.get());
        // It compares unequal to the canonical TRUE, so a checksum notices. That is the
        // intended failure mode: a detectable difference, never undefined behavior.
        assert_ne!(odd, Bit::TRUE);
    }

    #[test]
    fn round_trips_as_bytes() {
        assert_eq!(bytes_of(&Bit::TRUE), &[1]);
    }
}
