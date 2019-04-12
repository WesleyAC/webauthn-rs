extern crate base64;
#[macro_use]
extern crate serde_derive;
extern crate byteorder;
extern crate sha2;

pub mod constants;
pub mod error;
pub mod proto;

// use digest::digest::Digest;
use crate::sha2::digest::generic_array::functional::FunctionalSequence;
use crate::sha2::Digest;
use std::collections::BTreeMap;

use constants::*;
use proto::*;
use rand::prelude::*;

type UserId = String;

#[derive(Clone)]
pub struct Challenge(Vec<u8>);

impl std::fmt::Debug for Challenge {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", base64::encode(&self.0))
    }
}

impl std::fmt::Display for Challenge {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", base64::encode(&self.0))
    }
}

// We have to remember the challenges we issued, so keep a reference ...

pub struct Webauthn<T> {
    rng: StdRng,
    config: T,
    pkcp: Vec<PubKeyCredParams>,
    rp_id_hash: Vec<u8>,
}

impl<T> Webauthn<T> {
    pub fn new(config: T) -> Self
    where
        T: WebauthnConfig,
    {
        let pkcp = config
            .get_credential_algorithms()
            .iter()
            .map(|a| PubKeyCredParams {
                type_: "public-key".to_string(),
                alg: a.into(),
            })
            .collect();
        println!("rp_id: {:?}", config.get_relying_party_id());
        let rp_id_hash = {
            let mut hasher = sha2::Sha256::new();
            hasher.input(config.get_relying_party_id().as_bytes());
            hasher.result().iter().map(|b| *b).collect()
        };
        Webauthn {
            // rng: config.get_rng(),
            // We use stdrng because unlike thread_rng, it's a csprng, which given
            // this is a cryptographic operation, we kind of want!
            rng: StdRng::from_entropy(),
            config: config,
            pkcp: pkcp,
            rp_id_hash: rp_id_hash,
        }
    }

    fn generate_challenge(&mut self) -> Challenge {
        Challenge(
            (0..CHALLENGE_SIZE_BYTES)
                // TODO: UNDO THIS ITS ONLY FOR TESTING AND HOLY SHIT ITS INSECURE
                // .map(|_| self.rng.gen())
                .map(|_| 0)
                .collect::<Vec<u8>>(),
        )
    }

    fn generate_challenge_response(
        &mut self,
        username: &UserId,
        chal: &Challenge,
    ) -> CreationChallengeResponse
    where
        T: WebauthnConfig,
    {
        println!("Challenge for {} -> {:?}", username, chal);
        CreationChallengeResponse::new(
            self.config.get_relying_party_name(),
            username.clone(),
            username.clone(),
            username.clone(),
            chal.to_string(),
            self.pkcp.clone(),
            self.config.get_authenticator_timeout(),
        )
        // Now, do we persist the challenge here for tests so we can
        // byyass the RNG parts?
        // Or do we do it in the challenge_register, and have the test
        // just pass in the challenge to the verify so that tests
        // don't need a config at all?
    }

    pub fn generate_challenge_register(&mut self, username: UserId) -> CreationChallengeResponse
    where
        T: WebauthnConfig,
    {
        let chal = self.generate_challenge();
        let c = self.generate_challenge_response(&username, &chal);

        self.config.persist_challenge(username, chal);
        c
    }

    pub fn generate_challenge_login(&mut self, username: UserId) -> RequestChallengeResponse
    where
        T: WebauthnConfig,
    {
        let chal = self.generate_challenge();

        // Get the user's existing creds if any.

        let uc = self.config.retrieve_credentials(username.as_str());

        /*
        let ac = match uc {
            Some(creds) => creds
                .iter()
                .map(|cred_id| AllowCredentials {
                    type_: "public-key".to_string(),
                    id: cred_id.clone(),
                })
                .collect(),
            None => Vec::new(),
        };
        println!("Creds for {} -> {:?}", username, ac);
        */

        unimplemented!();
    }

    // From the rfc https://w3c.github.io/webauthn/#registering-a-new-credential
    pub fn register_credential(&mut self, reg: RegisterResponse) -> Result<(), ()>
    where
        T: WebauthnConfig,
    {
        // get the challenge
        // send to register_credential_internal
        // match res, if good, save cred.

        Err(())
    }

    pub(crate) fn register_credential_internal(
        &mut self,
        reg: RegisterResponse,
        chal: Challenge,
    ) -> Result<(), ()>
    where
        T: WebauthnConfig,
    {
        println!("{:?}", reg);

        // Let JSONtext be the result of running UTF-8 decode on the value of response.clientDataJSON.
        //  ^-- this is done in the actix extractors.

        // Let C, the client data claimed as collected during the credential creation, be the result of running an implementation-specific JSON parser on JSONtext.
        let client_data = CollectedClientData::from(&reg.response.clientDataJSON);
        println!("{:?}", client_data);

        // Verify that the value of C.type is webauthn.create.
        if client_data.type_ != "webauthn.create" {
            println!("Invalid client_data type");
            return Err(());
        }

        // Verify that the value of C.challenge matches the challenge that was sent to the authenticator in the create() call.
        // First, we have to decode the challenge to vec?
        let decoded_challenge = base64::decode(&client_data.challenge).unwrap();
        if decoded_challenge != chal.0 {
            println!(
                "ClientCollectedData challenge does not match the challenge we have associated!"
            );
            return Err(());
        }

        // Verify that the value of C.origin matches the Relying Party's origin.
        if &client_data.origin != self.config.get_origin() {
            println!(
                "ClientCollectedData origin {} does not match our configured origin",
                client_data.origin
            );
        }

        // Verify that the value of C.tokenBinding.status matches the state of Token Binding for the TLS connection over which the assertion was obtained. If Token Binding was used on that TLS connection, also verify that C.tokenBinding.id matches the base64url encoding of the Token Binding ID for the connection.
        //
        //  This could be reasonably complex to do, given that we could be behind a load balancer
        // or we may not directly know the status of TLS inside this api. I'm open to creative
        // suggestions on this topic!
        //

        // 7. Compute the hash of response.clientDataJSON using SHA-256.
        //    This will be used in step 14.
        let client_data_json_hash: Vec<u8> = {
            let mut hasher = sha2::Sha256::new();
            hasher.input(reg.response.clientDataJSON.as_bytes());
            hasher.result().iter().map(|b| *b).collect()
        };

        // Perform CBOR decoding on the attestationObject field of the AuthenticatorAttestationResponse structure to obtain the attestation statement format fmt, the authenticator data authData, and the attestation statement attStmt.
        let attest_data = AttestationObject::from(&reg.response.attestationObject);
        println!("{:?}", attest_data);

        // Verify that the rpIdHash in authData is the SHA-256 hash of the RP ID expected by the Relying Party.
        //
        //  NOW: Remember that RP ID https://w3c.github.io/webauthn/#rp-id is NOT THE SAME as the RP name
        // it's actually derived from the RP origin.
        if attest_data.authData.rp_id_hash != self.rp_id_hash {
            println!("rp_id_hash from authenitcatorData does not match our rp_id_hash");
            let a: String = base64::encode(&attest_data.authData.rp_id_hash);
            let b: String = base64::encode(&self.rp_id_hash);
            println!("{:?} != {:?}", a, b);
            return Err(());
        }

        // Verify that the User Present bit of the flags in authData is set.
        if !attest_data.authData.user_present {
            println!("User not present!");
            return Err(());
        }

        // Check that signCount has not gone backwards (NOT AN RFC REQUIREMENT, THIS IS AN ADDITIONAL STEP FOR THIS IMPLEMENTATION)
        //
        //  We probably need a config.get_user_token_counter((user, tokenid)) -> counter funciton hook.

        // If user verification is required for this registration, verify that the User Verified bit of the flags in authData is set.
        if self.config.get_user_verification_required() && !attest_data.authData.user_verified {
            println!("User not verified when required!");
            return Err(());
        }

        // Verify that the values of the client extension outputs in clientExtensionResults and the authenticator extension outputs in the extensions in authData are as expected, considering the client extension input values that were given as the extensions option in the create() call. In particular, any extension identifier values in the clientExtensionResults and the extensions in authData MUST be also be present as extension identifier values in the extensions member of options, i.e., no extensions are present that were not requested. In the general case, the meaning of "are as expected" is specific to the Relying Party and which extensions are in use.

        // TODO: Today we send NO EXTENSIONS, so we'll never have a case where the extensions
        // are present! But because extensions are possible from the config we WILL need to manage
        // this situation eventually!!!
        match attest_data.authData.extensions {
            Some(ex) => {
                // We don't know how to handle client extensions yet!!!
                unimplemented!();
            }
            None => {}
        }

        // Determine the attestation statement format by performing a USASCII case-sensitive match on fmt against the set of supported WebAuthn Attestation Statement Format Identifier values. An up-to-date list of registered WebAuthn Attestation Statement Format Identifier values is maintained in the IANA registry of the same name [WebAuthn-Registries]. ( https://tools.ietf.org/html/draft-hodges-webauthn-registries-02 )
        //
        // So the rfc here actually doesn't help, and I can't see the values obviously. It looks like
        // there is probably a string list of types of attestation statements, and my token is giving
        // me "fido-u2f" but I'm sure there are more, and a better way to get these from a registry
        // that seems to be undeclared ...
        match attest_data.fmt.as_str() {
            "fido-u2f" => {},
            e => {
                println!("unknown fmt type {:?}", e);
                return Err(())
            }
        };

        // 14. Verify that attStmt is a correct attestation statement, conveying a valid attestation signature, by using the attestation statement format fmt’s verification procedure given attStmt, authData and the hash of the serialized client data computed in step 7.
        let acd = match &attest_data.authData.acd {
            Some(acd) => acd,
            None => {
                println!("No ACD present!");
                return Err(());
            }
        };
        let att_type = match self.verify_attStmt(
            &attest_data.attStmt,
            acd,
            &attest_data.authDataBytes,
            &client_data_json_hash
        ) {
            Ok(t) => t,
            Err(e) => {
                return Err(e)
            }
        };

        // If validation is successful, obtain a list of acceptable trust anchors (attestation root certificates or ECDAA-Issuer public keys) for that attestation type and attestation statement format fmt, from a trusted source or from policy. For example, the FIDO Metadata Service [FIDOMetadataService] provides one way to obtain such information, using the aaguid in the attestedCredentialData in authData.

        // 16: Assess the attestation trustworthiness using the outputs of the verification procedure in step 14, as follows: (SEE RFC)
        // If the attestation statement attStmt successfully verified but is not trustworthy per step 16 above, the Relying Party SHOULD fail the registration ceremony.

        // Check that the credentialId is not yet registered to any other user. If registration is requested for a credential that is already registered to a different user, the Relying Party SHOULD fail this registration ceremony, or it MAY decide to accept the registration, e.g. while deleting the older registration.

        //  If the attestation statement attStmt verified successfully and is found to be trustworthy, then register the new credential with the account that was denoted in the options.user passed to create(), by associating it with the credentialId and credentialPublicKey in the attestedCredentialData in authData, as appropriate for the Relying Party's system.

        unimplemented!();
    }

    fn verify_attStmt(
        &self,
        attStmt: &serde_cbor::Value,
        acd: &AttestedCredentialData,
        authDataBytes: &Vec<u8>,
        client_data_hash: &Vec<u8>,
    ) -> Result<AttStmtType, ()> {
        // Okay, the process is:
        //  https://w3c.github.io/webauthn/#generating-an-attestation-object
        // https://w3c.github.io/webauthn/#packed-attestation

        // Given the verification procedure inputs attStmt, authenticatorData and clientDataHash, the verification procedure is as follows:

        // Verify that attStmt is valid CBOR conforming to the syntax defined above and perform CBOR decoding on it to extract the contained fields.
        // ^-- This is already DONE as a factor of serde_cbor not erroring up to this point,
        // and those errors will be handled better than just "unwrap" :)
        // we'll also find out quickly when we attempt to access the data as a map ...

        println!("attStmt: {:?}", attStmt);

        let attStmtMap = match attStmt.as_object() {
            Some(m) => m,
            None => {
                println!("Can't get attStmt map");
                return Err(());
            }
        };

        // If x5c is present, this indicates that the attestation type is not ECDAA. In this case:
        let x5c = attStmtMap.get(&serde_cbor::ObjectKey::String("x5c".to_string()));
        if x5c.is_some() {
            // This is safe since we already did the is_some.
            let x5ci = x5c.unwrap();
            println!("x5ci: {:?}", x5ci);

            // Verify that sig is a valid signature over the concatenation of authenticatorData and clientDataHash using the attestation public key in attestnCert with the algorithm specified in alg.
            // sig is from m
            let sig = match attStmtMap.get(&serde_cbor::ObjectKey::String("sig".to_string())) {
                Some(s) => s,
                None => {
                    println!("Can't get attStmt sig");
                    return Err(())
                }
            };
            println!("sig: {:?}", sig);
            let concat: Vec<u8> = authDataBytes.iter().chain(client_data_hash.iter())
                .map(|b| *b)
                .collect();

            // Now, they say to get the alg, which we do from the alg
            // which is in the authData.acd.credential_pk;
            // The credential_pk is in "COSE_Key format" apparently
            // which is documented here
            // https://www.rfc-editor.org/rfc/rfc8152.txt
            // which means that alg is in optional field keyd 3 in the map.

            // Object({Integer(-3): Bytes([48, 185, 178, 204, 113, 186, 105, 138, 190, 33, 160, 46, 131, 253, 100, 177, 91, 243, 126, 128, 245, 119, 209, 59, 186, 41, 215, 196, 24, 222, 46, 102]), Integer(-2): Bytes([158, 212, 171, 234, 165, 197, 86, 55, 141, 122, 253, 6, 92, 242, 242, 114, 158, 221, 238, 163, 127, 214, 120, 157, 145, 226, 232, 250, 144, 150, 218, 138]), Integer(-1): U64(1), Integer(1): U64(2), Integer(3): I64(-7)})
            // 
            let cred_pk = match acd.credential_pk.as_object() {
                Some(cred_pk) => cred_pk,
                None => {
                    println!("ACD cbor not usable as map");
                    return Err(());
                }
            };

            let alg_id = match cred_pk.get(&serde_cbor::ObjectKey::Integer(3)) {
                Some(id) => {
                    match id.as_i64() {
                        Some(i) => i,
                        None => {
                            println!("ALG ID Was not an integer!?");
                            return Err(());
                        }
                    }
                }
                None => {
                    println!("No ALG ID present");
                    return Err(());
                }
            };

            let alg_enum = match Algorithm::new(alg_id) {
                Some(a) => a,
                None => {
                    println!("Alg ID not understood by our code ...");
                    return Err(());
                }
            };

            println!("Selected alg id {:?}", alg_enum);

            // Verify stuff meow.
            // https://medium.com/@herrjemand/verifying-fido2-packed-attestation-a067a9b2facd
            let valid = match alg_enum {
                ALG_ECDSA_SHA256 => {
                    //     Extract leaf cert from “x5c” as attCert
                }
                ALG_RSASSA_PKCS15_SHA256 => {
                    unimplemented!()
                }
                ALG_RSASSA_PSS_SHA256 => {
                    unimplemented!()
                }
            };

            // Verify that attestnCert meets the requirements in §8.2.1 Packed Attestation Statement Certificate Requirements.
                    // Check that attCert is of version 3(ASN1 INT 2)
                    // Check that attCert subject country (C) is set to a valid two character ISO 3166 code
                    // Check that attCert subject organisation (O) is not empty
                    // Check that attCert subject organisation unit (OU) is set to literal string “Authenticator Attestation”
                    // Check that attCert subject common name(CN) is not empty.
                    // Check that attCert basic constraints for CA is set to FALSE
                    // If certificate contains id-fido-gen-ce-aaguid(1.3.6.1.4.1.45724.1.1.4) extension, then check that its value set to the AAGUID returned by the authenticator in authData.
                    // Verify signature “sig” over the signatureBase with the public key extracted from leaf attCert in “x5c”, using the algorithm “alg”


            // If attestnCert contains an extension with OID 1.3.6.1.4.1.45724.1.1.4 (id-fido-gen-ce-aaguid) verify that the value of this extension matches the aaguid in authenticatorData.

            // Optionally, inspect x5c and consult externally provided knowledge to determine whether attStmt conveys a Basic or AttCA attestation.

            // If successful, return implementation-specific values representing attestation type Basic, AttCA or uncertainty, and attestation trust path x5c.
            return Ok(AttStmtType::X5C)
        }

        // If ecdaaKeyId is present, then the attestation type is ECDAA. In this case:

            // Verify that sig is a valid signature over the concatenation of authenticatorData and clientDataHash using ECDAA-Verify with ECDAA-Issuer public key identified by ecdaaKeyId (see [FIDOEcdaaAlgorithm]).

            // If successful, return implementation-specific values representing attestation type ECDAA and attestation trust path ecdaaKeyId.

        // If neither x5c nor ecdaaKeyId is present, self attestation is in use.

            // Validate that alg matches the algorithm of the credentialPublicKey in authenticatorData.

            // Verify that sig is a valid signature over the concatenation of authenticatorData and clientDataHash using the credential public key with alg.

            // If successful, return implementation-specific values representing attestation type Self and an empty attestation trust path.
        unimplemented!();
    }

    pub fn verify_credential(&self, lgn: LoginRequest) -> Result<(), ()> {
        // https://w3c.github.io/webauthn/#verifying-assertion
        println!("{:?}", lgn);

        unimplemented!();
    }
}

pub trait WebauthnConfig {
    fn get_relying_party_name(&self) -> String;

    // TODO: This should be a generic impl that produceds the following from origin
    //    https://name:port/path
    //            name  <<-- this is the rp_id
    // It should check it is https also.
    // For now, we expect people to over-ride it though ...
    fn get_relying_party_id(&self) -> String;

    fn persist_challenge(&mut self, userid: UserId, challenge: Challenge) -> Result<(), ()>;

    fn retrieve_challenge(&self, userid: &str) -> Option<Challenge>;

    fn persist_credential(&mut self, userid: UserId) -> Result<(), ()>;

    fn retrieve_credentials(&self, userid: &str) -> Option<Vec<()>>;

    fn get_credential_algorithms(&self) -> Vec<Algorithm> {
        vec![Algorithm::ALG_ECDSA_SHA256]
    }

    fn get_authenticator_timeout(&self) -> u32 {
        AUTHENTICATOR_TIMEOUT
    }

    // Currently false, because I can't work out what is needed to get the UV bit to set ...
    fn get_user_verification_required(&self) -> bool {
        false
    }

    // This probably shouldn't be the default impl, so move it?
    fn get_origin(&self) -> &String;

    // By default do we need any?
    // TODO: Is this the right type? The standard is a bit confusing
    // in this section:
    // https://w3c.github.io/webauthn/#extensions
    fn get_extensions(&self) -> Option<JSONExtensions> {
        None
    }

    /*
    fn get_rng(&self) -> dyn rand::Rng {
        StdRng::from_entropy()
    }
    */
}

pub struct WebauthnEphemeralConfig {
    chals: BTreeMap<UserId, Challenge>,
    creds: BTreeMap<UserId, Vec<CredentialID>>,
    rp: String,
    rp_id: String,
    origin: String,
}

impl WebauthnConfig for WebauthnEphemeralConfig {
    fn get_relying_party_name(&self) -> String {
        self.rp.clone()
    }

    fn get_relying_party_id(&self) -> String {
        self.rp_id.clone()
    }

    fn persist_challenge(&mut self, userid: UserId, challenge: Challenge) -> Result<(), ()> {
        self.chals.insert(userid, challenge);
        Ok(())
    }

    fn retrieve_challenge(&self, userid: &str) -> Option<Challenge> {
        unimplemented!();
        None
    }

    fn persist_credential(&mut self, userid: UserId) -> Result<(), ()> {
        unimplemented!();
    }

    fn retrieve_credentials(&self, userid: &str) -> Option<Vec<()>> {
        unimplemented!();
        None
    }

    fn get_origin(&self) -> &String {
        &self.origin
    }
}

impl WebauthnEphemeralConfig {
    pub fn new(rp: &str, origin: &str, rp_id: &str) -> Self {
        WebauthnEphemeralConfig {
            chals: BTreeMap::new(),
            creds: BTreeMap::new(),
            rp: rp.to_string(),
            rp_id: rp_id.to_string(),
            origin: origin.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ephemeral() {}

    // Test the crypto operations of the webauthn impl

    #[test]
    fn test_registration() {
        let wan_c = WebauthnEphemeralConfig::new(
            "http://127.0.0.1:8080/auth",
            "http://127.0.0.1:8080",
            "127.0.0.1",
        );
        let mut wan = Webauthn::new(wan_c);
        // Generated by a yubico 5
        // Make a "fake" challenge, where we know what the values should be ....

        let zero_chal = Challenge((0..CHALLENGE_SIZE_BYTES).map(|_| 0).collect::<Vec<u8>>());

        // This is the json challenge this would generate in this case, with the rp etc.
        // {"publicKey":{"rp":{"name":"http://127.0.0.1:8080/auth"},"user":{"id":"xxx","name":"xxx","displayName":"xxx"},"challenge":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=","pubKeyCredParams":[{"type":"public-key","alg":-7}],"timeout":6000,"attestation":"direct"}}

        // And this is the response, from a real device. Let's register it!

        let rsp = r#"
        {"id":"0xYE4bQ_HZM51-XYwp7WHJu8RfeA2Oz3_9HnNIZAKqRTz9gsUlF3QO7EqcJ0pgLSwDcq6cL1_aQpTtKLeGu6Ig","rawId":"0xYE4bQ/HZM51+XYwp7WHJu8RfeA2Oz3/9HnNIZAKqRTz9gsUlF3QO7EqcJ0pgLSwDcq6cL1/aQpTtKLeGu6Ig==","response":{"attestationObject":"o2NmbXRoZmlkby11MmZnYXR0U3RtdKJjc2lnWEcwRQIhALjRb43YFcbJ3V9WiYPpIrZkhgzAM6KTR8KIjwCXejBCAiAO5Lvp1VW4dYBhBDv7HZIrxZb1SwKKYOLfFRXykRxMqGN4NWOBWQLBMIICvTCCAaWgAwIBAgIEGKxGwDANBgkqhkiG9w0BAQsFADAuMSwwKgYDVQQDEyNZdWJpY28gVTJGIFJvb3QgQ0EgU2VyaWFsIDQ1NzIwMDYzMTAgFw0xNDA4MDEwMDAwMDBaGA8yMDUwMDkwNDAwMDAwMFowbjELMAkGA1UEBhMCU0UxEjAQBgNVBAoMCVl1YmljbyBBQjEiMCAGA1UECwwZQXV0aGVudGljYXRvciBBdHRlc3RhdGlvbjEnMCUGA1UEAwweWXViaWNvIFUyRiBFRSBTZXJpYWwgNDEzOTQzNDg4MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEeeo7LHxJcBBiIwzSP+tg5SkxcdSD8QC+hZ1rD4OXAwG1Rs3Ubs/K4+PzD4Hp7WK9Jo1MHr03s7y+kqjCrutOOqNsMGowIgYJKwYBBAGCxAoCBBUxLjMuNi4xLjQuMS40MTQ4Mi4xLjcwEwYLKwYBBAGC5RwCAQEEBAMCBSAwIQYLKwYBBAGC5RwBAQQEEgQQy2lIHo/3QDmT7AonKaFUqDAMBgNVHRMBAf8EAjAAMA0GCSqGSIb3DQEBCwUAA4IBAQCXnQOX2GD4LuFdMRx5brr7Ivqn4ITZurTGG7tX8+a0wYpIN7hcPE7b5IND9Nal2bHO2orh/tSRKSFzBY5e4cvda9rAdVfGoOjTaCW6FZ5/ta2M2vgEhoz5Do8fiuoXwBa1XCp61JfIlPtx11PXm5pIS2w3bXI7mY0uHUMGvxAzta74zKXLslaLaSQibSKjWKt9h+SsXy4JGqcVefOlaQlJfXL1Tga6wcO0QTu6Xq+Uw7ZPNPnrpBrLauKDd202RlN4SP7ohL3d9bG6V5hUz/3OusNEBZUn5W3VmPj1ZnFavkMB3RkRMOa58MZAORJT4imAPzrvJ0vtv94/y71C6tZ5aGF1dGhEYXRhWMQSyhe0mvIolDbzA+AWYDCiHlJdJm4gkmdDOAGo/UBxoEEAAAAAAAAAAAAAAAAAAAAAAAAAAABA0xYE4bQ/HZM51+XYwp7WHJu8RfeA2Oz3/9HnNIZAKqRTz9gsUlF3QO7EqcJ0pgLSwDcq6cL1/aQpTtKLeGu6IqUBAgMmIAEhWCCe1KvqpcVWN416/QZc8vJynt3uo3/WeJ2R4uj6kJbaiiJYIDC5ssxxummKviGgLoP9ZLFb836A9XfRO7op18QY3i5m","clientDataJSON":"eyJjaGFsbGVuZ2UiOiJBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBIiwiY2xpZW50RXh0ZW5zaW9ucyI6e30sImhhc2hBbGdvcml0aG0iOiJTSEEtMjU2Iiwib3JpZ2luIjoiaHR0cDovLzEyNy4wLjAuMTo4MDgwIiwidHlwZSI6IndlYmF1dGhuLmNyZWF0ZSJ9"},"type":"public-key"}
        "#;
        // turn it into our "deserialised struct"
        let rsp_d: RegisterResponse = serde_json::from_str(rsp).unwrap();

        // Now register, providing our fake challenge.
        let result = wan.register_credential_internal(rsp_d, zero_chal);
        println!("{:?}", result);
        assert!(result.is_ok());
    }

    // This is an attestation-none response (zero chal)
    // {"id":"3X-5CTlxwwSS_yNnTkZusvOC3xk_l0zVi3xtXWwdB9CiBBWgeOZ0pRHKcl7sku4kJPd3sW_2TNHW8qoAW8Rqlg","rawId":"3X+5CTlxwwSS/yNnTkZusvOC3xk/l0zVi3xtXWwdB9CiBBWgeOZ0pRHKcl7sku4kJPd3sW/2TNHW8qoAW8Rqlg==","response":{"attestationObject":"o2NmbXRkbm9uZWdhdHRTdG10oGhhdXRoRGF0YVjEEsoXtJryKJQ28wPgFmAwoh5SXSZuIJJnQzgBqP1AcaBBAAAAAAAAAAAAAAAAAAAAAAAAAAAAQN1/uQk5ccMEkv8jZ05GbrLzgt8ZP5dM1Yt8bV1sHQfQogQVoHjmdKURynJe7JLuJCT3d7Fv9kzR1vKqAFvEapalAQIDJiABIVggDUwKZ63+ymZqPzF/2O/ZH2ZPE/Qi7xB4isH51A6ydIkiWCDbffU2JnR1EltRQZwP5q+FkE8+yj/vSY+FWgyeYaNT/A==","clientDataJSON":"eyJjaGFsbGVuZ2UiOiJBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBIiwiY2xpZW50RXh0ZW5zaW9ucyI6e30sImhhc2hBbGdvcml0aG0iOiJTSEEtMjU2Iiwib3JpZ2luIjoiaHR0cDovLzEyNy4wLjAuMTo4MDgwIiwidHlwZSI6IndlYmF1dGhuLmNyZWF0ZSJ9"},"type":"public-key"}

    #[test]
    fn test_authentication() {}
}
