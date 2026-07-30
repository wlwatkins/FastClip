//! DPAPI — the outer layer, and the factor that binds the store to one Windows
//! account.
//!
//! `CryptProtectData` and `CryptUnprotectData`
//! ([ADR-0004](../../../docs/src/architecture/adr/0004-optional-pin-encryption.md)).
//! This is an operating-system call, not cryptography written here: the key is
//! derived and held by Windows and never exists in this process.
//!
//! **What it buys.** Without it, the PIN alone would guard the DEK and an
//! attacker holding `keyfile` could try all 10⁶ PINs offline. With it, the blob
//! cannot be unprotected at all except by the same Windows user on the same
//! machine — which is what makes the PIN's small space irrelevant to someone who
//! copied the file.
//!
//! **What it costs, and it is deliberate.** The store cannot be moved to another
//! machine or account. Export is the way to move clips
//! ([ADR-0004](../../../docs/src/architecture/adr/0004-optional-pin-encryption.md)).
//!
//! **No optional entropy — `None`** (`storage.md` § DPAPI entropy, and the
//! blob's encoding). Permanent: changing it makes every existing store
//! unopenable, and there is no migration path for key material because the store
//! cannot be read to rewrite it without the key the change just invalidated.
//!
//! **This module is Windows-only**, as the whole application is.

use std::ffi::c_void;

use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB,
};

use crate::error::{ClipError, CryptoReason};

/// A blob Windows allocated, freed on drop.
///
/// `CryptProtectData` returns memory from `LocalAlloc`, and the caller frees it
/// with `LocalFree`. Wrapping it means the free happens on every path — including
/// the early return when the copy out fails — rather than at one place a later
/// edit could step around.
struct OwnedBlob(CRYPT_INTEGER_BLOB);

impl OwnedBlob {
    /// Copy the contents out. The copy is this crate's to zeroise.
    fn to_vec(&self) -> Vec<u8> {
        if self.0.pbData.is_null() || self.0.cbData == 0 {
            return Vec::new();
        }
        // SAFETY: Windows set `pbData` and `cbData` together on a successful
        // return, and the pointer is valid until the `LocalFree` in `Drop`.
        unsafe { std::slice::from_raw_parts(self.0.pbData, self.0.cbData as usize) }.to_vec()
    }
}

impl Drop for OwnedBlob {
    fn drop(&mut self) {
        if !self.0.pbData.is_null() {
            // SAFETY: the pointer came from `CryptProtectData` or
            // `CryptUnprotectData`, both of which allocate with `LocalAlloc`.
            unsafe {
                LocalFree(self.0.pbData as *mut c_void);
            }
            self.0.pbData = std::ptr::null_mut();
        }
    }
}

/// Build the input descriptor Windows expects.
///
/// The pointer is cast from a shared slice, and Windows treats `pDataIn` as
/// read-only, so this does not alias mutably.
fn blob_of(bytes: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    }
}

/// Encrypt for the current Windows user.
pub fn protect(plaintext: &[u8]) -> Result<Vec<u8>, ClipError> {
    let input = blob_of(plaintext);
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };

    // SAFETY: `input` points at `plaintext` for the duration of the call, every
    // optional parameter is null, and `output` is owned below whatever happens.
    let ok = unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),     // no description
            std::ptr::null(),     // **no optional entropy** — see the header
            std::ptr::null_mut(), // reserved
            std::ptr::null(),     // no prompt
            0,                    // no flags: bind to the user, not the machine
            &mut output,
        )
    };
    let output = OwnedBlob(output);

    if ok == 0 {
        // **The OS error code and nothing else.** It describes why Windows
        // refused, and it cannot carry the plaintext — which here is the whole
        // key material blob.
        log::error!(
            "the key material could not be protected by DPAPI (os error {})",
            std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
        );
        return Err(ClipError::Crypto {
            reason: CryptoReason::BadKeyMaterial,
        });
    }

    let protected = output.to_vec();
    if protected.is_empty() {
        log::error!("DPAPI reported success and returned nothing");
        return Err(ClipError::Crypto {
            reason: CryptoReason::BadKeyMaterial,
        });
    }
    Ok(protected)
}

/// Decrypt a blob this Windows user protected.
///
/// **Failure is `crypto { bad_key_material }`, never `bad_pin`.** It means the
/// file is missing, truncated, or from another Windows account or machine —
/// a different fact from a wrong PIN, with a different remedy and a different
/// sentence in the copy deck (contract §4).
pub fn unprotect(protected: &[u8]) -> Result<Vec<u8>, ClipError> {
    let input = blob_of(protected);
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };

    // SAFETY: as `protect`.
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(), // no description wanted back
            std::ptr::null(),     // no optional entropy — must match `protect`
            std::ptr::null_mut(), // reserved
            std::ptr::null(),     // no prompt
            0,
            &mut output,
        )
    };
    let output = OwnedBlob(output);

    if ok == 0 {
        log::error!(
            "the key material could not be unprotected by DPAPI; it is missing, \
             damaged, or from another Windows account or machine (os error {})",
            std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
        );
        return Err(ClipError::Crypto {
            reason: CryptoReason::BadKeyMaterial,
        });
    }

    Ok(output.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_protected_blob_round_trips_for_this_account() {
        let secret = b"the wrapped data encryption key and its parameters";
        let protected = match protect(secret) {
            Ok(protected) => protected,
            Err(e) => panic!("DPAPI should protect: {e}"),
        };

        assert_ne!(
            protected.as_slice(),
            secret.as_slice(),
            "the blob must not be the plaintext"
        );
        assert!(
            protected
                .windows(secret.len())
                .all(|window| window != secret.as_slice()),
            "the plaintext must not appear inside the protected blob"
        );

        match unprotect(&protected) {
            Ok(recovered) => assert_eq!(recovered.as_slice(), secret.as_slice()),
            Err(e) => panic!("DPAPI should unprotect what it protected: {e}"),
        }
    }

    /// **The factor is real, not decorative.** A blob DPAPI did not produce, or
    /// one that has been altered, does not unprotect — which is what stops the
    /// wrapped DEK being read on another machine.
    #[test]
    fn a_blob_this_account_did_not_protect_is_bad_key_material() {
        let bad_key_material = Err(ClipError::Crypto {
            reason: CryptoReason::BadKeyMaterial,
        });

        assert_eq!(unprotect(b"not a DPAPI blob at all"), bad_key_material);
        assert_eq!(unprotect(&[]), bad_key_material);
        assert_eq!(unprotect(&[0u8; 64]), bad_key_material);

        // A real blob with one bit flipped: DPAPI authenticates its own
        // ciphertext, so this is refused rather than returning rubbish.
        let mut protected = match protect(b"the wrapped key") {
            Ok(protected) => protected,
            Err(e) => panic!("DPAPI should protect: {e}"),
        };
        let last = protected.len() - 1;
        protected[last] ^= 0x01;
        assert_eq!(unprotect(&protected), bad_key_material);
    }

    /// Every call allocates a Windows blob and frees it in `Drop`. A leak would
    /// not fail a test, so this at least exercises the path enough that a
    /// double-free or a use-after-free would surface.
    #[test]
    fn repeated_round_trips_do_not_fault() {
        for n in 0..32u8 {
            let secret = vec![n; 64];
            let protected = match protect(&secret) {
                Ok(protected) => protected,
                Err(e) => panic!("DPAPI should protect: {e}"),
            };
            match unprotect(&protected) {
                Ok(recovered) => assert_eq!(recovered, secret),
                Err(e) => panic!("DPAPI should unprotect: {e}"),
            }
        }
    }

    #[test]
    fn an_empty_plaintext_round_trips_rather_than_faulting() {
        let protected = match protect(&[]) {
            Ok(protected) => protected,
            Err(e) => panic!("DPAPI should protect an empty input: {e}"),
        };
        match unprotect(&protected) {
            Ok(recovered) => assert!(recovered.is_empty()),
            Err(e) => panic!("DPAPI should unprotect an empty input: {e}"),
        }
    }
}
