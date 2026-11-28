use super::*;

#[test]
fn update_product_rejects_release_fetch_before_auth_or_network() {
    let error = fetch_latest_release_blocking().expect_err("KCode updater must fail closed");
    assert!(error.to_string().contains("KCode self-update is disabled"));
}
