use atv_remote::cert::generate_identity;
use atv_remote::config::AppConfig;
use tempfile::tempdir;

#[test]
fn test_generate_identity_produces_valid_rsa_cert() {
    let (cert_der, key_der) = generate_identity().expect("generate_identity failed");
    assert!(!cert_der.as_ref().is_empty());
    assert!(!key_der.secret_der().is_empty());
}

#[test]
fn test_app_config_persistence() {
    let dir = tempdir().expect("tempdir failed");
    let config_path = dir.path().join("config.json");

    let cfg = AppConfig {
        last_tv_ip: Some("192.168.1.120".to_string()),
        tv_name: Some("Living Room Android TV".to_string()),
        paired: true,
    };

    cfg.save_to(&config_path).expect("save failed");
    assert!(config_path.exists());

    let loaded = AppConfig::load_from(&config_path).expect("load failed");
    assert_eq!(loaded.last_tv_ip.as_deref(), Some("192.168.1.120"));
    assert_eq!(loaded.tv_name.as_deref(), Some("Living Room Android TV"));
    assert!(loaded.paired);
}

#[test]
fn test_corrupted_pem_regenerates_identity() {
    let dir = tempdir().expect("tempdir failed");
    let cert_path = dir.path().join("cert.pem");
    let key_path = dir.path().join("key.pem");

    // Write garbage to both files
    std::fs::write(&cert_path, "NOT A VALID PEM").unwrap();
    std::fs::write(&key_path, "CORRUPTED KEY DATA").unwrap();

    let (cert_der, key_der) = atv_remote::cert::load_or_generate_identity_in_dir(dir.path())
        .expect("must recover and regenerate rather than crashing");
    assert!(!cert_der.as_ref().is_empty());
    assert!(!key_der.secret_der().is_empty());
}
