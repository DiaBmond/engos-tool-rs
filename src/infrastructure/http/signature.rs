use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

/// Verifies LINE's `x-line-signature` header against the raw request body.
///
/// LINE signs the exact bytes it sent. The check must therefore run on the
/// untouched body — deserialising to `serde_json::Value` and re-serialising
/// changes key order and whitespace, which would break verification.
///
/// The comparison is constant-time: a byte-by-byte early return would let an
/// attacker recover a valid signature one byte at a time by timing responses.
pub fn verify_line_signature(channel_secret: &str, body: &[u8], header_signature: &str) -> bool {
    if channel_secret.is_empty() || header_signature.is_empty() {
        return false;
    }

    let Ok(expected) = compute_signature(channel_secret, body) else {
        return false;
    };

    // Compare the decoded digests so that padding or encoding differences in
    // the header cannot cause a false mismatch.
    let Ok(provided) = STANDARD.decode(header_signature) else {
        return false;
    };

    let Ok(expected_bytes) = STANDARD.decode(&expected) else {
        return false;
    };

    if provided.len() != expected_bytes.len() {
        return false;
    }

    provided.ct_eq(&expected_bytes).into()
}

/// Base64-encoded HMAC-SHA256 of `body` under `channel_secret`.
fn compute_signature(channel_secret: &str, body: &[u8]) -> Result<String, ()> {
    let mut mac = HmacSha256::new_from_slice(channel_secret.as_bytes()).map_err(|_| ())?;
    mac.update(body);
    Ok(STANDARD.encode(mac.finalize().into_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test_channel_secret";
    const BODY: &[u8] = br#"{"events":[{"type":"message"}]}"#;

    fn valid_signature() -> String {
        compute_signature(SECRET, BODY).expect("signature")
    }

    #[test]
    fn accepts_a_correctly_signed_body() {
        assert!(verify_line_signature(SECRET, BODY, &valid_signature()));
    }

    #[test]
    fn rejects_a_tampered_body() {
        let sig = valid_signature();
        let tampered = br#"{"events":[{"type":"message","evil":1}]}"#;
        assert!(!verify_line_signature(SECRET, tampered, &sig));
    }

    #[test]
    fn rejects_a_signature_made_with_the_wrong_secret() {
        let sig = compute_signature("attacker_secret", BODY).expect("signature");
        assert!(!verify_line_signature(SECRET, BODY, &sig));
    }

    #[test]
    fn rejects_missing_or_empty_signature() {
        assert!(!verify_line_signature(SECRET, BODY, ""));
    }

    #[test]
    fn rejects_non_base64_signature() {
        assert!(!verify_line_signature(SECRET, BODY, "!!!not-base64!!!"));
    }

    #[test]
    fn rejects_truncated_signature() {
        let sig = valid_signature();
        let truncated = &sig[..sig.len() / 2];
        assert!(!verify_line_signature(SECRET, BODY, truncated));
    }

    /// An empty secret means the deployment is misconfigured; failing closed is
    /// the only safe behaviour.
    #[test]
    fn rejects_everything_when_secret_is_empty() {
        let sig = compute_signature("", BODY).expect("signature");
        assert!(!verify_line_signature("", BODY, &sig));
    }

    #[test]
    fn signature_matches_the_documented_hmac_sha256_base64_form() {
        // Sanity anchor: recomputing by hand must agree with the helper.
        let mut mac = HmacSha256::new_from_slice(SECRET.as_bytes()).unwrap();
        mac.update(BODY);
        let expected = STANDARD.encode(mac.finalize().into_bytes());
        assert_eq!(valid_signature(), expected);
    }

    #[test]
    fn empty_body_is_still_verifiable() {
        let sig = compute_signature(SECRET, b"").expect("signature");
        assert!(verify_line_signature(SECRET, b"", &sig));
    }
}
