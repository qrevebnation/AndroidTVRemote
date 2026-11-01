use atv_remote::pairing::calculate_pairing_secret;

#[test]
fn test_calculate_pairing_secret_valid_vector() {
    let client_mod = hex::decode("AABBCC").unwrap();
    let client_exp = hex::decode("010001").unwrap();
    let server_mod = hex::decode("112233").unwrap();
    let server_exp = hex::decode("010001").unwrap();

    let pin = "D48F1C"; // D4 matches digest[0]
    let secret = calculate_pairing_secret(&client_mod, &client_exp, &server_mod, &server_exp, pin)
        .expect("calculation failed");

    assert_eq!(
        hex::encode(secret),
        "d4b69888bf2ba8ff1b7d4e206ba774e097a3f46a589af85d46e2d8bc1dce987a"
    );
}

#[test]
fn test_calculate_pairing_secret_mismatched_pin_fails() {
    let client_mod = hex::decode("AABBCC").unwrap();
    let client_exp = hex::decode("010001").unwrap();
    let server_mod = hex::decode("112233").unwrap();
    let server_exp = hex::decode("010001").unwrap();

    let wrong_pin = "008F1C"; // 00 != D4
    let result = calculate_pairing_secret(&client_mod, &client_exp, &server_mod, &server_exp, wrong_pin);
    assert!(result.is_err(), "mismatched PIN must fail");
}

#[test]
fn test_calculate_pairing_secret_invalid_length_or_hex_fails() {
    let client_mod = hex::decode("AABBCC").unwrap();
    let client_exp = hex::decode("010001").unwrap();
    let server_mod = hex::decode("112233").unwrap();
    let server_exp = hex::decode("010001").unwrap();

    assert!(calculate_pairing_secret(&client_mod, &client_exp, &server_mod, &server_exp, "123").is_err());
    assert!(calculate_pairing_secret(&client_mod, &client_exp, &server_mod, &server_exp, "ZZZZZZ").is_err());
}
