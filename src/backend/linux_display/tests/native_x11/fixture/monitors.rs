//! Bounded changes to monitors CREATED by this fixture, never existing ones.
use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

impl Fixture {
    pub fn split(&mut self) {
        assert!(self.monitors.is_empty());
        assert_eq!(
            self.monitor_facts(),
            self.baseline,
            "fixture baseline changed externally"
        );
        let reply = self
            .connection
            .wait_for_reply(self.connection.send_request(&randr::GetMonitors {
                window: self.root,
                get_active: true,
            }))
            .unwrap();
        let outputs = reply.monitors().next().unwrap().outputs().to_vec();
        if outputs.is_empty() {
            unavailable(
                "baseline monitor has no active output to associate with owned logical monitor",
            );
        }
        for (index, bounds, primary, outputs) in
            [(0, LEFT, true, outputs), (1, RIGHT, false, vec![])]
        {
            let name = format!(
                "RPA_DISPLAY_TEST_{}_{}_{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                index
            );
            let existing = self
                .connection
                .wait_for_reply(self.connection.send_request(&x::InternAtom {
                    only_if_exists: true,
                    name: name.as_bytes(),
                }))
                .unwrap()
                .atom();
            assert!(
                existing.is_none(),
                "refusing to reuse an existing monitor atom"
            );
            let atom = self
                .connection
                .wait_for_reply(self.connection.send_request(&x::InternAtom {
                    only_if_exists: false,
                    name: name.as_bytes(),
                }))
                .unwrap()
                .atom();
            let owned = OwnedMonitor {
                atom,
                name,
                presence: Presence::Absent,
                primary,
                bounds,
                outputs,
            };
            // Record ownership before any write, including a lost reply.
            self.monitors.push(owned);
            self.create_owned(self.monitors.len() - 1);
        }
        self.await_owned();
    }

    pub fn move_right(&mut self) {
        assert_eq!(self.monitors.len(), 2);
        // Validate the full owned topology BEFORE any destructive write.
        self.await_owned();
        let original = self.monitors[1].clone();
        assert_eq!(original.presence, Presence::Present);
        assert_eq!(original.bounds, RIGHT);
        assert!(!original.primary && original.outputs.is_empty());
        let atom_name = self
            .connection
            .wait_for_reply(self.connection.send_request(&x::GetAtomName {
                atom: original.atom,
            }))
            .expect("owned monitor atom lookup");
        assert_eq!(atom_name.name().as_bytes(), original.name.as_bytes());
        let mut changed = original.clone();
        changed.bounds.x += 8;
        assert!(
            changed.bounds.x <= RIGHT.x + 8,
            "fixture allows only one bounded mutation"
        );
        eprintln!("owned monitor replace BEGIN atom={} name={} old={:?} new={:?}; same atom/outputs/primary",
            original.atom.resource_id(), original.name, original.bounds, changed.bounds);

        // X server 21.1.12 rejects SetMonitor with an existing name even though
        // the protocol specifies replacement (README source references). Delete
        // ONLY our secondary, then recreate using the SAME atom and outputs.
        // No provider query occurs between these checked requests.
        self.monitors[1].presence = Presence::Unknown;
        match self.delete_owned(original.atom) {
            Ok(()) => self.monitors[1].presence = Presence::Absent,
            Err(error) => {
                unavailable(format!(
                    "delete owned monitor atom={} name={} state={:?}: {error:?}",
                    original.atom.resource_id(),
                    original.name,
                    self.monitors[1].presence
                ));
            }
        }
        eprintln!(
            "owned monitor delete CONFIRMED atom={} name={}",
            original.atom.resource_id(),
            original.name
        );
        changed.presence = Presence::Absent;
        self.monitors[1] = changed;
        self.create_owned(1);
        self.await_owned();
        eprintln!(
            "owned monitor replace VERIFIED atom={} name={} bounds={:?}",
            original.atom.resource_id(),
            original.name,
            self.monitors[1].bounds
        );
    }

    fn create_owned(&mut self, index: usize) {
        assert_eq!(self.monitors[index].presence, Presence::Absent);
        self.monitors[index].presence = Presence::Unknown;
        let monitor = self.monitors[index].clone();
        eprintln!(
            "owned monitor SetMonitor atom={} name={} bounds={:?} primary={} outputs={:?}",
            monitor.atom.resource_id(),
            monitor.name,
            monitor.bounds,
            monitor.primary,
            monitor.outputs
        );
        match self.set_owned(&monitor) {
            Ok(()) => self.monitors[index].presence = Presence::Present,
            Err(error) => {
                unavailable(format!(
                    "SetMonitor owned atom={} name={} state={:?}: {error:?}",
                    monitor.atom.resource_id(),
                    monitor.name,
                    self.monitors[index].presence
                ));
            }
        }
    }

    fn checked_write(&self, cookie: xcb::VoidCookieChecked) -> xcb::Result<()> {
        // check_request reports protocol errors, not connection failures. A
        // disconnected socket must never become a confirmed creation/deletion.
        self.connection.check_request(cookie)?;
        self.connection.has_error()?;
        Ok(())
    }

    fn delete_owned(&self, atom: x::Atom) -> xcb::Result<()> {
        self.checked_write(self.connection.send_request_checked(&randr::DeleteMonitor {
            window: self.root,
            name: atom,
        }))
    }

    fn set_owned(&self, monitor: &OwnedMonitor) -> xcb::Result<()> {
        let b = monitor.bounds;
        let info = randr::MonitorInfoBuf::new(
            monitor.atom,
            monitor.primary,
            false,
            b.x as i16,
            b.y as i16,
            b.width as u16,
            b.height as u16,
            200,
            160,
            &monitor.outputs,
        );
        self.checked_write(self.connection.send_request_checked(&randr::SetMonitor {
            window: self.root,
            monitorinfo: &info,
        }))
    }

    fn owned_exists(&self, atom: x::Atom) -> Result<bool, String> {
        // Include inactive monitors: cleanup cannot infer absence from the
        // active-only topology used by the capture provider.
        let reply = self
            .connection
            .wait_for_reply(self.connection.send_request(&randr::GetMonitors {
                window: self.root,
                get_active: false,
            }))
            .map_err(|error| format!("cleanup GetMonitors: {error:?}"))?;
        if reply.monitors().take(65).count() > 64 {
            return Err("cleanup monitor count exceeded fixture bound".into());
        }
        Ok(reply.monitors().any(|monitor| monitor.name() == atom))
    }

    fn await_owned(&self) {
        assert!(self
            .monitors
            .iter()
            .all(|monitor| monitor.presence == Presence::Present));
        let mut expected: Vec<_> = self
            .monitors
            .iter()
            .map(|m| {
                let mut outputs: Vec<_> = m.outputs.iter().map(Xid::resource_id).collect();
                outputs.sort_unstable();
                BaselineMonitor {
                    atom: m.atom.resource_id(),
                    primary: m.primary,
                    automatic: false,
                    bounds: m.bounds,
                    outputs,
                }
            })
            .collect();
        expected.sort_by_key(|m| m.atom);
        let deadline = Instant::now() + WAIT;
        loop {
            let actual = self.monitor_facts();
            if actual == expected {
                return;
            }
            if Instant::now() >= deadline {
                unavailable(format!("RandR did not expose exact owned nonzero-origin active monitors; expected={expected:?}, actual={actual:?}"));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn cleanup(&mut self) -> Result<(), String> {
        let mut errors = Vec::new();
        for monitor in std::mem::take(&mut self.monitors).into_iter().rev() {
            eprintln!(
                "owned monitor cleanup atom={} name={} state={:?}",
                monitor.atom.resource_id(),
                monitor.name,
                monitor.presence
            );
            let present = match monitor.presence {
                Presence::Present => true,
                Presence::Absent => false,
                Presence::Unknown => {
                    match self.owned_exists(monitor.atom) {
                        Ok(present) => {
                            eprintln!("owned monitor cleanup reconciled atom={} name={} present={present}",
                            monitor.atom.resource_id(), monitor.name);
                            present
                        }
                        Err(error) => {
                            errors.push(format!(
                                "owned monitor atom={} name={} remains UNKNOWN: {error}",
                                monitor.atom.resource_id(),
                                monitor.name
                            ));
                            continue;
                        }
                    }
                }
            };
            // Skip only a confirmed deletion or an actual all-monitor query
            // proving absence. Never turn an ignored DeleteMonitor error into
            // a cleanup success; every original write error already failed.
            if present {
                if let Err(error) = self.delete_owned(monitor.atom) {
                    errors.push(format!(
                        "DeleteMonitor atom={} name={}: {error:?}",
                        monitor.atom.resource_id(),
                        monitor.name
                    ));
                }
            }
        }
        for gc in self.gcs.drain(..).rev() {
            if let Err(e) = self
                .connection
                .check_request(self.connection.send_request_checked(&x::FreeGc { gc }))
            {
                errors.push(format!("FreeGc: {e:?}"));
            }
        }
        for window in self.windows.drain(..).rev() {
            if let Err(e) = self.connection.check_request(
                self.connection
                    .send_request_checked(&x::DestroyWindow { window }),
            ) {
                errors.push(format!("DestroyWindow: {e:?}"));
            }
        }
        // Catch verification panics here so Drop never aborts an existing unwind.
        let verified = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if !self.baseline.is_empty() {
                assert_eq!(
                    self.monitor_facts(),
                    self.baseline,
                    "fixture did not restore monitor baseline"
                );
            }
            let primary = self
                .connection
                .wait_for_reply(
                    self.connection
                        .send_request(&randr::GetOutputPrimary { window: self.root }),
                )
                .unwrap()
                .output();
            assert_eq!(
                primary, self.output_primary,
                "fixture must not change output primary"
            );
        }));
        if verified.is_err() {
            errors.push("post-cleanup topology/output-primary verification failed".into());
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup() {
            if std::thread::panicking() {
                eprintln!("FIXTURE CLEANUP FAILED: {error}");
            } else {
                panic!("FIXTURE CLEANUP FAILED: {error}");
            }
        }
    }
}
