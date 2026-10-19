//! Runtime ownership checks. This fixture NEVER signals or restarts a server.
use std::{fs, os::unix::fs::MetadataExt, path::Path};

fn process(pid: u32) -> (u32, u64) {
    let stat =
        fs::read_to_string(format!("/proc/{pid}/stat")).expect("read fixture process identity");
    let fields: Vec<_> = stat
        .rsplit_once(") ")
        .expect("proc stat format")
        .1
        .split_whitespace()
        .collect();
    (fields[1].parse().unwrap(), fields[19].parse().unwrap())
}

pub(super) fn require_owned_server() -> String {
    assert_eq!(
        std::env::var("RPA_X11_HOTKEY_QUALIFY").as_deref(),
        Ok("owned-disposable-default-map-v1"),
        "explicit disposable input-fixture opt-in required"
    );
    let display = std::env::var("DISPLAY").expect("explicit DISPLAY");
    super::super::super::x11_plan::check_session(
        Some(&display),
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
    )
    .unwrap();
    let number = super::super::super::x11_auth::local_display_number(&display).unwrap();
    assert_eq!(
        display,
        format!(":{number}"),
        "fixture requires canonical local DISPLAY, no screen suffix"
    );
    let pid: u32 = std::env::var("RPA_X11_HOTKEY_SERVER_PID")
        .expect("runner-owned Xvfb PID")
        .parse()
        .unwrap();
    let ticks: u64 = std::env::var("RPA_X11_HOTKEY_SERVER_START_TICKS")
        .expect("runner-recorded Xvfb identity")
        .parse()
        .unwrap();
    let owner: u32 = std::env::var("RPA_X11_HOTKEY_RUNNER_PID")
        .expect("server's live parent runner")
        .parse()
        .unwrap();
    assert!(pid > 1 && owner > 1);
    assert_eq!(
        process(pid),
        (owner, ticks),
        "stale PID or server not a child of declared runner"
    );
    let mut ancestor = std::process::id();
    for _ in 0..64 {
        if ancestor == owner || ancestor <= 1 {
            break;
        }
        ancestor = process(ancestor).0;
    }
    assert_eq!(
        ancestor, owner,
        "runner must also be a live ancestor of this test process"
    );
    let exe = fs::read_link(format!("/proc/{pid}/exe")).expect("server executable");
    assert_eq!(
        exe.file_name().unwrap(),
        "Xvfb",
        "only owned Xvfb is authorized"
    );
    let command = fs::read(format!("/proc/{pid}/cmdline")).unwrap();
    let args: Vec<_> = command.split(|byte| *byte == 0).collect();
    assert_eq!(
        Path::new(std::str::from_utf8(args[0]).unwrap())
            .file_name()
            .unwrap(),
        "Xvfb"
    );
    assert!(
        args.contains(&display.as_bytes()),
        "owned PID does not match DISPLAY"
    );
    assert!(
        args.contains(&b"-noreset".as_slice()),
        "server must not reset between fixture connections"
    );
    // A matching command line alone is not proof that this PID owns the
    // endpoint. Match its live socket fd inode to this exact local X listener.
    let own_uid = fs::metadata("/proc/self").unwrap().uid();
    assert_eq!(fs::metadata(format!("/proc/{pid}")).unwrap().uid(), own_uid);
    assert_eq!(
        fs::metadata(format!("/proc/{owner}")).unwrap().uid(),
        own_uid
    );
    let fds: Vec<_> = fs::read_dir(format!("/proc/{pid}/fd"))
        .unwrap()
        .filter_map(|entry| fs::read_link(entry.ok()?.path()).ok())
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    let sockets = fs::read_to_string("/proc/net/unix").unwrap();
    let name = format!("/tmp/.X11-unix/X{number}");
    assert!(
        sockets.lines().any(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            fields.len() >= 8 && (fields[7] == name || fields[7] == format!("@{name}"))
            && fields[3] == "00010000" // listening socket, not another client
            && fds.contains(&format!("socket:[{}]", fields[6]))
        }),
        "declared Xvfb does not own the DISPLAY listener"
    );
    display
}
