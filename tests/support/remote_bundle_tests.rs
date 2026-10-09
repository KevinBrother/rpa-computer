use super::*;
use crate::remote_assets::Fixtures;

#[test]
fn missing_binary_is_harness_error_not_skip_or_path_search() {
    let assets = Fixtures::new();
    let exe = assets.create_file("test.exe", b"not executed");
    let error = resolve(
        "missing-host",
        "TEST_OVERRIDE",
        None,
        &exe,
        Path::new("missing-cargo-bin"),
    )
    .unwrap_err();
    assert!(error.contains("test harness failure"), "{error}");
    assert!(error.contains("TEST_OVERRIDE"), "{error}");
}

#[test]
fn explicit_override_wins_and_invalid_override_never_falls_back() {
    let assets = Fixtures::new();
    let exe = assets.create_file("test.exe", b"not executed");
    let sibling = assets.create_file(
        &format!("computer-host{}", std::env::consts::EXE_SUFFIX),
        b"sibling",
    );
    let override_bin = assets.create_file("override.exe", b"override");
    let selected = resolve(
        "computer-host",
        "TEST_OVERRIDE",
        Some(override_bin.as_os_str()),
        &exe,
        &sibling,
    )
    .unwrap();
    assert_eq!(selected, override_bin.canonicalize().unwrap());
    let absent = exe.with_file_name("absent.exe");
    let error = resolve(
        "computer-host",
        "TEST_OVERRIDE",
        Some(absent.as_os_str()),
        &exe,
        &sibling,
    )
    .unwrap_err();
    assert!(error.contains("test harness failure"), "{error}");
    assert!(error.contains("TEST_OVERRIDE"), "{error}");
    assert!(resolve(
        "computer-host",
        "TEST_OVERRIDE",
        Some(std::ffi::OsStr::new("")),
        &exe,
        &sibling
    )
    .is_err());
}

#[test]
fn sibling_precedes_existing_cargo_fallback() {
    let assets = Fixtures::new();
    let exe = assets.create_file("test.exe", b"not executed");
    let sibling = assets.create_file(
        &format!("computer-host{}", std::env::consts::EXE_SUFFIX),
        b"sibling",
    );
    let fallback = assets.create_file("cargo-host.exe", b"fallback");
    assert_eq!(
        resolve("computer-host", "TEST_OVERRIDE", None, &exe, &fallback).unwrap(),
        sibling.canonicalize().unwrap()
    );
    std::fs::remove_file(sibling).unwrap();
    assert_eq!(
        resolve("computer-host", "TEST_OVERRIDE", None, &exe, &fallback).unwrap(),
        fallback.canonicalize().unwrap()
    );
}

#[test]
fn directory_is_not_an_executable_candidate() {
    let assets = Fixtures::new();
    let exe = assets.create_file("test.exe", b"not executed");
    assert!(resolve(
        "computer-host",
        "TEST_OVERRIDE",
        Some(exe.parent().unwrap().as_os_str()),
        &exe,
        &exe
    )
    .is_err());
}
