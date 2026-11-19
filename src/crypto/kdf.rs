use argon2::{Algorithm, Argon2, Params, Version};
use zeroize::Zeroizing;

pub const SALT_LEN: usize = 16;

/// Spec §8: Argon2id, m = 64 MiB, t = 3, p = 1, 32-byte output.
pub fn derive_db_key(passphrase: &[u8], salt: &[u8; SALT_LEN])
    -> Result<Zeroizing<[u8; 32]>, argon2::Error>
{
    let params = Params::new(64 * 1024, 3, 1, Some(32))?;
    let mut key = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(passphrase, salt, key.as_mut())?;
    Ok(key)
}