//! Generic single-owner primitive used by REAL GDI resources; tests invoke only
//! Rust closures, never GetDC/DeleteObject/other desktop APIs.
pub(crate) struct Owner<T: Copy, R: FnMut(T) -> bool> {
    value: Option<T>,
    release: R,
}
impl<T: Copy, R: FnMut(T) -> bool> Owner<T, R> {
    pub fn new(value: T, release: R) -> Self {
        Self {
            value: Some(value),
            release,
        }
    }
    pub fn get(&self) -> T {
        self.value.expect("resource is open before explicit close")
    }
    pub fn close(&mut self) -> bool {
        if let Some(value) = self.value {
            if !(self.release)(value) {
                return false;
            }
            self.value = None;
        }
        true
    }
}
impl<T: Copy, R: FnMut(T) -> bool> Drop for Owner<T, R> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    #[test]
    fn explicit_close_and_drop_release_exactly_once() {
        let n = Cell::new(0);
        {
            let mut owner = Owner::new(7, |handle| {
                assert_eq!(handle, 7);
                n.set(n.get() + 1);
                true
            });
            assert_eq!(owner.get(), 7);
            assert!(owner.close());
            assert!(owner.close());
        }
        assert_eq!(n.get(), 1);
    }
    #[test]
    fn failed_close_retains_ownership_for_drop_retry() {
        let n = Cell::new(0);
        {
            let mut owner = Owner::new(1, |_| {
                n.set(n.get() + 1);
                n.get() > 1
            });
            assert!(!owner.close());
        }
        assert_eq!(n.get(), 2);
    }
    #[test]
    fn early_exit_unwinds_selection_then_dc_then_bitmap_then_screen() {
        let events = RefCell::new(Vec::new());
        let fail = || -> Result<(), ()> {
            let _screen = Owner::new("screen", |s| {
                events.borrow_mut().push(s);
                true
            });
            let _bitmap = Owner::new("bitmap", |s| {
                events.borrow_mut().push(s);
                true
            });
            let _memory = Owner::new("memory_dc", |s| {
                events.borrow_mut().push(s);
                true
            });
            let _selected = Owner::new("restore_selection", |s| {
                events.borrow_mut().push(s);
                true
            });
            Err(())
        };
        assert_eq!(fail(), Err(()));
        assert_eq!(
            *events.borrow(),
            ["restore_selection", "memory_dc", "bitmap", "screen"]
        );
    }
}
