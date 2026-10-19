//! Supplemental CC-only normal reuse check. Run this EXACT filter in its own
//! test process with --test-threads=1 and an outer timeout. No lifecycle fixtures
//! or hand-created completion evidence; every acquisition uses the public API.
use crate::x11_credentials::{Cleanup, Request, MAX_PENDING_CHILDREN};
use std::time::{Duration, Instant};

#[test]
#[ignore = "CC: dedicated process, normal native acquisition beyond capacity"]
fn normal_acquisitions_retire_resources_and_reuse_all_permits() {
    assert_eq!(
        super::lifecycle_owner_view(0).0,
        0,
        "PERMIT_FIXTURE_INFRA: shared host"
    );
    let directory = std::env::temp_dir().join(format!("credential-permit-{}", std::process::id()));
    std::fs::create_dir(&directory).expect("PERMIT_FIXTURE_INFRA: owned directory");
    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let directory = Directory(directory);
    let path = directory.0.join("synthetic");
    let mut bytes = 256u16.to_be_bytes().to_vec();
    for field in [
        &b"permit-host"[..],
        &b"100001"[..],
        &b"MIT-MAGIC-COOKIE-1"[..],
        &[0x57; 16][..],
    ] {
        bytes.extend_from_slice(&(field.len() as u16).to_be_bytes());
        bytes.extend_from_slice(field);
    }
    std::fs::write(&path, bytes).expect("PERMIT_FIXTURE_INFRA: synthetic authority");
    for _ in 0..2 * MAX_PENDING_CHILDREN {
        let result = crate::x11_credentials::acquire(
            Request {
                authority_path: Some(&path),
                hostname: b"permit-host",
                display_number: "100001",
            },
            Instant::now() + Duration::from_secs(2),
        )
        .expect("normal acquisition must succeed, not remain quarantined or reject all");
        assert_eq!(result.cleanup, Cleanup::Reaped);
        assert!(result.cookie.unwrap().as_bytes() == &[0x57; 16]);
        // No reaper grace is needed: actual successful completion precedes return,
        // and foreground resources must retire before its admission permit.
        assert_eq!(super::lifecycle_owner_view(0).0, 0);
    }
    println!("CREDENTIAL_PERMIT_NORMAL_REUSE_OK");
}
