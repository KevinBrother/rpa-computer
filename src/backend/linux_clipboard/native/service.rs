//! Selection event service, always outside the short preparation/verify grab.
use super::server::{atom, before_clear, reply, window, xerror, State, EVENT_BUDGET, MAX_QUEUED};
use crate::backend::linux_clipboard::{
    policy::error,
    protocol::{MultipleProperty, PropertyData, Request, Write, MAX_MULTIPLE_PAIRS},
};
use crate::backend::BackendError;
use std::time::Instant;
use xcb::{x, Event, Xid};

impl State {
    fn handle_clear(&mut self, event: &x::SelectionClearEvent) {
        if event.response_type() & 0x80 == 0
            && event.owner().resource_id() == self.owner
            && event.selection().resource_id() == self.atoms.clipboard
        {
            if let Some(data) = self.data.as_mut() {
                if !before_clear(event.time(), data.acquired()) {
                    data.ownership_lost();
                    // Zero cannot authorize a receipt, even if another client sets
                    // this window back as owner without publishing a new payload.
                    self.generation = 0;
                }
            }
        }
    }
    /// Drain a bounded queue without servicing client requests inside a grab.
    pub fn queue_pending(&mut self) -> Result<(), BackendError> {
        for _ in 0..MAX_QUEUED {
            let Some(event) = self.ctx.c.poll_for_event().map_err(xerror)? else {
                return Ok(());
            };
            match &event {
                Event::X(x::Event::SelectionClear(clear)) => self.handle_clear(clear),
                Event::X(x::Event::SelectionRequest(_)) => {
                    if self.deferred.len() >= MAX_QUEUED {
                        return Err(error("clipboard request queue capacity exceeded"));
                    }
                    self.deferred.push_back(event);
                }
                _ => {}
            }
        }
        Err(error(
            "clipboard event budget exhausted; no authorization granted",
        ))
    }
    pub fn server_time(&mut self, deadline: Instant) -> Result<u32, BackendError> {
        // Consume old property events before issuing the new timestamp request.
        self.queue_pending()?;
        let cookie = self.ctx.c.send_request_checked(&x::ChangeProperty {
            mode: x::PropMode::Replace,
            window: window(self.owner),
            property: atom(self.clock_property),
            r#type: x::ATOM_INTEGER,
            data: &[0u32],
        });
        self.ctx.checked(cookie, deadline)?;
        loop {
            self.ctx.live(deadline)?;
            match self.ctx.c.poll_for_event().map_err(xerror)? {
                Some(Event::X(x::Event::PropertyNotify(event)))
                    if event.response_type() & 0x80 == 0
                        && event.window().resource_id() == self.owner
                        && event.atom().resource_id() == self.clock_property
                        && event.state() == x::Property::NewValue
                        && event.time() != 0 =>
                {
                    return Ok(event.time())
                }
                Some(event) => self.event(event, deadline)?,
                None => std::thread::park_timeout(std::time::Duration::from_millis(1)),
            }
        }
    }
    pub fn event(&mut self, event: Event, deadline: Instant) -> Result<(), BackendError> {
        self.ctx.live(deadline)?;
        match event {
            Event::X(x::Event::SelectionClear(clear)) => self.handle_clear(&clear),
            Event::X(x::Event::SelectionRequest(event)) if event.response_type() & 0x80 == 0 => {
                self.serve_request(
                    Request {
                        owner: event.owner().resource_id(),
                        requestor: event.requestor().resource_id(),
                        selection: event.selection().resource_id(),
                        target: event.target().resource_id(),
                        property: event.property().resource_id(),
                        time: event.time(),
                    },
                    deadline,
                )?;
            }
            _ => {}
        }
        Ok(())
    }
    pub fn pump(&mut self) -> Result<(), BackendError> {
        // Command loop runs between batches: event floods cannot starve cancellation.
        let deadline = Instant::now() + EVENT_BUDGET;
        for _ in 0..16 {
            self.ctx.live(deadline)?;
            let event = match self.deferred.pop_front() {
                Some(e) => Some(e),
                None => self.ctx.c.poll_for_event().map_err(xerror)?,
            };
            let Some(event) = event else {
                break;
            };
            self.event(event, deadline)?;
        }
        Ok(())
    }
    fn serve_request(&mut self, request: Request, deadline: Instant) -> Result<(), BackendError> {
        if request.selection != self.atoms.clipboard
            || request.owner != self.owner
            || request.requestor == 0
        {
            return self.notify(
                Request {
                    property: 0,
                    ..request
                },
                deadline,
            );
        }
        if self.ctx.selection_owner(self.atoms.clipboard, deadline)? != self.owner {
            if let Some(data) = self.data.as_mut() {
                data.ownership_lost();
            }
            self.generation = 0;
        }
        let eligible = self.data.as_ref().is_some_and(|d| d.eligible(request));
        let pairs = if eligible && request.target == self.atoms.multiple && request.property != 0 {
            // This is the requestor's protocol metadata, never prior clipboard data.
            match reply(
                &self.ctx,
                self.ctx.c.send_request(&x::GetProperty {
                    delete: false,
                    window: window(request.requestor),
                    property: atom(request.property),
                    r#type: atom(self.atoms.atom_pair),
                    long_offset: 0,
                    long_length: (MAX_MULTIPLE_PAIRS * 2) as u32,
                }),
                deadline,
            ) {
                Ok(p) => Some(MultipleProperty {
                    type_atom: p.r#type().resource_id(),
                    format: p.format(),
                    words: if p.format() == 32 {
                        p.value::<u32>().to_vec()
                    } else {
                        vec![]
                    },
                    bytes_after: p.bytes_after(),
                }),
                Err(_) => {
                    self.ctx.live(deadline)?;
                    None
                }
            }
        } else {
            None
        };
        let Some(data) = self.data.as_ref() else {
            return self.notify(
                Request {
                    property: 0,
                    ..request
                },
                deadline,
            );
        };
        let mut plan = data.reply(request, pairs);
        let multiple = request.target == self.atoms.multiple && plan.notify.property != 0;
        let mut failed = Vec::new();
        let last = plan.writes.len().saturating_sub(1);
        for (index, write) in plan.writes.iter_mut().enumerate() {
            self.ctx.live(deadline)?;
            if multiple && index == last {
                if let PropertyData::Words(words) = &mut write.data {
                    for pair in words.chunks_exact_mut(2) {
                        if failed.contains(&pair[1]) {
                            pair[1] = 0;
                        }
                    }
                }
            }
            if self.write(write, deadline).is_err() {
                self.ctx.live(deadline)?; // transport loss is terminal, bad requestor/property is not
                if multiple && index != last {
                    failed.push(write.property);
                } else {
                    plan.notify.property = 0;
                }
            }
        }
        self.notify(plan.notify, deadline)
    }
    fn write(&self, write: &Write, deadline: Instant) -> Result<(), BackendError> {
        let c = &self.ctx.c;
        let cookie = match &write.data {
            PropertyData::Bytes(data) => {
                debug_assert_eq!(write.format, 8);
                c.send_request_checked(&x::ChangeProperty {
                    mode: x::PropMode::Replace,
                    window: window(write.window),
                    property: atom(write.property),
                    r#type: atom(write.type_atom),
                    data,
                })
            }
            PropertyData::Words(data) => {
                debug_assert_eq!(write.format, 32);
                c.send_request_checked(&x::ChangeProperty {
                    mode: x::PropMode::Replace,
                    window: window(write.window),
                    property: atom(write.property),
                    r#type: atom(write.type_atom),
                    data,
                })
            }
        };
        self.ctx.checked(cookie, deadline)
    }
    fn notify(&self, notify: Request, deadline: Instant) -> Result<(), BackendError> {
        if notify.requestor == 0 {
            return Ok(());
        }
        self.ctx.live(deadline)?;
        let event = x::SelectionNotifyEvent::new(
            notify.time,
            window(notify.requestor),
            atom(notify.selection),
            atom(notify.target),
            atom(notify.property),
        );
        let cookie = self.ctx.c.send_request_checked(&x::SendEvent {
            propagate: false,
            destination: x::SendEventDest::Window(window(notify.requestor)),
            event_mask: x::EventMask::NO_EVENT,
            event: &event,
        });
        // Destroyed requestors must not kill an otherwise healthy clipboard owner.
        if self.ctx.checked(cookie, deadline).is_err() {
            self.ctx.live(deadline)?;
        }
        Ok(())
    }
}
