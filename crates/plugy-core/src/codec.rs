/// Deserializes a slice of bytes into an instance of `T`.
pub fn deserialize<'a, T>(bytes: &'a [u8]) -> anyhow::Result<T>
where
    T: serde::de::Deserialize<'a>,
{
    Ok(rmp_serde::from_slice(bytes)?)
}

/// Serializes a serializable object into a `Vec` of bytes.
pub fn serialize<T: ?Sized>(value: &T) -> anyhow::Result<Vec<u8>>
where
    T: serde::Serialize,
{
    Ok(rmp_serde::to_vec(value)?)
}
