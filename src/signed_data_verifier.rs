use base64::{DecodeError, Engine};
use base64::engine::general_purpose::STANDARD;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::de::DeserializeOwned;
use crate::chain_verifier::{ChainVerifierError, verify_chain};
use crate::primitives::environment::Environment;
use crate::primitives::jws_transaction_decoded_payload::JWSTransactionDecodedPayload;
use crate::primitives::response_body_v2_decoded_payload::ResponseBodyV2DecodedPayload;
use crate::utils::StringExt;

#[derive(thiserror::Error, Debug)]
pub enum SignedDataVerifierError {
    #[error("VerificationFailure")]
    VerificationFailure,

    #[error("InvalidAppIdentifier")]
    InvalidAppIdentifier,

    #[error("InvalidEnvironment")]
    InvalidEnvironment,

    #[error("InternalChainVerifierError")]
    InternalChainVerifierError(#[from] ChainVerifierError),

    #[error("InternalDecodeError: [{0}]")]
    InternalDecodeError(#[from] base64::DecodeError)
}

pub struct SignedDataVerifier {
    root_certificates: Vec<Vec<u8>>,
    environment: Environment,
    bundle_id: String,
    app_apple_id: Option<i64>,
}

impl SignedDataVerifier {
    pub fn new(root_certificates: Vec<Vec<u8>>,
           environment: Environment,
           bundle_id: String,
           app_apple_id: Option<i64>,
    ) -> Self {
        return SignedDataVerifier {
            root_certificates,
            environment,
            bundle_id,
            app_apple_id,
        };
    }
}

impl SignedDataVerifier {
    pub fn verify_and_decode_signed_transaction(&self, signed_transaction: &str) -> Result<JWSTransactionDecodedPayload, SignedDataVerifierError> {
        let decoded_signed_tx: JWSTransactionDecodedPayload  = self.decode_signed_object(signed_transaction)?;

        if decoded_signed_tx.bundle_id.as_ref() != Some(&self.bundle_id) {
            return Err(SignedDataVerifierError::InvalidAppIdentifier)
        }

        if decoded_signed_tx.environment.as_ref() != Some(&self.environment) {
            return Err(SignedDataVerifierError::InvalidEnvironment)
        }

        Ok(decoded_signed_tx)
    }
    pub fn verify_and_decode_notification(&self, signed_payload: &str) -> Result<ResponseBodyV2DecodedPayload, SignedDataVerifierError> {
        let decoded_signed_notification: ResponseBodyV2DecodedPayload  = self.decode_signed_object(signed_payload)?;

        let bundle_id;
        let app_apple_id;
        let environment;

        if let Some(data) = &decoded_signed_notification.data {
            bundle_id = data.bundle_id.clone();
            app_apple_id = data.app_apple_id.clone();
            environment = data.environment.clone();
        } else if let Some(summary) = &decoded_signed_notification.summary {
            bundle_id = summary.bundle_id.clone();
            app_apple_id = summary.app_apple_id.clone();
            environment = summary.environment.clone();
        } else {
            return Err(SignedDataVerifierError::InvalidAppIdentifier)
        }

        if bundle_id.as_ref() != Some(&self.bundle_id) || (self.environment == Environment::Production && app_apple_id.as_ref() != self.app_apple_id.as_ref() ) {
            return Err(SignedDataVerifierError::InvalidAppIdentifier)
        }

        if environment.as_ref() != Some(&self.environment) {
            return Err(SignedDataVerifierError::InvalidEnvironment)
        }

        Ok(decoded_signed_notification)
    }

    fn decode_signed_object<T: DeserializeOwned>(&self, signed_obj: &str) -> Result<T, SignedDataVerifierError> {
        let header = jsonwebtoken::decode_header(signed_obj).expect("Expect header");

        let Some(x5c) = header.x5c else {
            return Err(SignedDataVerifierError::VerificationFailure);
        };

        if x5c.is_empty() {
            return Err(SignedDataVerifierError::VerificationFailure);
        }

        let x5c: Result<Vec<Vec<u8>>, DecodeError> = x5c.iter().map(|c| c.as_der_bytes()).collect();
        let chain = x5c?;

        if header.alg != Algorithm::ES256 {
            return Err(SignedDataVerifierError::VerificationFailure);
        }

        let pub_key = verify_chain(&chain, &self.root_certificates, None)?;
        let pub_key = &pub_key[pub_key.len() - 65..];

        let decoding_key = DecodingKey::from_ec_der(pub_key);
        let claims: [&str; 0] = [];

        let mut validator = Validation::new(Algorithm::ES256);
        validator.validate_exp = false;
        validator.set_required_spec_claims(&claims);

        let payload = jsonwebtoken::decode::<T>(signed_obj, &decoding_key, &validator).expect("Expect Payload");
        return Ok(payload.claims);
    }
}

#[cfg(test)]
mod tests {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use crate::primitives::notification_type_v2::NotificationTypeV2;
    use super::*;

    pub fn signed_payload() -> String {
        std::env::var("SIGNED_PAYLOAD").expect("SIGNED_PAYLOAD must be set")
    }

    pub fn apple_root_cert() -> String {
        std::env::var("APPLE_ROOT_BASE64_ENCODED").expect("APPLE_ROOT_BASE64_ENCODED must be set")
    }

    #[test]
    fn text_verify_and_decode_notification() {
        dotenv::dotenv().ok();

        let root_cert = apple_root_cert();
        let root_cert_der = STANDARD.decode(root_cert).expect("Expect bytes");

        let verifier = SignedDataVerifier::new(vec![root_cert_der],
                                               Environment::Sandbox,
                                               "app.namecare.ios".to_string(),
                                               Some(1578773551));

        let payload = signed_payload();
        let decoded_payload = verifier.verify_and_decode_notification(payload.as_str()).unwrap();

        assert_eq!(decoded_payload.notification_type, NotificationTypeV2::DidRenew);
    }

    #[test]
    fn text_verify_and_decode_transaction() {
        dotenv::dotenv().ok();

        let root_cert = apple_root_cert();
        let root_cert_der = STANDARD.decode(root_cert).expect("Expect bytes");

        let verifier = SignedDataVerifier::new(vec![root_cert_der],
                                               Environment::Sandbox,
                                               "app.namecare.ios".to_string(),
                                               Some(1578773551));

        let payload = signed_payload();
        let decoded_payload = verifier.verify_and_decode_notification(payload.as_str()).unwrap();
        let tx = decoded_payload.data.as_ref().unwrap().signed_transaction_info.as_ref().unwrap();
        let decoded_tx = verifier.verify_and_decode_signed_transaction(tx.as_str()).unwrap();

        assert_eq!(decoded_tx.bundle_id, decoded_payload.data.unwrap().bundle_id);
    }

    fn test_app_store_server_notification_decoding_production() { todo!() }

    fn test_missing_x5c_header() { todo!() }

    fn test_wrong_bundle_id_for_server_notification() {
        todo!()
    }

    fn test_wrong_app_apple_id_for_server_notification() { todo!() }
}