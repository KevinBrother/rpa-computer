use super::*;
impl Native for X11Native {
    fn facts(&mut self) -> Result<DisplayFacts, UiFailure> {
        let mut facts = self.read_facts()?;
        if self.facts.is_some() {
            self.epoch = self.epoch.checked_add(1).ok_or(UiFailure::Geometry)?;
        }
        facts.epoch = self.epoch;
        self.facts = Some(facts.clone());
        Ok(facts)
    }
    fn create(&mut self) -> Result<WindowSet, UiFailure> {
        if self.windows.is_some() {
            return Err(UiFailure::AlreadyStarted);
        }
        let conn = self.conn()?;
        let ids: [x::Window; 3] = [conn.generate_id(), conn.generate_id(), conn.generate_id()];
        let set = WindowSet {
            status: ids[0].resource_id(),
            stop: ids[1].resource_id(),
            ring: ids[2].resource_id(),
            epoch: 1,
        };
        self.windows = Some(set);
        for window in ids {
            self.checked(&x::CreateWindow {
                depth: self.format.depth,
                wid: window,
                parent: self.root,
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                border_width: 0,
                class: x::WindowClass::InputOutput,
                visual: self.format.visual,
                value_list: &[
                    x::Cw::BackPixel(0),
                    x::Cw::BorderPixel(0),
                    x::Cw::OverrideRedirect(true),
                    x::Cw::EventMask(
                        x::EventMask::EXPOSURE
                            | x::EventMask::STRUCTURE_NOTIFY
                            | x::EventMask::BUTTON_PRESS
                            | x::EventMask::BUTTON_RELEASE
                            | x::EventMask::ENTER_WINDOW
                            | x::EventMask::LEAVE_WINDOW,
                    ),
                    x::Cw::Colormap(self.colormap),
                ],
            })?;
            let gc = self.conn()?.generate_id();
            self.checked(&x::CreateGc {
                cid: gc,
                drawable: x::Drawable::Window(window),
                value_list: &[x::Gc::GraphicsExposures(false)],
            })?;
            self.gcs.push(gc);
        }
        Ok(set)
    }
    fn configure(
        &mut self,
        windows: WindowSet,
        policies: &[WindowPolicy; 3],
    ) -> Result<(), UiFailure> {
        if self.windows != Some(windows) || *policies != window_policies() {
            return Err(UiFailure::InvalidWindows);
        }
        for (id, role) in [windows.status, windows.stop, windows.ring]
            .into_iter()
            .zip(["status", "stop", "ring"])
        {
            let window = Self::window(id);
            self.checked(&x::ChangeProperty {
                mode: x::PropMode::Replace,
                window,
                property: x::ATOM_WM_HINTS,
                r#type: x::ATOM_WM_HINTS,
                data: &[1u32, 0, 0, 0, 0, 0, 0, 0, 0],
            })?; // InputHint=true, input=false; no focus activation
            self.checked(&x::ChangeProperty {
                mode: x::PropMode::Replace,
                window,
                property: self.atoms.protocols,
                r#type: x::ATOM_ATOM,
                data: &[self.atoms.delete],
            })?;
            self.checked(&x::ChangeProperty {
                mode: x::PropMode::Replace,
                window,
                property: self.atoms.name,
                r#type: self.atoms.utf8,
                data: b"Desktop feedback",
            })?;
            self.checked(&x::ChangeProperty {
                mode: x::PropMode::Replace,
                window,
                property: x::ATOM_WM_CLASS,
                r#type: x::ATOM_STRING,
                data: b"desktop-feedback\0RpaDesktopFeedback\0",
            })?;
            self.checked(&x::ChangeProperty {
                mode: x::PropMode::Replace,
                window,
                property: self.atoms.pid,
                r#type: x::ATOM_CARDINAL,
                data: &[std::process::id()],
            })?;
            self.checked(&x::ChangeProperty {
                mode: x::PropMode::Replace,
                window,
                property: self.atoms.role,
                r#type: self.atoms.utf8,
                data: role.as_bytes(),
            })?;
            self.checked(&x::ChangeProperty {
                mode: x::PropMode::Replace,
                window,
                property: self.atoms.state,
                r#type: x::ATOM_ATOM,
                data: &[
                    self.atoms.above,
                    self.atoms.skip_taskbar,
                    self.atoms.skip_pager,
                ],
            })?;
            // Installed before any mapping. Stop gets its full-size region at paint.
            self.input_shape(window, &[])?;
        }
        Ok(())
    }
    fn paint(&mut self, windows: WindowSet, scene: &Scene) -> Result<(), UiFailure> {
        if self.windows != Some(windows) {
            return Err(UiFailure::InvalidWindows);
        }
        let scale = self.facts.as_ref().ok_or(UiFailure::Geometry)?.scale;
        let diameter = (44. * scale).ceil() as u32;
        let (x, y) = scene.ring_center.unwrap_or((0, 0));
        let ring = Rect {
            x: x - diameter as i32 / 2,
            y: y - diameter as i32 / 2,
            width: diameter,
            height: diameter,
        };
        let rectangles = [scene.layout.bar, scene.layout.stop, ring];
        let mut rasters = Vec::with_capacity(3);
        for (index, (rect, role)) in rectangles
            .iter()
            .zip([Role::Status, Role::Stop, Role::Ring])
            .enumerate()
        {
            let width = u16::try_from(rect.width).map_err(|_| UiFailure::Geometry)?;
            let height = u16::try_from(rect.height).map_err(|_| UiFailure::Geometry)?;
            i16::try_from(rect.x).map_err(|_| UiFailure::Geometry)?;
            i16::try_from(rect.y).map_err(|_| UiFailure::Geometry)?;
            let raster = drawing::draw(role, scene, width, height, scale, self.format)?;
            let window = Self::window([windows.status, windows.stop, windows.ring][index]);
            self.checked(&x::ConfigureWindow {
                window,
                value_list: &[
                    x::ConfigWindow::X(rect.x),
                    x::ConfigWindow::Y(rect.y),
                    x::ConfigWindow::Width(rect.width),
                    x::ConfigWindow::Height(rect.height),
                ],
            })?;
            let full = [x::Rectangle {
                x: 0,
                y: 0,
                width,
                height,
            }];
            self.checked(&shape::Rectangles {
                operation: shape::So::Set,
                destination_kind: shape::Sk::Bounding,
                ordering: x::ClipOrdering::Unsorted,
                destination_window: window,
                x_offset: 0,
                y_offset: 0,
                rectangles: if self.format.argb || role == Role::Stop {
                    &full
                } else {
                    &raster.shape
                },
            })?;
            self.input_shape(window, if role == Role::Stop { &full } else { &[] })?;
            self.upload(window, index, &raster)?;
            rasters.push(raster);
        }
        self.raster = rasters;
        self.rectangles = rectangles.to_vec();
        Ok(())
    }
    fn present_checked(&mut self, windows: WindowSet, scene: &Scene) -> Result<Receipt, UiFailure> {
        if self.windows != Some(windows) || self.raster.len() != 3 {
            return Err(UiFailure::InvalidWindows);
        }
        self.maintain_stack()?;
        for id in [windows.status, windows.stop] {
            self.checked(&x::MapWindow {
                window: Self::window(id),
            })?;
        }
        if scene.ring_center.is_some() != self.ring_mapped {
            if scene.ring_center.is_some() {
                self.checked(&x::MapWindow {
                    window: Self::window(windows.ring),
                })?;
            } else {
                self.checked(&x::UnmapWindow {
                    window: Self::window(windows.ring),
                })?;
            }
            self.ring_mapped = scene.ring_center.is_some();
        }
        for (index, id) in [windows.status, windows.stop, windows.ring]
            .into_iter()
            .enumerate()
        {
            let window = Self::window(id);
            let conn = self.conn()?;
            let attr = conn
                .wait_for_reply(conn.send_request(&x::GetWindowAttributes { window }))
                .map_err(|_| UiFailure::Native)?;
            let visible = attr.map_state() == x::MapState::Viewable;
            if !attr.override_redirect() || ((index < 2 || self.ring_mapped) && !visible) {
                return Err(UiFailure::IncompletePresentation);
            }
            let geometry = conn
                .wait_for_reply(conn.send_request(&x::GetGeometry {
                    drawable: x::Drawable::Window(window),
                }))
                .map_err(|_| UiFailure::Native)?;
            let expected = self.rectangles[index];
            if i32::from(geometry.x()) != expected.x
                || i32::from(geometry.y()) != expected.y
                || u32::from(geometry.width()) != expected.width
                || u32::from(geometry.height()) != expected.height
            {
                return Err(UiFailure::Geometry);
            }
            let shape = conn
                .wait_for_reply(conn.send_request(&shape::GetRectangles {
                    window,
                    source_kind: shape::Sk::Input,
                }))
                .map_err(|_| UiFailure::Native)?;
            if index != 1 {
                if !shape.rectangles().is_empty() {
                    return Err(UiFailure::InputShapeUnavailable);
                }
            } else {
                let actual = shape.rectangles();
                if actual.len() != 1
                    || actual[0].x != 0
                    || actual[0].y != 0
                    || u32::from(actual[0].width) != expected.width
                    || u32::from(actual[0].height) != expected.height
                {
                    return Err(UiFailure::InputShapeUnavailable);
                }
            }
        }
        if self.grant != scene.control_session || self.grant_revision != scene.control_revision {
            use xcb::Cookie;
            let conn = self.conn()?;
            let cookie = conn.send_request(&x::GetInputFocus {});
            let serial = cookie.sequence() as u32;
            conn.wait_for_reply(cookie).map_err(|_| UiFailure::Native)?;
            self.grant = scene.control_session.clone();
            self.grant_serial = serial;
            self.grant_revision = scene.control_revision;
        }
        self.conn()?.has_error().map_err(|_| UiFailure::Native)?;
        Ok(Receipt {
            server_checked: true,
            status_viewable: true,
            stop_viewable: true,
            ring_initialized: true,
            window_epoch: windows.epoch,
            topology_epoch: self.facts.as_ref().ok_or(UiFailure::Geometry)?.epoch,
        })
    }
    fn destroy_all(&mut self) {
        // Disconnect releases all owned windows/pixmaps/GCs without a final round
        // trip to an unresponsive server. No other client's resources touched.
        self.connection.take();
        self.windows = None;
        self.gcs.clear();
        self.raster.clear();
    }
}
impl X11Native {
    pub(super) fn maintain_stack(&self) -> Result<(), UiFailure> {
        let Some(w) = self.windows else {
            return Ok(());
        };
        // Preserve own relative stacking; raising each painted window separately
        // can create an Expose storm when the ring intersects the status bar.
        self.checked(&x::ConfigureWindow {
            window: Self::window(w.stop),
            value_list: &[x::ConfigWindow::StackMode(x::StackMode::Above)],
        })?;
        self.checked(&x::ConfigureWindow {
            window: Self::window(w.status),
            value_list: &[
                x::ConfigWindow::Sibling(Self::window(w.stop)),
                x::ConfigWindow::StackMode(x::StackMode::Below),
            ],
        })?;
        self.checked(&x::ConfigureWindow {
            window: Self::window(w.ring),
            value_list: &[
                x::ConfigWindow::Sibling(Self::window(w.status)),
                x::ConfigWindow::StackMode(x::StackMode::Below),
            ],
        })?;
        Ok(())
    }
    fn input_shape(&self, window: x::Window, rectangles: &[x::Rectangle]) -> Result<(), UiFailure> {
        self.checked(&shape::Rectangles {
            operation: shape::So::Set,
            destination_kind: shape::Sk::Input,
            ordering: x::ClipOrdering::Unsorted,
            destination_window: window,
            x_offset: 0,
            y_offset: 0,
            rectangles,
        })
    }
    fn upload(&self, window: x::Window, index: usize, raster: &Raster) -> Result<(), UiFailure> {
        let conn = self.conn()?;
        let pixmap = conn.generate_id();
        self.checked(&x::CreatePixmap {
            depth: self.format.depth,
            pid: pixmap,
            drawable: x::Drawable::Window(window),
            width: raster.width,
            height: raster.height,
        })?;
        let stride = usize::from(raster.width) * 4;
        let rows = (16384 / stride).max(1);
        let mut cookies = Vec::new();
        for y in (0..usize::from(raster.height)).step_by(rows) {
            let height = rows.min(usize::from(raster.height) - y);
            cookies.push(conn.send_request_checked(&x::PutImage {
                format: x::ImageFormat::ZPixmap,
                drawable: x::Drawable::Pixmap(pixmap),
                gc: self.gcs[index],
                width: raster.width,
                height: height as u16,
                dst_x: 0,
                dst_y: y as i16,
                left_pad: 0,
                depth: self.format.depth,
                data: &raster.pixels[y * stride..(y + height) * stride],
            }));
        }
        // Complete pending upload requests before marking any presentation ready.
        conn.wait_for_reply(conn.send_request(&x::GetInputFocus {}))
            .map_err(|_| UiFailure::Native)?;
        for cookie in cookies {
            conn.check_request(cookie).map_err(|_| UiFailure::Native)?;
        }
        self.checked(&x::ChangeWindowAttributes {
            window,
            value_list: &[x::Cw::BackPixmap(pixmap)],
        })?;
        self.checked(&x::ClearArea {
            exposures: false,
            window,
            x: 0,
            y: 0,
            width: raster.width,
            height: raster.height,
        })?;
        // Window background retains the server pixmap; replacing it releases the
        // old reference. Thus drawing is valid even before the first MapWindow.
        self.checked(&x::FreePixmap { pixmap })?;
        Ok(())
    }
}
