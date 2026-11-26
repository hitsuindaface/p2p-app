use super::error::CryptoError;
use argon2::{Algorithm, Argon2, Params, Version};
use zeroize::Zeroizing;

pub const SALT_LEN: usize = 16;

/// Spec §8: Argon2id, m = 64 MiB, t = 3, p = 1, 32-byte output.
pub fn derive_db_key(
    passphrase: &[u8],
    salt: &[u8; SALT_LEN],
) -> Result<Zeroizing<[u8; 32]>, CryptoError> {
    let params = Params::new(64 * 1024, 3, 1, Some(32)).map_err(|_| CryptoError::Kdf)?;
    let mut key = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(passphrase, salt, key.as_mut())
        .map_err(|_| CryptoError::Kdf)?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn matches_reference_vector() {
        let salt: [u8; 16] = core::array::from_fn(|i| i as u8); // 00 01 .. 0f
        let key = derive_db_key(b"correct horse battery staple", &salt).unwrap();
        assert_eq!(
            hex(&key[..]),
            "0d1a3c6523c8f06e4e0af9c515aa5b5448cfebd6838f2d52c3d8b6ef8ddc3c2e"
        );
    }
}
