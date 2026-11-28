use super::super::*;
use crate::cli::startup::{apply_home, apply_startup_home};

struct SavedHome {
    previous: Option<std::ffi::OsString>,
}

impl SavedHome {
    fn capture() -> Self {
        Self {
            previous: std::env::var_os("KCODE_HOME"),
        }
    }
}

impl Drop for SavedHome {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => crate::env::set_var("KCODE_HOME", value),
            None => crate::env::remove_var("KCODE_HOME"),
        }
    }
}

#[test]
fn home_and_data_dir_are_global_and_apply_kcode_home() {
    let _guard = crate::storage::lock_test_env();
    let _saved_home = SavedHome::capture();
    let temp = tempfile::tempdir().unwrap();
    let kcode_home = temp.path().join("kcode-home");
    assert!(Args::try_parse_from(["jcode", "--data-dir", kcode_home.to_str().unwrap()]).is_ok());
    let args = Args::try_parse_from(["jcode", "--home", kcode_home.to_str().unwrap()]).unwrap();
    assert_eq!(args.home.as_deref(), Some(kcode_home.to_str().unwrap()));

    assert!(apply_home(Some(temp.path().join("not-there").to_str().unwrap())).is_err());

    let dir = temp.path().join("kcode-home");
    std::fs::create_dir_all(&dir).unwrap();
    let jcode_dir = temp.path().join(".jcode");
    std::fs::create_dir_all(&jcode_dir).unwrap();
    assert!(apply_home(Some(&jcode_dir.to_string_lossy())).is_err());
    assert!(apply_home(Some(dir.to_str().unwrap())).is_ok());
    assert_eq!(std::env::var("KCODE_HOME").unwrap(), dir.to_str().unwrap());
}

#[test]
fn saved_home_restores_set_and_unset_values() {
    let _guard = crate::storage::lock_test_env();
    let original = std::env::var_os("KCODE_HOME");

    // Unset case: guard must remove the variable again on drop.
    crate::env::remove_var("KCODE_HOME");
    {
        let _saved = SavedHome::capture();
        crate::env::set_var("KCODE_HOME", "/sentinel/unset-case");
        assert_eq!(std::env::var("KCODE_HOME").unwrap(), "/sentinel/unset-case");
    }
    assert!(
        std::env::var_os("KCODE_HOME").is_none(),
        "guard must leave KCODE_HOME unset when it captured no value"
    );

    // Set case: guard must restore the exact prior value on drop.
    crate::env::set_var("KCODE_HOME", "/sentinel/prior-value");
    {
        let _saved = SavedHome::capture();
        crate::env::set_var("KCODE_HOME", "/sentinel/overwritten");
    }
    assert_eq!(
        std::env::var("KCODE_HOME").unwrap(),
        "/sentinel/prior-value",
        "guard must restore the captured KCODE_HOME value"
    );

    match original {
        Some(value) => crate::env::set_var("KCODE_HOME", value),
        None => crate::env::remove_var("KCODE_HOME"),
    }
}

#[test]
fn data_dir_is_honored_by_application_path_and_no_flag_is_unchanged() {
    let _guard = crate::storage::lock_test_env();
    let _saved_home = SavedHome::capture();
    let temp = tempfile::tempdir().unwrap();
    let default_home = temp.path().join("default-home");
    std::fs::create_dir_all(&default_home).unwrap();
    let flag_home = temp.path().join("flag-home");
    std::fs::create_dir_all(&flag_home).unwrap();

    crate::env::set_var("KCODE_HOME", &default_home);
    let args = Args::parse_from([
        "jcode",
        "--data-dir",
        flag_home.to_str().unwrap(),
        "version",
    ]);
    apply_startup_home(&args).unwrap();
    assert_eq!(
        std::env::var("KCODE_HOME").unwrap(),
        flag_home.to_str().unwrap()
    );
    assert_eq!(
        crate::storage::jcode_dir().unwrap().canonicalize().unwrap(),
        flag_home.canonicalize().unwrap()
    );

    crate::env::set_var("KCODE_HOME", &default_home);
    let no_flag = Args::parse_from(["jcode", "version"]);
    assert_eq!(no_flag.home, None);
    apply_startup_home(&no_flag).unwrap();
    assert_eq!(
        std::env::var("KCODE_HOME").unwrap(),
        default_home.to_str().unwrap()
    );
    assert_eq!(
        crate::storage::jcode_dir().unwrap().canonicalize().unwrap(),
        default_home.canonicalize().unwrap()
    );
}
