/// Memcpy
///
/// @param dst - Destination
/// @param src - Source
/// @param n - Number of bytes to copy
#[inline]
pub fn unsafe_memcpy(dst: &mut [u8], src: &[u8], n: usize) {
    unsafe {
        let dst = dst.as_mut_ptr();
        let src = src.as_ptr();
        // cannot be overlapping
        assert!(
            is_nonoverlapping(src as usize, dst as usize, n),
            "memcpy does not support overlapping regions"
        );
        std::ptr::copy_nonoverlapping(src, dst, n as usize);
    }
}

/// Memset
///
/// @param s1 - Slice to be compared
/// @param s2 - Slice to be compared
/// @param n - Number of bytes to compare
#[inline]
pub fn unsafe_memset(s: &mut [u8], c: u8, n: usize) {
    unsafe {
        let s = std::slice::from_raw_parts_mut(s.as_mut_ptr(), n);
        for val in s.iter_mut().take(n) {
            *val = c;
        }
    }
}

/// Memcmp
///
/// @param s1 - Slice to be compared
/// @param s2 - Slice to be compared
/// @param n - Number of bytes to compare
#[inline]
pub fn unsafe_memcmp(s1: &[u8], s2: &[u8], n: usize) -> i32 {
    unsafe {
        let mut result = 0;
        _unsafe_memcmp(s1.as_ptr(), s2.as_ptr(), n, &mut result as *mut i32);
        result
    }
}

/// Memmove
///
/// @param dst - Destination
/// @param src - Source
/// @param n - Number of bytes to copy
///
/// # Safety
#[inline]
pub fn unsafe_memmove(dst: *mut u8, src: *mut u8, n: usize) {
    unsafe {
        std::ptr::copy(src, dst, n as usize);
    }
}

unsafe fn _unsafe_memcmp(s1: *const u8, s2: *const u8, n: usize, result: *mut i32) {
    let mut i = 0;
    while i < n {
        let a = *s1.add(i);
        let b = *s2.add(i);
        if a != b {
            *result = a as i32 - b as i32;
            return;
        }
        i += 1;
    }
    *result = 0
}

/// Check that two regions do not overlap.
#[doc(hidden)]
pub fn is_nonoverlapping<N>(src: N, dst: N, count: N) -> bool
    where
        N: Ord + std::ops::Sub<Output = N>,
        <N as std::ops::Sub>::Output: Ord,
{
    let diff = if src > dst { src - dst } else { dst - src };
    // If the absolute distance between the ptrs is at least as big as the size of the buffer,
    // they do not overlap.
    diff >= count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_nonoverlapping() {
        assert!(is_nonoverlapping(10, 7, 3));
        assert!(!is_nonoverlapping(10, 8, 3));
        assert!(!is_nonoverlapping(10, 9, 3));
        assert!(!is_nonoverlapping(10, 10, 3));
        assert!(!is_nonoverlapping(10, 11, 3));
        assert!(!is_nonoverlapping(10, 12, 3));
        assert!(is_nonoverlapping(10, 13, 3));
    }
}