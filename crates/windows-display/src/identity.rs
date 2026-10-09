//! Exact device-identity matching, never a short hash of monitor facts.
use crate::DisplayError;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Identity {
    pub handle: usize,
    pub device: String,
    pub adapter: String,
    pub interfaces: Vec<String>,
}
#[derive(Debug, Default)]
pub(crate) struct IdRegistry {
    next: u64,
    active: BTreeMap<Identity, String>,
}
impl IdRegistry {
    /// A failed enumeration is not evidence of continued attachment. Retire all
    /// tokens (without resetting the counter); recovery cannot revive old handles.
    pub fn invalidate(&mut self) {
        self.active.clear();
    }
    /// Caller supplies a COMPLETE successful enumeration. Sorting exact identities
    /// makes initial assignment independent of callback ordering. Missing devices
    /// are retired; observed replug gets a new ID, even if handles are reused.
    pub fn reconcile(&mut self, identities: &[Identity]) -> Result<Vec<String>, DisplayError> {
        let mut handles = BTreeSet::new();
        let mut devices = BTreeSet::new();
        let mut ordered = identities.to_vec();
        ordered.sort();
        for identity in &ordered {
            if !handles.insert(identity.handle) || !devices.insert(identity.device.clone()) {
                return Err(DisplayError::DuplicateNativeIdentity);
            }
        }
        let mut next = self.next;
        let mut active = BTreeMap::new();
        for identity in ordered {
            let id = if let Some(id) = self.active.get(&identity) {
                id.clone()
            } else {
                next = next.checked_add(1).ok_or(DisplayError::IdentityExhausted)?;
                format!("win-gdi-{next:016x}")
            };
            active.insert(identity, id);
        }
        let ids = identities.iter().map(|i| active[i].clone()).collect();
        self.next = next;
        self.active = active;
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id(handle: usize, device: &str) -> Identity {
        Identity {
            handle,
            device: device.into(),
            adapter: "adapter".into(),
            interfaces: vec![format!("interface-{device}")],
        }
    }
    #[test]
    fn sorted_exact_identity_assignments_and_wire_bounds() {
        let (a, b) = (id(1, "a"), id(2, "b"));
        let mut registry = IdRegistry::default();
        assert_eq!(
            registry.reconcile(&[b.clone(), a.clone()]).unwrap(),
            ["win-gdi-0000000000000002", "win-gdi-0000000000000001"]
        );
        assert_eq!(
            registry.reconcile(&[a, b]).unwrap(),
            ["win-gdi-0000000000000001", "win-gdi-0000000000000002"]
        );
    }
    #[test]
    fn identity_change_or_observed_replug_does_not_reuse_token() {
        let mut registry = IdRegistry::default();
        let a = id(1, "a");
        registry.reconcile(&[a.clone()]).unwrap();
        let mut changed = a.clone();
        changed.interfaces = vec!["different panel".into()];
        assert_eq!(
            registry.reconcile(&[changed]).unwrap(),
            ["win-gdi-0000000000000002"]
        );
        registry.reconcile(&[]).unwrap();
        assert_eq!(
            registry.reconcile(&[a]).unwrap(),
            ["win-gdi-0000000000000003"]
        );
    }
    #[test]
    fn failed_enumeration_then_recovery_cannot_revive_old_tokens() {
        let mut registry = IdRegistry::default();
        let a = id(1, "a");
        registry.reconcile(&[a.clone()]).unwrap();
        registry.invalidate();
        assert_eq!(
            registry.reconcile(&[a]).unwrap(),
            ["win-gdi-0000000000000002"]
        );
    }
    #[test]
    fn duplicate_and_exhaustion_fail_transactionally() {
        let mut registry = IdRegistry::default();
        let a = id(1, "a");
        assert_eq!(
            registry.reconcile(&[a.clone(), a.clone()]).unwrap_err(),
            DisplayError::DuplicateNativeIdentity
        );
        assert_eq!(registry.next, 0);
        registry.next = u64::MAX;
        assert_eq!(
            registry.reconcile(&[a]).unwrap_err(),
            DisplayError::IdentityExhausted
        );
        assert!(registry.active.is_empty());
    }
}
