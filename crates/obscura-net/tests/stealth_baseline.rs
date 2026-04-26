#![cfg(feature = "stealth")]

use obscura_net::STEALTH_USER_AGENT;

const CHROME145_BASELINE_TLS_EXTENSIONS: &[&str] = &[
    "server_name",
    "extended_master_secret",
    "renegotiation_info",
    "supported_groups",
    "ec_point_formats",
    "session_ticket",
    "application_layer_protocol_negotiation",
    "status_request",
    "signature_algorithms",
    "signed_certificate_timestamp",
    "key_share",
    "psk_key_exchange_modes",
    "supported_versions",
    "compress_certificate",
    "encrypted_client_hello",
    "padding",
];

const CHROME145_BASELINE_TLS_CIPHERS: &[&str] = &[
    "TLS_AES_128_GCM_SHA256",
    "TLS_AES_256_GCM_SHA384",
    "TLS_CHACHA20_POLY1305_SHA256",
    "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256",
    "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256",
    "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384",
    "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384",
    "TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256",
    "TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256",
    "TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA",
    "TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA",
    "TLS_RSA_WITH_AES_128_GCM_SHA256",
    "TLS_RSA_WITH_AES_256_GCM_SHA384",
    "TLS_RSA_WITH_AES_128_CBC_SHA",
    "TLS_RSA_WITH_AES_256_CBC_SHA",
];

#[test]
fn chrome145_tls_baseline_snapshot_is_present() {
    assert_eq!(STEALTH_USER_AGENT, "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36");
    assert_eq!(CHROME145_BASELINE_TLS_EXTENSIONS[0], "server_name");
    assert_eq!(CHROME145_BASELINE_TLS_CIPHERS[0], "TLS_AES_128_GCM_SHA256");
}
