use std::collections::BTreeMap;
use operit_link::{CoreEventKind, CoreValue, encodeLink};

fn value(items: Vec<CoreValue>) -> CoreValue {
    CoreValue::Map(BTreeMap::from([
        ("unchanged".into(), CoreValue::String("背景内容".repeat(500))),
        ("items".into(), CoreValue::List(items)),
    ]))
}

#[test]
fn link_selects_and_reconstructs_successive_updates_without_business_flags() {
    let mut previous = None;
    let initial = value(vec![CoreValue::Unsigned(1), CoreValue::Unsigned(2)]);
    let (kind, mut received) = CoreValue::incrementalEvent(&mut previous, initial.clone());
    assert_eq!(kind, CoreEventKind::Snapshot);
    assert_eq!(received, initial);
    for current in [
        value(vec![CoreValue::Unsigned(9), CoreValue::Unsigned(2)]),
        value(vec![CoreValue::Unsigned(9)]),
        value(vec![CoreValue::Unsigned(9), CoreValue::Null]),
        value(vec![CoreValue::Unsigned(9), CoreValue::Null]),
    ] {
        let (kind, delta) = CoreValue::incrementalEvent(&mut previous, current.clone());
        assert_eq!(kind, CoreEventKind::Delta);
        assert!(encodeLink(&delta).unwrap().len() < encodeLink(&current).unwrap().len());
        received = received.applyIncrementalDelta(&delta).unwrap();
        assert_eq!(received, current);
        assert_eq!(previous.as_ref(), Some(&current));
    }
    let (kind, small) = CoreValue::incrementalEvent(&mut previous, CoreValue::Bool(true));
    assert_eq!(kind, CoreEventKind::Changed);
    assert_eq!(small, CoreValue::Bool(true));
}

#[test]
fn unchanged_fields_are_absent_from_patch() {
    let mut previous = Some(value(vec![CoreValue::Unsigned(1)]));
    let (_, delta) = CoreValue::incrementalEvent(&mut previous, value(vec![CoreValue::Unsigned(2)]));
    let CoreValue::Map(fields) = delta else { panic!("expected delta") };
    let CoreValue::List(operations) = &fields["$coreDelta"] else { panic!("expected operations") };
    assert_eq!(operations.len(), 1);
    let CoreValue::Map(operation) = &operations[0] else { panic!("expected operation") };
    assert_eq!(operation["path"], CoreValue::List(vec![CoreValue::String("items".into()), CoreValue::Unsigned(0)]));
}
