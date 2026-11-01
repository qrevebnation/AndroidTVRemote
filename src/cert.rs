use rand::rngs::OsRng;
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use rsa::pkcs8::EncodePrivateKey;
use rsa::RsaPrivateKey;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{Error as RustlsError, SignatureScheme};
use std::fs;
use std::path::Path;

use crate::config::get_config_dir;

#[derive(Debug)]
pub struct NoVerifyServerCert;

impl ServerCertVerifier for NoVerifyServerCert {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ED25519,
        ]
    }
}

/// Generates a new self-signed 2048-bit RSA X.509 certificate and private key.
pub fn generate_identity(
) -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>), Box<dyn std::error::Error>> {
    let mut rng = OsRng;
    let rsa_key = RsaPrivateKey::new(&mut rng, 2048)?;
    let key_pem = rsa_key.to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)?;
    let key_pair = KeyPair::from_pem(&key_pem)?;

    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "atv-remote");
    params.distinguished_name = dn;

    let cert = params.self_signed(&key_pair)?;
    let cert_der = CertificateDer::from(cert.der().to_vec());
    let key_der = PrivateKeyDer::Pkcs8(key_pair.serialize_der().into());

    Ok((cert_der, key_der))
}

/// Loads existing certificate and private key from the given directory,
/// or regenerates fresh credentials gracefully if missing or corrupted.
pub fn load_or_generate_identity_in_dir(
    config_dir: &Path,
) -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>), Box<dyn std::error::Error>> {
    fs::create_dir_all(config_dir)?;

    let cert_path = config_dir.join("cert.pem");
    let key_path = config_dir.join("key.pem");

    if cert_path.exists() && key_path.exists() {
        if let (Ok(cert_pem), Ok(key_pem)) = (
            fs::read_to_string(&cert_path),
            fs::read_to_string(&key_path),
        ) {
            let cert_parse =
                rustls_pemfile::certs(&mut cert_pem.as_bytes()).collect::<Result<Vec<_>, _>>();
            let key_parse = rustls_pemfile::private_key(&mut key_pem.as_bytes());

            if let (Ok(certs), Ok(Some(key))) = (cert_parse, key_parse) {
                if let Some(cert) = certs.into_iter().next() {
                    return Ok((cert, key));
                }
            }
        }
        // If files were present but corrupted, log and fall through to regenerate
        eprintln!(
            "Warning: Corrupted certificate or key detected. Regenerating fresh credentials..."
        );
    }

    let mut rng = OsRng;
    let rsa_key = RsaPrivateKey::new(&mut rng, 2048)?;
    let key_pem = rsa_key.to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)?;
    let key_pair = KeyPair::from_pem(&key_pem)?;

    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "atv-remote");
    params.distinguished_name = dn;

    let cert = params.self_signed(&key_pair)?;
    let cert_pem = cert.pem();

    fs::write(&cert_path, cert_pem)?;
    fs::write(&key_path, key_pem.as_str())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600));
    }

    let cert_der = CertificateDer::from(cert.der().to_vec());
    let key_der = PrivateKeyDer::Pkcs8(key_pair.serialize_der().into());

    Ok((cert_der, key_der))
}

pub fn load_or_generate_identity(
) -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>), Box<dyn std::error::Error>> {
    load_or_generate_identity_in_dir(&get_config_dir())
}
