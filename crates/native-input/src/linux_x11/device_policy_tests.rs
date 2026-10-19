use super::*;

fn device(id: u16, keys: &[u8]) -> DeviceMap {
    DeviceMap {
        device_id: id,
        rows: keys
            .iter()
            .map(|key| MappingRow {
                keycode: *key,
                core: vec![0],
                xkb: vec![0],
                is_modifier: false,
                is_pressed: false,
                has_actions: false,
            })
            .collect(),
        definitions: keys
            .iter()
            .map(|_| {
                let mut def = Definition::for_symbol(0);
                def.explicit = 0;
                def.repeat = Some(false);
                def
            })
            .collect(),
    }
}
fn leased(id: u16, lease: &Lease) -> DeviceMap {
    let mut dev = device(id, &[lease.keycode]);
    dev.rows[0].core = lease.installed.clone();
    dev.rows[0].xkb = vec![lease.keysym];
    dev.definitions[0] = Definition::for_symbol(lease.keysym as _);
    dev.definitions[0].repeat = Some(false);
    dev
}
fn lease() -> Lease {
    Lease {
        keycode: 248,
        keysym: 0x01004e2d,
        original: vec![0],
        installed: vec![0x01004e2d],
    }
}

#[test]
fn divergent_slave_binding_is_skipped_even_when_master_candidate_is_empty() {
    let master = device(3, &[248, 249]);
    let mut slave = device(5, &[248, 249]);
    slave.rows[0].core = vec![0x61];
    slave.rows[0].xkb = vec![0x61];
    slave.definitions[0].symbols = vec![0x61];
    let result = plan_affected_lease(0x01004e2d, &[master, slave], &[]).unwrap();
    assert!(
        matches!(result, LeaseChoice::Install(Lease { keycode: 249, .. })),
        "must not overwrite attached keyboard's occupied keycode 248"
    );
}

#[test]
fn candidate_excludes_slave_modifier_action_and_pressed_keys() {
    for kind in 0..3 {
        let master = device(3, &[248]);
        let mut slave = device(5, &[248]);
        match kind {
            0 => slave.rows[0].is_modifier = true,
            1 => slave.rows[0].has_actions = true,
            _ => slave.rows[0].is_pressed = true,
        }
        assert!(plan_affected_lease(0x01004e2d, &[master, slave], &[]).is_err());
    }
}

#[test]
fn divergent_empty_key_metadata_is_not_a_compatible_restore_candidate() {
    let master = device(3, &[248]);
    let mut slave = device(5, &[248]);
    slave.definitions[0].types[0] = 1;
    assert!(plan_affected_lease(0x01004e2d, &[master, slave], &[]).is_err());
}

#[test]
fn every_affected_device_requires_a_saved_definition_and_repeat_observation() {
    let master = device(3, &[248]);
    let mut unknown = device(5, &[248]);
    unknown.definitions.clear();
    assert!(plan_affected_lease(0x01004e2d, &[master, unknown], &[]).is_err());
    let mut unknown_repeat = device(5, &[248]);
    unknown_repeat.definitions[0].repeat = None;
    assert!(plan_affected_lease(0x01004e2d, &[device(3, &[248]), unknown_repeat], &[]).is_err());
}

#[test]
fn compatible_master_and_slave_candidate_remains_supported() {
    assert!(matches!(
        plan_affected_lease(0x01004e2d, &[device(3, &[248]), device(5, &[248])], &[]),
        Ok(LeaseChoice::Install(Lease { keycode: 248, .. }))
    ));
}

#[test]
fn slave_ownership_change_prevents_restoration_and_reuse() {
    let owned = lease();
    let master = leased(3, &owned);
    let mut slave = leased(5, &owned);
    slave.rows[0].core = vec![0x42];
    slave.rows[0].xkb = vec![0x42];
    let devices = [master, slave];
    assert!(affected_restoration_allowed(&owned, &[3, 5], &devices, false).is_err());
    assert!(plan_affected_lease(owned.keysym, &devices, &[owned]).is_err());
}

#[test]
fn topology_change_cannot_be_reported_as_complete_restoration() {
    let owned = lease();
    assert!(affected_restoration_allowed(&owned, &[3, 5], &[leased(3, &owned)], false).is_err());
    assert!(affected_restoration_allowed(
        &owned,
        &[3, 5],
        &[leased(3, &owned), leased(5, &owned), leased(7, &owned)],
        false
    )
    .is_err());
}

#[test]
fn pending_release_blocks_restoration_on_all_devices() {
    let owned = lease();
    assert_eq!(
        affected_restoration_allowed(
            &owned,
            &[3, 5],
            &[leased(3, &owned), leased(5, &owned)],
            true
        ),
        Err(PolicyError::KeyStillDown)
    );
}

#[test]
fn vacant_multigroup_key_is_skipped_to_avoid_global_group_control_mutation() {
    let mut master = device(3, &[248, 249]);
    let mut slave = device(5, &[248, 249]);
    master.definitions[0].group_info = 2;
    slave.definitions[0].group_info = 2;
    assert!(matches!(
        plan_affected_lease(0x01004e2d, &[master, slave], &[]),
        Ok(LeaseChoice::Install(Lease { keycode: 249, .. }))
    ));
}
