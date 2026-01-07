/// Converts a 64-bit value into a pair of 32-bit values using bitwise operations.
///
/// This function takes a 64-bit unsigned integer `value` and extracts two 32-bit
/// unsigned integers from it using bitwise operations. The lower 32 bits of the
/// input value are extracted as the first 32-bit value, and the upper 32 bits are
/// extracted as the second 32-bit value.
///
/// # Arguments
///
/// * `value` - The 64-bit value from which to extract the 32-bit values.
///
/// # Returns
///
/// A tuple containing two 32-bit unsigned integers. The first element of the tuple
/// is the lower 32 bits of the input value, and the second element is the upper 32
/// bits of the input value.
///
/// # Examples
///
/// ```
/// use plugy_core::bitwise::from_bitwise;
/// let value: u64 = 0x0000_1234_5678_9ABC;
/// let (lower, upper) = from_bitwise(value);
/// assert_eq!(lower, 0x5678_9ABC);
/// assert_eq!(upper, 0x0000_1234);
/// ```
#[inline(always)]
pub const fn from_bitwise(value: u64) -> (u32, u32) {
    ((value << 32 >> 32) as u32, (value >> 32) as u32)
}

/// Combines two 32-bit values into a single 64-bit value using bitwise operations.
///
/// This function takes two 32-bit unsigned integers, `a` and `b`, and combines them
/// into a single 64-bit unsigned integer using bitwise operations. The value `a` is
/// stored in the lower 32 bits of the result, and the value `b` is stored in the upper
/// 32 bits.
///
/// # Arguments
///
/// * `a` - The lower 32 bits of the resulting 64-bit value.
/// * `b` - The upper 32 bits of the resulting 64-bit value.
///
/// # Returns
///
/// A 64-bit unsigned integer obtained by combining the input values `a` and `b`
/// using bitwise OR and left shift operations.
///
/// # Examples
///
/// ```
/// use plugy_core::bitwise::into_bitwise;
/// let a: u32 = 0x5678_9ABC;
/// let b: u32 = 0x0000_1234;
/// let combined = into_bitwise(a, b);
/// assert_eq!(combined, 0x0000_1234_5678_9ABC);
/// ```
#[inline(always)]
pub const fn into_bitwise(a: u32, b: u32) -> u64 {
    (a as u64) | (b as u64) << 32
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn bitwise() {
        const DATA: (u32, u32) = (10, 20);
        const INTO: u64 = into_bitwise(DATA.0, DATA.1);
        const FROM: (u32, u32) = from_bitwise(INTO);
        assert_eq!(DATA, FROM)
    }
}
