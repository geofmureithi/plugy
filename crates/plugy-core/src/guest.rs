use crate::bitwise::{from_bitwise, into_bitwise};

/// Allocates a buffer of the specified length and returns a pointer to it.
///
/// This function allocates a buffer of `len` bytes using a `Vec<u8>` and returns a
/// mutable pointer to the allocated buffer. The `Vec` is created with a capacity of
/// `len`, and its ownership is immediately transferred to the caller through the
/// returned pointer. The allocated buffer must be deallocated using the `dealloc`
/// function to prevent memory leaks.
///
/// # Arguments
///
/// * `len` - The length of the buffer to allocate, in bytes.
///
/// # Returns
///
/// A mutable pointer to the allocated buffer of the specified length.
///
/// # Safety
///
/// This function is marked as `unsafe` because it returns a raw pointer to memory,
/// and the caller is responsible for ensuring the proper deallocation of the buffer
/// to avoid memory leaks.
///
/// # Examples
///
/// ```no_run
/// use plugy_core::guest::dealloc;
/// use plugy_core::guest::alloc;
/// let len: u32 = 1024;
/// let buffer_ptr = alloc(len);
/// // Use the allocated buffer...
/// // Remember to deallocate the buffer when it's no longer needed.
/// unsafe { dealloc(buffer_ptr as u64) };
/// ```
#[inline]
#[no_mangle]
pub extern "C" fn alloc(len: u32) -> *mut u8 {
    let mut buf = Vec::with_capacity(len as _);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// Deallocates a buffer previously allocated by the `alloc` function.
///
/// This function takes a value `value` obtained from a previous call to the `alloc`
/// function. The value is expected to be a combined representation of a pointer and
/// a length, obtained using the `into_bitwise` function. The function properly
/// deallocates the buffer and frees the associated memory.
///
/// # Arguments
///
/// * `value` - The value representing the pointer and length of the buffer to
///            deallocate, obtained from the `alloc` function.
///
/// # Safety
///
/// This function is marked as `unsafe` because it performs a deallocation of memory.
/// The `value` parameter must be a valid representation obtained from the `alloc`
/// function, and improper usage can lead to memory corruption.
///
/// # Examples
///
/// ```no_run
/// use plugy_core::guest::dealloc;
/// use plugy_core::guest::alloc;
/// let len: u32 = 1024;
/// let buffer_ptr = alloc(len);
/// // Use the allocated buffer...
/// unsafe { dealloc(buffer_ptr as u64) };
/// ```
#[inline]
#[no_mangle]
pub unsafe extern "C" fn dealloc(value: u64) {
    let (ptr, len) = from_bitwise(value);
    #[allow(clippy::useless_transmute)]
    let ptr = std::mem::transmute::<usize, *mut u8>(ptr as _);
    let buffer = Vec::from_raw_parts(ptr, len as _, len as _);
    std::mem::drop(buffer);
}

/// Serializes a value using bincode and returns a combined representation.
///
/// This function serializes a value implementing the `serde::ser::Serialize` trait
/// using the bincode serialization format. The serialized data is stored in a `Vec<u8>`
/// buffer, and a combined representation of the buffer's pointer and length is
/// obtained using the `into_bitwise` function. The ownership of the buffer is
/// transferred to the caller, who is responsible for deallocating it using the
/// `dealloc` function.
///
/// # Arguments
///
/// * `value` - A reference to the value to be serialized.
///
/// # Returns
///
/// A combined representation of the serialized buffer's pointer and length.
///
/// # Examples
///
/// ```
/// use plugy_core::guest::dealloc;
/// use plugy_core::guest::write_msg;
/// #[derive(serde::Serialize)]
/// struct MyStruct {
///     // Fields of MyStruct...
/// }
///
/// let my_instance = MyStruct { /* initialize fields */ };
/// let combined = write_msg(&my_instance);
/// // Deallocate the buffer when no longer needed.
/// unsafe { dealloc(combined) };
/// ```
pub fn write_msg<T: serde::ser::Serialize>(value: &T) -> u64 {
    let mut buffer = bincode::serialize(value).expect("could not serialize");
    let len = buffer.len();
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    into_bitwise(ptr as _, len as _)
}

/// Deserializes a value using bincode from a combined representation.
///
/// This function takes a combined representation obtained from the `write_msg`
/// function, which includes a pointer and length of a serialized buffer. The
/// function safely deserializes the buffer back into a value implementing the
/// `serde::de::DeserializeOwned` trait and returns it. The ownership of the buffer
/// is transferred to the function, which takes care of proper deallocation.
///
/// # Arguments
///
/// * `value` - The combined representation of the serialized buffer's pointer and
///            length, obtained from the `write_msg` function.
///
/// # Returns
///
/// The deserialized value of type `T`.
///
/// # Safety
///
/// This function is marked as `unsafe` because it involves working with raw pointers
/// and memory management. The provided `value` parameter must be a valid combined
/// representation obtained from the `write_msg` function, and incorrect usage can lead
/// to memory corruption or other issues.
///
/// # Examples
///
/// ```no_run
/// use plugy_core::guest::read_msg;
/// #[derive(serde::Deserialize)]
/// struct MyStruct {
///     // Fields of MyStruct...
/// }
///
/// let combined: u64 = 0;/* ptr on the host side */;
/// let my_instance: MyStruct = unsafe { read_msg(combined) };
/// ```
pub unsafe fn read_msg<T: serde::de::DeserializeOwned>(value: u64) -> T {
    let (ptr, len) = from_bitwise(value);
    #[allow(clippy::useless_transmute)]
    let ptr = std::mem::transmute::<usize, *mut u8>(ptr as _);
    let buffer = Vec::from_raw_parts(ptr, len as _, len as _);
    bincode::deserialize(&buffer).expect("invalid bytes provided")
}
