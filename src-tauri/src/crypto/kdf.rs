//! Argon2id over the PIN.
//!
//! `storage.md` § Argon2id parameters and the backoff. **This does not produce
//! the store's key.** It produces the key that *unwraps* the store's key, and the
//! distinction is the whole of [ADR-0004](../../../docs/src/architecture/adr/0004-optional-pin-encryption.md):
//! a 6-digit PIN has 10⁶ values, so if it derived the store key directly an
//! attacker holding the file could try all of them offline. Here it only gates a
//! wrap around 256 bits of real entropy, and the KDF's cost is what makes each of
//! those 10⁶ guesses expensive for someone who already has the DPAPI blob.
//!
//! **The parameters live in `keyfile`, not in this binary.** That is what lets
//! the cost be raised later without making every existing store unopenable, and
//! it is the reason they are written down at all. It cuts one way only — see
//! [`KdfParams::validate`].

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::crypto::fill_random;
use crate::error::{ClipError, CryptoReason};

/// The salt's length in bytes (`storage.md`: a 16-byte random salt).
pub const SALT_LEN: usize = 16;

/// The wrapping key's length. XChaCha20-Poly1305 takes 256 bits.
pub const WRAP_KEY_LEN: usize = 32;

/// OWASP's minimum for Argon2id, and this build's binding floor.
pub const MIN_M_COST_KIB: u32 = 19 * 1024;
/// OWASP's minimum iteration count.
pub const MIN_T_COST: u32 = 2;
/// Fixed by `storage.md`. Not a range.
pub const P_COST: u32 = 1;

/// A ceiling, so that a tampered `keyfile` cannot ask this process to allocate
/// its way out of memory or spin for hours.
///
/// Editing the blob needs the Windows account, which
/// [ADR-0002](../../../docs/src/architecture/adr/0002-threat-model.md) already
/// concedes — so this is not a security boundary. It stops a corrupted or
/// hand-edited file turning into a hang the user cannot diagnose, which is a
/// different and more likely failure than an attack.
const MAX_M_COST_KIB: u32 = 1024 * 1024;
/// Likewise. At the floor's memory cost this is already minutes per unwrap.
const MAX_T_COST: u32 = 64;

/// The tuple this build writes into a **new** `keyfile`: 128 MiB, 3 passes.
///
/// **Measured, not assumed** — `storage.md` requires 250–500 ms per unwrap and
/// requires the figure to have been taken. On the development machine, in
/// `--release`, this is a mean of **266 ms** and a worst of **287 ms** over five
/// runs. Reproduce it with
/// [`tests::measure_the_cost_of_one_unwrap`], which sweeps the neighbouring
/// tuples so the choice is visible as a choice.
///
/// **Why the low end of the band rather than the middle.** 192 MiB with 3 passes
/// also lands in band, at 395 ms here, and is the more memory-hard option. It was
/// not chosen, for two reasons that point the same way:
///
/// - **The cost is asymmetric and permanent for a given store.** A store carries
///   the tuple it was created with, so raising these constants affects only new
///   stores and can never be undone for an existing one short of `change_pin`
///   rewriting the file. A machine slower than this one pays the difference on
///   every unlock, forever. 250–500 ms *here* is several seconds on a machine
///   four times slower, and the user cannot opt out.
/// - [ADR-0011](../../../docs/src/architecture/adr/0011-flat-backoff.md) says in
///   as many words that the parameters live in the file "so the cost can be
///   raised later". The design anticipates raising, not lowering.
///
/// The transient allocation is also halved — 128 MiB rather than 192 MiB per
/// unwrap, on a tray utility that is otherwise small.
///
/// **This is not a tuning knob.** Changing it changes what new stores cost to
/// open, forever, on hardware nobody here has measured.
pub const DEFAULT_M_COST_KIB: u32 = 128 * 1024;
/// See [`DEFAULT_M_COST_KIB`].
pub const DEFAULT_T_COST: u32 = 3;

/// The tuple this build writes must itself pass the floor every reader enforces,
/// or `enable_encryption` would create a store the next launch refuses to open.
///
/// Checked at compile time, so a tuning edit that dropped below OWASP's minimum
/// cannot reach a test run, let alone a user's key material.
const _: () = assert!(
    DEFAULT_M_COST_KIB >= MIN_M_COST_KIB
        && DEFAULT_M_COST_KIB <= MAX_M_COST_KIB
        && DEFAULT_T_COST >= MIN_T_COST
        && DEFAULT_T_COST <= MAX_T_COST,
    "the shipped Argon2id tuple must lie within the bounds KdfParams::validate enforces"
);

/// The KDF settings a particular `keyfile` was written with.
///
/// **No `deny_unknown_fields`.** This is a migration path: a `keyfile` written by
/// a later build may carry fields this one does not know, and refusing to read it
/// would make the store unopenable rather than merely un-upgraded
/// (`storage.md` § DPAPI entropy, and the blob's encoding).
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    pub m_cost_kib: u32,
    pub t_cost: u32,
    pub p_cost: u32,
    pub salt: [u8; SALT_LEN],
}

/// The salt is not a secret in the cryptographic sense, but it is key material
/// and criterion 6 names it. Hand-written so a `{:?}` cannot print it.
impl std::fmt::Debug for KdfParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KdfParams")
            .field("m_cost_kib", &self.m_cost_kib)
            .field("t_cost", &self.t_cost)
            .field("p_cost", &self.p_cost)
            .field("salt", &"<redacted>")
            .finish()
    }
}

impl KdfParams {
    /// Fresh parameters at this build's cost, with a new random salt.
    pub fn generate() -> Result<Self, ClipError> {
        let mut salt = [0u8; SALT_LEN];
        match fill_random(&mut salt) {
            Ok(()) => {}
            Err(error) => {
                salt.zeroize();
                return Err(error);
            }
        }

        Ok(Self {
            m_cost_kib: DEFAULT_M_COST_KIB,
            t_cost: DEFAULT_T_COST,
            p_cost: P_COST,
            salt,
        })
    }

    /// Reject parameters this build will not run.
    ///
    /// **Below the floor is rejected rather than raised.** Substituting a
    /// different cost would derive a different key and surface as a wrong PIN —
    /// the user would be told their PIN is wrong when the file was the problem.
    /// The migration rule that an unmappable value falls back to a default does
    /// not apply to a parameter that *is* part of the key derivation.
    ///
    /// No build has ever written a tuple below the floor, so this cannot reject a
    /// genuine store.
    pub fn validate(&self) -> Result<(), ClipError> {
        let within_bounds = self.m_cost_kib >= MIN_M_COST_KIB
            && self.m_cost_kib <= MAX_M_COST_KIB
            && self.t_cost >= MIN_T_COST
            && self.t_cost <= MAX_T_COST
            && self.p_cost == P_COST;

        if within_bounds {
            return Ok(());
        }

        // The parameters are not secret, but the salt in the same struct is, so
        // the three costs are named individually rather than through `{self:?}`.
        log::error!(
            "the key material carries Argon2id parameters this build will not run \
             (m_cost_kib {}, t_cost {}, p_cost {})",
            self.m_cost_kib,
            self.t_cost,
            self.p_cost
        );
        Err(ClipError::Crypto {
            reason: CryptoReason::BadKeyMaterial,
        })
    }
}

/// The key that wraps the DEK. Never the store's key.
#[derive(Clone, ZeroizeOnDrop)]
pub struct WrapKey([u8; WRAP_KEY_LEN]);

impl WrapKey {
    pub fn expose(&self) -> &[u8; WRAP_KEY_LEN] {
        &self.0
    }
}

impl std::fmt::Debug for WrapKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WrapKey(<redacted>)")
    }
}

/// Derive the wrapping key from a PIN and the parameters that store was written
/// with.
///
/// The memory blocks are allocated here rather than by argon2, because this crate
/// builds argon2 without its `alloc` feature — that feature is what pulls in the
/// PHC-string parser, which nothing here uses (see `Cargo.toml`).
/// `hash_password_into_with_memory` checks the buffer's length itself, so a
/// wrong size is an error rather than a silent short hash.
pub fn derive(pin: &crate::crypto::Pin, params: &KdfParams) -> Result<WrapKey, ClipError> {
    use argon2::{Algorithm, Argon2, Block, Params, Version};

    params.validate()?;

    let configured = Params::new(
        params.m_cost_kib,
        params.t_cost,
        params.p_cost,
        Some(WRAP_KEY_LEN),
    );
    let configured = match configured {
        Ok(configured) => configured,
        Err(error) => {
            // argon2's own range validation. The error names a parameter, never
            // the PIN or the salt.
            log::error!("the Argon2id parameters were rejected by the KDF: {error}");
            return Err(ClipError::Crypto {
                reason: CryptoReason::BadKeyMaterial,
            });
        }
    };

    let mut blocks = vec![Block::default(); configured.block_count()];
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, configured);

    let mut derived = [0u8; WRAP_KEY_LEN];
    let outcome = argon2.hash_password_into_with_memory(
        pin.expose(),
        &params.salt,
        &mut derived,
        &mut blocks,
    );

    // The blocks hold intermediate state derived from the PIN. argon2's
    // `zeroize` feature wipes its own internals, and this wipes the buffer this
    // function owns.
    blocks.zeroize();

    match outcome {
        Ok(()) => Ok(WrapKey(derived)),
        Err(error) => {
            derived.zeroize();
            // argon2's `Error` is a fixed enum of parameter faults. It carries
            // no input, but it is logged as a discriminant rather than
            // interpolated with anything else.
            log::error!("the key could not be derived from the PIN: {error}");
            Err(ClipError::Crypto {
                reason: CryptoReason::BadKeyMaterial,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::Pin;

    /// The floor, for tests that do not care about the cost. Running the shipped
    /// tuple in every test would add seconds to the suite for nothing.
    fn cheap(salt: [u8; SALT_LEN]) -> KdfParams {
        KdfParams {
            m_cost_kib: MIN_M_COST_KIB,
            t_cost: MIN_T_COST,
            p_cost: P_COST,
            salt,
        }
    }

    fn pin(value: &str) -> Pin {
        match Pin::parse("pin", Some(value.to_owned())) {
            Ok(pin) => pin,
            Err(e) => panic!("the fixture PIN should parse: {e}"),
        }
    }

    fn derive_or_panic(value: &str, params: &KdfParams) -> WrapKey {
        match derive(&pin(value), params) {
            Ok(key) => key,
            Err(e) => panic!("the key should derive: {e}"),
        }
    }

    #[test]
    fn the_same_pin_and_salt_derive_the_same_key() {
        let params = cheap([0x11; SALT_LEN]);
        let first = derive_or_panic("123456", &params);
        let second = derive_or_panic("123456", &params);
        assert_eq!(first.expose(), second.expose());
    }

    #[test]
    fn a_different_pin_derives_a_different_key() {
        let params = cheap([0x11; SALT_LEN]);
        assert_ne!(
            derive_or_panic("123456", &params).expose(),
            derive_or_panic("123457", &params).expose()
        );
    }

    /// **The salt is why two users with the same PIN do not share a wrap key**,
    /// and why a precomputed table over 10⁶ PINs is worthless.
    #[test]
    fn a_different_salt_derives_a_different_key_from_the_same_pin() {
        assert_ne!(
            derive_or_panic("123456", &cheap([0x11; SALT_LEN])).expose(),
            derive_or_panic("123456", &cheap([0x22; SALT_LEN])).expose()
        );
    }

    #[test]
    fn generated_parameters_meet_the_floor_and_carry_a_random_salt() {
        let first = match KdfParams::generate() {
            Ok(params) => params,
            Err(e) => panic!("parameters should generate: {e}"),
        };
        let second = match KdfParams::generate() {
            Ok(params) => params,
            Err(e) => panic!("parameters should generate: {e}"),
        };

        assert!(first.validate().is_ok());
        assert!(first.m_cost_kib >= MIN_M_COST_KIB);
        assert!(first.t_cost >= MIN_T_COST);
        assert_eq!(first.p_cost, P_COST);
        assert_ne!(
            first.salt, [0u8; SALT_LEN],
            "a salt of zeroes is not a salt"
        );
        assert_ne!(first.salt, second.salt, "the salt must be per-store");
    }

    /// The shipped tuple is checked at compile time beside its declaration; this
    /// checks the other half — that a real `KdfParams` built from it passes the
    /// same `validate` every reader calls.
    #[test]
    fn parameters_generated_from_the_shipped_tuple_validate() {
        let params = match KdfParams::generate() {
            Ok(params) => params,
            Err(e) => panic!("parameters should generate: {e}"),
        };
        assert_eq!(params.m_cost_kib, DEFAULT_M_COST_KIB);
        assert_eq!(params.t_cost, DEFAULT_T_COST);
        assert_eq!(params.validate(), Ok(()));
    }

    /// Below the floor is refused rather than quietly raised: raising it would
    /// derive a different key and be reported to the user as a wrong PIN.
    #[test]
    fn parameters_below_the_floor_are_bad_key_material() {
        let bad = [
            (MIN_M_COST_KIB - 1, MIN_T_COST, P_COST),
            (MIN_M_COST_KIB, MIN_T_COST - 1, P_COST),
            (MIN_M_COST_KIB, MIN_T_COST, P_COST + 1),
            (0, 0, 0),
        ];
        for (m_cost_kib, t_cost, p_cost) in bad {
            let params = KdfParams {
                m_cost_kib,
                t_cost,
                p_cost,
                salt: [0x11; SALT_LEN],
            };
            assert_eq!(
                params.validate(),
                Err(ClipError::Crypto {
                    reason: CryptoReason::BadKeyMaterial
                }),
                "for ({m_cost_kib}, {t_cost}, {p_cost})"
            );
        }
    }

    /// A tampered or corrupted blob must not be able to make this process
    /// allocate a terabyte or iterate for hours.
    #[test]
    fn absurd_parameters_are_refused_rather_than_attempted() {
        let params = KdfParams {
            m_cost_kib: u32::MAX,
            t_cost: u32::MAX,
            p_cost: P_COST,
            salt: [0x11; SALT_LEN],
        };
        assert_eq!(
            params.validate(),
            Err(ClipError::Crypto {
                reason: CryptoReason::BadKeyMaterial
            })
        );
        // And `derive` refuses before allocating anything.
        assert!(derive(&pin("123456"), &params).is_err());
    }

    #[test]
    fn the_debug_of_the_parameters_redacts_the_salt_and_the_debug_of_a_key_is_empty() {
        let params = cheap([0xAB; SALT_LEN]);
        let rendered = format!("{params:?}");
        assert!(rendered.contains("redacted"), "{rendered}");
        assert!(!rendered.contains("171"), "{rendered}");
        assert!(rendered.contains("m_cost_kib"), "the costs are not secret");

        let key = derive_or_panic("123456", &params);
        let rendered = format!("{key:?}");
        assert!(rendered.contains("redacted"), "{rendered}");
    }

    /// **The tuning instrument.** `storage.md` requires one unwrap to take
    /// 250–500 ms and requires the figure to have been measured; this is what
    /// measures it, so the number in the implementation report can be
    /// reproduced rather than trusted.
    ///
    /// ```text
    /// cargo test --release --target x86_64-pc-windows-msvc \
    ///     --lib crypto::kdf::tests::measure -- --ignored --nocapture
    /// ```
    ///
    /// **`--release` is not optional.** Argon2id is arithmetic over tens of
    /// megabytes; an unoptimised build is several times slower, and a tuple
    /// tuned against it would land far below the target in the binary that
    /// ships.
    ///
    /// `#[ignore]` because it is a measurement, not an assertion. Asserting a
    /// duration would make the suite fail on a loaded or slower machine, which
    /// is a flaky test rather than a security check.
    #[test]
    #[ignore = "a measurement, not an assertion; run with --release --ignored"]
    fn measure_the_cost_of_one_unwrap() {
        /// The shipped tuple first, then the neighbours either side of it, so
        /// the choice is visibly a choice rather than the first thing tried.
        const CANDIDATES: &[(u32, u32)] = &[
            (DEFAULT_M_COST_KIB, DEFAULT_T_COST),
            (MIN_M_COST_KIB, MIN_T_COST),
            (47 * 1024, 2),
            (64 * 1024, 3),
            (128 * 1024, 2),
            (128 * 1024, 3),
            (192 * 1024, 3),
        ];

        for (m_cost_kib, t_cost) in CANDIDATES {
            let params = KdfParams {
                m_cost_kib: *m_cost_kib,
                t_cost: *t_cost,
                p_cost: P_COST,
                salt: [0x11; SALT_LEN],
            };

            // One untimed run first: the first allocation of the block buffer
            // pays for page faults the later ones do not.
            let _ = derive_or_panic("123456", &params);

            let mut worst = std::time::Duration::ZERO;
            let mut total = std::time::Duration::ZERO;
            let runs = 5;
            for _ in 0..runs {
                let started = std::time::Instant::now();
                let _ = derive_or_panic("123456", &params);
                let elapsed = started.elapsed();
                worst = worst.max(elapsed);
                total += elapsed;
            }

            let shipped = *m_cost_kib == DEFAULT_M_COST_KIB && *t_cost == DEFAULT_T_COST;
            println!(
                "argon2id m={:>7} KiB ({:>4} MiB) t={} p={} -> mean {:>9.2?} worst {:>9.2?}{}",
                m_cost_kib,
                m_cost_kib / 1024,
                t_cost,
                P_COST,
                total / runs,
                worst,
                if shipped { "   <- shipped" } else { "" }
            );
        }
        println!("target: 250-500 ms per unwrap. Debug builds are several times slower.");
    }

    /// The parameters round-trip through the encoding the blob uses, and an
    /// unknown field from a later build is ignored rather than fatal
    /// (migrations are forgiving on input).
    #[test]
    fn parameters_round_trip_through_json_and_tolerate_an_unknown_field() {
        let params = cheap([0x33; SALT_LEN]);
        let json = match serde_json::to_string(&params) {
            Ok(json) => json,
            Err(e) => panic!("the parameters should serialise: {e}"),
        };
        match serde_json::from_str::<KdfParams>(&json) {
            Ok(back) => assert_eq!(back, params),
            Err(e) => panic!("the parameters should deserialise: {e}"),
        }

        let from_a_later_build = format!(
            r#"{{"m_cost_kib":{},"t_cost":{},"p_cost":{},"salt":{:?},"kdf_variant":"argon2id"}}"#,
            params.m_cost_kib, params.t_cost, params.p_cost, params.salt
        );
        match serde_json::from_str::<KdfParams>(&from_a_later_build) {
            Ok(back) => assert_eq!(back, params, "an unknown field must be ignored, not fatal"),
            Err(e) => panic!("a later build's key material must still read: {e}"),
        }
    }
}
