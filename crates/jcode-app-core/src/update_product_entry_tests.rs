//! Entry-point regressions: rejection must precede network and install work.

use super::*;

fn assert_product_rejection<T>(result: Result<T>) {
    match result {
        Ok(_) => panic!("KCode must not enter the inherited updater"),
        Err(error) => assert!(error.to_string().contains("KCode self-update is disabled")),
    }
}

#[test]
fn update_product_blocks_background_and_explicit_release_checks() {
    assert!(!should_auto_update());
    assert_product_rejection(fetch_latest_release_blocking());
    assert_product_rejection(latest_main_sha_blocking());
    assert_product_rejection(prepare_update_blocking());
    assert_product_rejection(check_for_update_blocking());
}

#[test]
fn update_product_blocks_source_install_before_resolving_jcode_paths() {
    assert_product_rejection(build_from_source());
    assert_product_rejection(install_main_source_update_blocking("unused"));
}

#[test]
fn update_product_rejects_supplied_release_before_asset_or_download_work() {
    let release: GitHubRelease = serde_json::from_value(serde_json::json!({
        "tag_name": "v99.0.0",
        "html_url": "https://github.com/KooshaPari/KCode/releases/tag/v99.0.0",
        "assets": [],
        "target_commitish": "unused"
    }))
    .expect("minimal release");
    assert_product_rejection(download_and_install_blocking_with_progress(
        &release,
        |_| panic!("a rejected update must never report download progress"),
    ));
}
