//! Test-only independent X client. Does not link or invoke renderer policy.
mod observe;
mod pixels;
mod requests;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

fn main() {
    std::panic::set_hook(Box::new(|_| {}));
    let code = match run() {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("observer failure: {error}");
            65
        }
    };
    std::process::exit(code);
}
fn emit(value: Value) -> Result<(), &'static str> {
    let data = serde_json::to_vec(&value).map_err(|_| "encode")?;
    if data.len() > 60000 {
        return Err("reply budget");
    }
    let mut out = io::stdout().lock();
    out.write_all(&data).map_err(|_| "stdout")?;
    out.write_all(b"\n").map_err(|_| "stdout")?;
    out.flush().map_err(|_| "stdout")
}
fn run() -> Result<(), &'static str> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 || args[1] != "--artifacts" {
        return Err("arguments");
    }
    let display = std::env::var("DISPLAY").map_err(|_| "display")?;
    if std::env::var("RPA_FIXTURE_OWNED_DISPLAY").ok().as_ref() != Some(&display)
        || !display.starts_with(':')
        || std::env::var_os("XAUTHORITY").is_none()
    {
        return Err("owned display marker required");
    }
    let mut observer = observe::Observer::new(std::path::PathBuf::from(&args[2]))?;
    emit(json!({"id":0,"ok":true,"result":observer.info()}))?;
    let mut reader = io::stdin().lock();
    loop {
        let mut bytes = Vec::new();
        let mut limited = (&mut reader).take(4097);
        use std::io::Read;
        if limited.read_until(b'\n', &mut bytes).map_err(|_| "stdin")? == 0 {
            break;
        }
        let (id, cmd) = requests::parse(&bytes)?;
        if cmd.op == "quit" {
            observer.release()?;
            emit(json!({"id":id,"ok":true,"result":{}}))?;
            break;
        }
        match observer.command(cmd) {
            Ok(result) => emit(json!({"id":id,"ok":true,"result":result}))?,
            Err(error) => {
                emit(json!({"id":id,"ok":false,"error":error}))?;
                return Err(error);
            }
        }
    }
    observer.release()?;
    Ok(())
}
