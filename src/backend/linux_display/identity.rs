//! Active attachment identities survive reorder/geometry changes, not an
//! observed disconnect or failed enumeration. Never revive retired XIDs.
use rpa_display_topology::DisplayDescriptor;
use rpa_windows_display::DisplayError;
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Identities {
    next: u64,
    active: BTreeMap<String, String>,
}
impl Identities {
    pub fn invalidate(&mut self) {
        self.active.clear();
    }
    pub fn reconcile(
        &mut self,
        descriptors: &[DisplayDescriptor],
    ) -> Result<Vec<DisplayDescriptor>, DisplayError> {
        let mut ordered = descriptors.to_vec();
        ordered.sort_by(|a, b| a.id.cmp(&b.id));
        if ordered.windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err(DisplayError::DuplicateNativeIdentity);
        }
        let mut next = self.next;
        let mut active = BTreeMap::new();
        for descriptor in &mut ordered {
            let raw = descriptor.id.clone();
            let public = match self.active.get(&raw) {
                Some(id) => id.clone(),
                None => {
                    next = next.checked_add(1).ok_or(DisplayError::IdentityExhausted)?;
                    format!("{raw}:a{next:016x}")
                }
            };
            if public.len() > 128 {
                return Err(DisplayError::InvalidDisplayId { id: public });
            }
            active.insert(raw, public.clone());
            descriptor.id = public;
        }
        self.next = next;
        self.active = active;
        Ok(ordered)
    }
    pub fn matches(&self, raw: &DisplayDescriptor, public: &DisplayDescriptor) -> bool {
        self.active.get(&raw.id).is_some_and(|id| {
            let mut expected = raw.clone();
            expected.id = id.clone();
            expected == *public
        })
    }
}
