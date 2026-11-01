use rand::rngs::OsRng;
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use rsa::pkcs8::EncodePrivateKey;
use rsa::RsaPrivateKey;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use std::fs;

use crate::config::get_config_dir;

/// Generates a new self-signed 2048-bit RSA X.509 certificate and private key.
pub fn generate_identity() -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>), Box<dyn std::error::Error>> {
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

/// Loads existing certificate and private key from the app config directory,
/// or generates a fresh pair and saves them as PEM files if missing.
pub fn load_or_generate_identity() -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>), Box<dyn std::error::Error>> {
    let config_dir = get_config_dir();
    fs::create_dir_all(&config_dir)?;

    let cert_path = config_dir.join("cert.pem");
    let key_path = config_dir.join("key.pem");

    if cert_path.exists() && key_path.exists() {
        if let (Ok(cert_pem), Ok(key_pem)) = (fs::read_to_string(&cert_path), fs::read_to_string(&key_path)) {
            let certs = rustls_pemfile::certs(&mut cert_pem.as_bytes())
                .collect::<Result<Vec<_>, _>>()?;
            let key = rustls_pemfile::private_key(&mut key_pem.as_bytes())?;
            if let (Some(cert), Some(key)) = (certs.into_iter().next(), key) {
                return Ok((cert, key));
            }
        }
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

    let cert_der = CertificateDer::from(cert.der().to_vec());
    let key_der = PrivateKeyDer::Pkcs8(key_pair.serialize_der().into());

    Ok((cert_der, key_der))
}
