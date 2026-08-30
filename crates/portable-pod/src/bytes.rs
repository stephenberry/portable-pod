use crate::Pod;

/// A freshly zeroed value.
///
/// Sound because [`Pod`] clause 2 makes the all-zero bit pattern a valid value for every `Pod`
/// type. This is the way to build a large fixed-size struct without a field-by-field initializer
/// and without leaving padding uninitialized.
#[inline]
#[must_use]
pub fn zeroed<T: Pod>() -> T {
    crate::prove_layout::<T>();
    // SAFETY: `T: Pod` guarantees the all-zero bit pattern is a valid `T` (clause 2).
    unsafe { core::mem::zeroed() }
}

/// The bytes of a value.
///
/// The returned slice is `size_of::<T>()` long. By the [`Pod`] contract its *layout* is the same
/// on every target — same length, same field offsets, no padding — but the byte order within a
/// multi-byte integer is the target's native one. See
/// [What "portable" does and does not mean](crate#what-portable-does-and-does-not-mean).
#[inline]
#[must_use]
pub fn bytes_of<T: Pod>(v: &T) -> &[u8] {
    crate::prove_layout::<T>();
    // SAFETY: `T: Pod` is padding-free (clause 3), so every byte in the range is initialized;
    // `u8` has alignment 1, so the cast cannot misalign; and the lifetime is tied to `v`.
    unsafe { core::slice::from_raw_parts(core::ptr::from_ref(v).cast::<u8>(), size_of::<T>()) }
}

/// The bytes of a value, mutably.
///
/// Any byte sequence written through this slice leaves a valid `T`, by [`Pod`] clause 2.
#[inline]
#[must_use]
pub fn bytes_of_mut<T: Pod>(v: &mut T) -> &mut [u8] {
    crate::prove_layout::<T>();
    // SAFETY: as `bytes_of`, plus: writing arbitrary bytes through this slice cannot form an
    // invalid `T`, because `T: Pod` is valid for every bit pattern (clause 2).
    unsafe { core::slice::from_raw_parts_mut(core::ptr::from_mut(v).cast::<u8>(), size_of::<T>()) }
}

/// The bytes of a slice of values.
#[inline]
#[must_use]
pub fn bytes_of_slice<T: Pod>(s: &[T]) -> &[u8] {
    crate::prove_layout::<T>();
    // SAFETY: as `bytes_of`. A slice of `Pod` is contiguous with stride `size_of::<T>()` and no
    // padding between elements, so `len * size_of::<T>()` bytes are all initialized. The product
    // cannot overflow: the slice already exists, so its total size fits in `isize`.
    unsafe { core::slice::from_raw_parts(s.as_ptr().cast::<u8>(), core::mem::size_of_val(s)) }
}

/// The bytes of a slice of values, mutably.
#[inline]
#[must_use]
pub fn bytes_of_slice_mut<T: Pod>(s: &mut [T]) -> &mut [u8] {
    crate::prove_layout::<T>();
    let len = core::mem::size_of_val(s);
    // SAFETY: as `bytes_of_slice` and `bytes_of_mut`.
    unsafe { core::slice::from_raw_parts_mut(s.as_mut_ptr().cast::<u8>(), len) }
}

/// Read a value from bytes, or `None` if the length is wrong.
///
/// This is the untrusted-input entry point, and it is total: **any** `size_of::<T>()` bytes
/// produce some valid `T`, by [`Pod`] clause 2. There is no bit pattern that fails, which is why
/// `bool` cannot be `Pod` and [`Bit`](crate::Bit) exists.
///
/// The bytes need not be aligned; they are copied.
#[inline]
#[must_use]
pub fn read_pod<T: Pod>(bytes: &[u8]) -> Option<T> {
    crate::prove_layout::<T>();
    if bytes.len() != size_of::<T>() {
        return None;
    }
    let mut out = zeroed::<T>();
    bytes_of_mut(&mut out).copy_from_slice(bytes);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_pod_rejects_wrong_length() {
        assert_eq!(read_pod::<u32>(&[1, 2, 3]), None);
        assert_eq!(read_pod::<u32>(&[1, 2, 3, 4, 5]), None);
        assert_eq!(
            read_pod::<u32>(&[1, 2, 3, 4]),
            Some(u32::from_ne_bytes([1, 2, 3, 4]))
        );
    }

    #[test]
    fn read_pod_accepts_unaligned_input() {
        let buf = [0u8; 9];
        // Offset 1 is not 4-aligned; `read_pod` copies, so this must still work.
        assert_eq!(read_pod::<u32>(&buf[1..5]), Some(0));
    }

    #[test]
    fn mutation_through_bytes_is_visible() {
        let mut v: u32 = 0;
        bytes_of_mut(&mut v).copy_from_slice(&[0xff, 0, 0, 0]);
        assert_eq!(v, u32::from_ne_bytes([0xff, 0, 0, 0]));
    }

    #[test]
    fn slice_bytes_have_no_inter_element_padding() {
        let s: [u16; 4] = [1, 2, 3, 4];
        assert_eq!(bytes_of_slice(&s).len(), 8);
    }

    #[test]
    fn empty_slice_is_empty() {
        let s: [u32; 0] = [];
        assert!(bytes_of_slice(&s).is_empty());
    }
}
