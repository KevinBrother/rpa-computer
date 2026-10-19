use crate::observe::Observer;
use serde_json::{json, Value};
use std::{collections::HashSet, io::Write};
use xcb::x;
impl Observer {
    pub fn capture(&mut self, pid: u32, role: &str, tag: &str) -> Result<Value, &'static str> {
        let windows = self.windows(pid)?;
        let candidates: Vec<_> = windows.iter().filter(|w| w["role"] == role).collect();
        if candidates.is_empty() {
            return Ok(json!({"present":false}));
        }
        if candidates.len() != 1 {
            return Err("ambiguous role");
        }
        let w = candidates[0];
        if w["map_state"] != 2 {
            return Ok(json!({"present":true,"viewable":false,"window":w["id"]}));
        }
        let x = w["x"].as_i64().ok_or("x")?;
        let y = w["y"].as_i64().ok_or("y")?;
        let width = w["width"].as_u64().ok_or("width")?;
        let height = w["height"].as_u64().ok_or("height")?;
        if x < 0
            || y < 0
            || width == 0
            || height == 0
            || x + width as i64 > 1920
            || y + height as i64 > 1080
            || width * height > 64 * 1024
        {
            return Err("pixel bounds");
        }
        let image = self
            .conn
            .wait_for_reply(self.conn.send_request(&x::GetImage {
                format: x::ImageFormat::ZPixmap,
                drawable: x::Drawable::Window(self.root),
                x: x as i16,
                y: y as i16,
                width: width as u16,
                height: height as u16,
                plane_mask: u32::MAX,
            }))
            .map_err(|_| "image")?;
        if image.depth() != self.depth || image.data().len() != width as usize * height as usize * 4
        {
            return Err("image format");
        }
        let mut colors = HashSet::new();
        let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
        let mut fingerprint = 0xcbf29ce484222325u64;
        let mut pattern = 0usize;
        for chunk in image.data().chunks_exact(4) {
            let raw: [u8; 4] = chunk.try_into().map_err(|_| "pixel")?;
            let pixel = if self.little {
                u32::from_le_bytes(raw)
            } else {
                u32::from_be_bytes(raw)
            };
            let channels = self
                .masks
                .map(|m| ((pixel & m) >> m.trailing_zeros()) as u8);
            let packed = (u32::from(channels[0]) << 16)
                | (u32::from(channels[1]) << 8)
                | u32::from(channels[2]);
            if packed == 0x203040 || packed == 0x607080 {
                pattern += 1;
            }
            if colors.len() < 4096 {
                colors.insert(packed);
            }
            for byte in channels {
                fingerprint = (fingerprint ^ u64::from(byte)).wrapping_mul(0x100000001b3);
                rgb.push(byte);
            }
        }
        let mut file = None;
        if !tag.is_empty() {
            if self.captures >= 32 {
                return Err("capture budget");
            }
            self.captures += 1;
            let name = format!("{tag}-{role}.ppm");
            let mut output = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(self.dir.join(&name))
                .map_err(|_| "image create")?;
            write!(output, "P6\n{width} {height}\n255\n").map_err(|_| "image write")?;
            output.write_all(&rgb).map_err(|_| "image write")?;
            file = Some(name);
        }
        Ok(
            json!({"present":true,"viewable":true,"window":w["id"],"fingerprint":format!("{fingerprint:016x}"),"colors":colors.len(),"pattern_pixels":pattern,"pixels":width*height,"file":file,"geometry":[x,y,width as i64,height as i64],"focus":self.focus()?,"events":self.events()?}),
        )
    }
}
