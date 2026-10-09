use super::*;

#[test]
fn embedded_assets_materialize_exact_bytes() {
    let assets = Fixtures::new();
    for &(name, expected) in EMBEDDED {
        assert_eq!(fs::read(assets.path(name)).unwrap(), expected);
    }
}

#[test]
fn independent_assets_are_isolated_and_drop_only_their_directory() {
    let first = Fixtures::new();
    let second = Fixtures::new();
    let first_root = first.root.clone();
    let second_root = second.root.clone();
    assert_ne!(first_root, second_root);
    drop(first);
    assert!(!first_root.exists());
    assert!(second.path("good/ca.pem").is_file());
    drop(second);
    assert!(!second_root.exists());
}

#[test]
fn unwind_cleans_owned_assets() {
    let mut root = None;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let assets = Fixtures::new();
        root = Some(assets.root.clone());
        panic!("simulated test failure");
    }));
    assert!(result.is_err());
    assert!(!root.unwrap().exists());
}

#[test]
fn missing_fixture_is_explicit_harness_failure() {
    let assets = Fixtures::new();
    fs::remove_file(assets.path("good/ca.pem")).unwrap();
    let error = std::panic::catch_unwind(|| assets.path("good/ca.pem")).unwrap_err();
    let message = error.downcast_ref::<String>().unwrap();
    assert!(message.contains("test harness failure"), "{message}");
}

#[test]
fn create_file_rejects_overwrite_and_path_escape() {
    let assets = Fixtures::new();
    let path = assets.create_file("owned.txt", b"original");
    assert!(std::panic::catch_unwind(|| assets.create_file("owned.txt", b"overwrite")).is_err());
    assert_eq!(fs::read(path).unwrap(), b"original");
    for name in [
        "",
        "../escaped",
        "good/server.key",
        "..",
        "/absolute",
        "x\\y",
        "x:y",
    ] {
        assert!(std::panic::catch_unwind(|| assets.create_file(name, b"no")).is_err());
    }
}
