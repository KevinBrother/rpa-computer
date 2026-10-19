use serde_json::Value;
pub struct Command {
    pub op: String,
    pub pid: u32,
    pub role: String,
    pub tag: String,
    pub x: i16,
    pub y: i16,
}
pub fn parse(bytes: &[u8]) -> Result<(u64, Command), &'static str> {
    if bytes.len() > 4096 || bytes.last() != Some(&b'\n') {
        return Err("request limit");
    }
    let v: Value = serde_json::from_slice(bytes).map_err(|_| "request JSON")?;
    let id = v["id"].as_u64().ok_or("request id")?;
    let op = v["op"].as_str().ok_or("op")?;
    if ![
        "inspect", "capture", "move", "press", "release", "events", "quit",
    ]
    .contains(&op)
    {
        return Err("op");
    }
    let pid = if ["inspect", "capture"].contains(&op) {
        u32::try_from(v["pid"].as_u64().ok_or("pid")?).map_err(|_| "pid")?
    } else {
        0
    };
    if ["inspect", "capture"].contains(&op) && pid == 0 {
        return Err("pid");
    }
    let role = v["role"].as_str().unwrap_or("");
    let tag = v["tag"].as_str().unwrap_or("");
    if op == "capture"
        && (!["status", "stop", "ring"].contains(&role)
            || tag.len() > 48
            || !tag.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
    {
        return Err("capture arguments");
    }
    let (x, y) = if op == "move" {
        let x = v["x"].as_i64().ok_or("x")?;
        let y = v["y"].as_i64().ok_or("y")?;
        if !(0..1920).contains(&x) || !(0..1080).contains(&y) {
            return Err("owned root bounds");
        }
        (x as i16, y as i16)
    } else {
        (0, 0)
    };
    Ok((
        id,
        Command {
            op: op.into(),
            pid,
            role: role.into(),
            tag: tag.into(),
            x,
            y,
        },
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reject_remote_coordinates() {
        assert!(parse(b"{\"id\":1,\"op\":\"move\",\"x\":-1,\"y\":0}\n").is_err());
    }
    #[test]
    fn reject_unsafe_image_path() {
        assert!(parse(
            b"{\"id\":1,\"op\":\"capture\",\"pid\":2,\"role\":\"stop\",\"tag\":\"../x\"}\n"
        )
        .is_err());
    }
    #[test]
    fn reject_unterminated_request() {
        assert!(parse(b"{\"id\":1,\"op\":\"quit\"}").is_err());
    }
    #[test]
    fn reject_unknown_operation() {
        assert!(parse(b"{\"id\":1,\"op\":\"shell\"}\n").is_err());
    }
    #[test]
    fn accepts_bounded_motion() {
        let (_, c) = parse(b"{\"id\":1,\"op\":\"move\",\"x\":1919,\"y\":1079}\n").unwrap();
        assert_eq!((c.x, c.y), (1919, 1079));
    }
}
