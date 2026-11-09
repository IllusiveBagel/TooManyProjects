//! The server's own HTTPS: its certificate, and the addresses it covers.

mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn https_is_off_in_tests_unless_asked() {
    let app = TestApp::new().await;
    let logan = app.setup_owner().await;
    let res = app.call("GET", "/api/v1/https", Some(&logan), None).await;
    assert_eq!(res.body["mode"], "off");
    assert_eq!(app.call("GET", "/api/v1/https/certificate", None, None).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_app_makes_a_certificate_and_the_owner_adds_addresses() {
    let app = TestApp::with_config(|c| {
        c.https_bind = Some("127.0.0.1:0".parse().unwrap());
        c.tls_hosts = vec!["NAS.local".into()];
    })
    .await;
    let logan = app.setup_owner().await;

    let res = app.call("GET", "/api/v1/https", Some(&logan), None).await;
    assert_eq!(res.body["mode"], "generated", "{:?}", res.body);
    let hosts: Vec<&str> =
        res.body["hosts"].as_array().unwrap().iter().map(|h| h["host"].as_str().unwrap()).collect();
    assert!(hosts.contains(&"localhost") && hosts.contains(&"nas.local"), "{hosts:?}");
    assert!(res.body["fingerprint"].as_str().unwrap().contains(':'));
    assert!(app.data_dir.join("tls/ca.pem").is_file());
    let key = std::fs::metadata(app.data_dir.join("tls/ca-key.pem")).unwrap();
    #[cfg(unix)]
    assert_eq!(std::os::unix::fs::PermissionsExt::mode(&key.permissions()) & 0o777, 0o600);

    // The certificate to trust is public, like any certificate.
    let (status, pem) = app.call_raw("GET", "/api/v1/https/certificate", "", "text/plain", vec![]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(String::from_utf8(pem).unwrap().starts_with("-----BEGIN CERTIFICATE-----"));

    let res =
        app.call("POST", "/api/v1/https/hosts", Some(&logan), Some(json!({"host": "192.168.1.50"}))).await;
    assert_eq!(res.status, StatusCode::OK, "{:?}", res.body);
    let added = res.body["hosts"].as_array().unwrap().iter().find(|h| h["host"] == "192.168.1.50").unwrap();
    assert_eq!(added["added"], true);
    let res =
        app.call("POST", "/api/v1/https/hosts", Some(&logan), Some(json!({"host": "not a host"}))).await;
    assert_eq!(res.body["code"], "invalid_host");

    // Only the owner changes it.
    let invite = app.call("POST", "/api/v1/invites", Some(&logan), Some(json!({}))).await;
    let sam = app
        .call(
            "POST",
            "/api/v1/auth/accept-invite",
            None,
            Some(json!({"token": invite.body["token"], "name": "Sam", "email": "sam@example.com", "password": "a long password"})),
        )
        .await
        .cookie
        .unwrap();
    let res = app.call("POST", "/api/v1/https/hosts", Some(&sam), Some(json!({"host": "evil.lan"}))).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    assert_eq!(app.call("GET", "/api/v1/https", Some(&sam), None).await.status, StatusCode::OK);

    let res = app.call("DELETE", "/api/v1/https/hosts/192.168.1.50", Some(&logan), None).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(
        app.call("DELETE", "/api/v1/https/hosts/localhost", Some(&logan), None).await.status,
        StatusCode::NOT_FOUND
    );
}
