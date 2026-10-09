use super::pipe;
use rpa_desktop_feedback::{
    process::{PrivateSpawner, ProcessControl, RendererCommand},
    Diagnostic,
};
use std::{
    io::{Read, Write},
    process::Child,
};

pub(crate) struct ChildControl {
    child: Child,
    reaped: bool,
}
impl ProcessControl for ChildControl {
    fn poll_exit(&mut self) -> Result<bool, Diagnostic> {
        if self.reaped {
            return Ok(true);
        }
        self.reaped = self
            .child
            .try_wait()
            .map_err(|_| Diagnostic::ReapFailed)?
            .is_some();
        Ok(self.reaped)
    }
    fn request_terminate(&mut self) -> Result<(), Diagnostic> {
        if self.poll_exit()? {
            return Ok(());
        }
        self.child.kill().map_err(|_| Diagnostic::TerminateFailed)
    }
    fn poll_reaped(&mut self) -> Result<bool, Diagnostic> {
        self.poll_exit()
    }
}
pub(crate) struct Pipes {
    pub reader: Box<dyn Read + Send>,
    pub writer: Box<dyn Write + Send>,
}
#[derive(Default)]
pub(crate) struct Spawner {
    pub pipes: Option<Pipes>,
}
impl PrivateSpawner for Spawner {
    type Process = ChildControl;
    fn spawn_private(&mut self, renderer: &RendererCommand) -> Result<ChildControl, Diagnostic> {
        let mut command = renderer.to_std_command();
        // Do not pass Host authentication/API environment to the UI process.
        command.env_clear();
        for key in [
            "PATH",
            "HOME",
            "USERPROFILE",
            "SystemRoot",
            "WINDIR",
            "TEMP",
            "TMP",
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "XDG_RUNTIME_DIR",
            "LANG",
            "LC_ALL",
            "__CF_USER_TEXT_ENCODING",
        ] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        let (child, reader, writer) =
            pipe::spawn(&mut command).map_err(|_| Diagnostic::SpawnFailed)?;
        self.pipes = Some(Pipes {
            reader: Box::new(reader),
            writer: Box::new(writer),
        });
        Ok(ChildControl {
            child,
            reaped: false,
        })
    }
}
