use std::sync::Arc;

use super::*;

#[test]
fn maps_preserve_insertion_order_and_same_value_zero_updates() {
    let program = lower_to_bytecode(&compile("0;").expect("source should compile"))
        .expect("lowering should succeed");
    let mut runtime =
        Runtime::new(Arc::new(program), ExecutionOptions::default()).expect("runtime init");
    let map = runtime.insert_map(Vec::new()).expect("map should allocate");
    let object = runtime
        .insert_object(IndexMap::new(), ObjectKind::Plain)
        .expect("object should allocate");

    runtime
        .map_set(map, Value::String("alpha".to_string()), Value::Number(1.0))
        .expect("alpha insert should succeed");
    runtime
        .map_set(
            map,
            Value::Number(f64::NAN),
            Value::String("nan".to_string()),
        )
        .expect("nan insert should succeed");
    runtime
        .map_set(map, Value::Number(-0.0), Value::String("zero".to_string()))
        .expect("negative zero insert should succeed");
    runtime
        .map_set(map, Value::Object(object), Value::Bool(true))
        .expect("object key insert should succeed");
    runtime
        .map_set(map, Value::String("alpha".to_string()), Value::Number(2.0))
        .expect("alpha update should keep insertion order");
    runtime
        .map_set(
            map,
            Value::Number(0.0),
            Value::String("zero-updated".to_string()),
        )
        .expect("positive zero update should reuse the existing entry");

    let map_ref = runtime.maps.get(map).expect("map should exist");
    let entries: Vec<_> = map_ref.entries.iter().flatten().collect();
    assert_eq!(map_ref.live_len, 4);
    assert_eq!(entries.len(), 4);
    assert!(matches!(entries[0].key, Value::String(ref value) if value == "alpha"));
    assert!(matches!(entries[0].value, Value::Number(value) if value == 2.0));
    assert!(matches!(entries[1].key, Value::Number(value) if value.is_nan()));
    assert!(matches!(entries[1].value, Value::String(ref value) if value == "nan"));
    assert!(matches!(entries[2].key, Value::Number(value) if value == 0.0));
    assert!(matches!(entries[2].value, Value::String(ref value) if value == "zero-updated"));
    assert!(matches!(entries[3].key, Value::Object(key) if key == object));
    assert!(matches!(entries[3].value, Value::Bool(true)));
}

#[test]
fn keyed_collection_tombstones_preserve_lookup_and_live_lengths() {
    let program = lower_to_bytecode(&compile("0;").expect("source should compile"))
        .expect("lowering should succeed");
    let mut runtime =
        Runtime::new(Arc::new(program), ExecutionOptions::default()).expect("runtime init");
    let map = runtime.insert_map(Vec::new()).expect("map should allocate");
    let set = runtime.insert_set(Vec::new()).expect("set should allocate");

    runtime
        .map_set(map, Value::String("alpha".to_string()), Value::Number(1.0))
        .expect("alpha insert should succeed");
    runtime
        .map_set(map, Value::String("beta".to_string()), Value::Number(2.0))
        .expect("beta insert should succeed");
    assert!(runtime
        .map_delete(map, &Value::String("alpha".to_string()))
        .expect("alpha delete should succeed"));
    runtime
        .map_set(map, Value::String("gamma".to_string()), Value::Number(3.0))
        .expect("gamma insert should succeed");

    let map_ref = runtime.maps.get(map).expect("map should exist");
    assert_eq!(map_ref.live_len, 2);
    assert_eq!(map_ref.entries.len(), 3, "deleted slots stay tombstoned");
    assert!(map_ref.entries[0].is_none());
    assert!(
        map_ref.lookup.is_empty(),
        "small maps stay on the linear fast path"
    );

    runtime
        .set_add(set, Value::String("alpha".to_string()))
        .expect("set alpha insert should succeed");
    runtime
        .set_add(set, Value::String("beta".to_string()))
        .expect("set beta insert should succeed");
    assert!(runtime
        .set_delete(set, &Value::String("alpha".to_string()))
        .expect("set alpha delete should succeed"));
    runtime
        .set_add(set, Value::String("gamma".to_string()))
        .expect("set gamma insert should succeed");

    let set_ref = runtime.sets.get(set).expect("set should exist");
    assert_eq!(set_ref.live_len, 2);
    assert_eq!(set_ref.entries.len(), 3, "deleted slots stay tombstoned");
    assert!(set_ref.entries[0].is_none());
    assert!(
        set_ref.lookup.is_empty(),
        "small sets stay on the linear fast path"
    );

    let original_map_epoch = map_ref.clear_epoch;
    let original_set_epoch = set_ref.clear_epoch;
    drop(map_ref);
    drop(set_ref);

    runtime.map_clear(map).expect("map clear should succeed");
    runtime.set_clear(set).expect("set clear should succeed");

    let map_ref = runtime.maps.get(map).expect("map should still exist");
    let set_ref = runtime.sets.get(set).expect("set should still exist");
    assert_eq!(map_ref.live_len, 0);
    assert!(map_ref.entries.is_empty());
    assert!(map_ref.lookup.is_empty());
    assert_eq!(map_ref.clear_epoch, original_map_epoch.wrapping_add(1));
    assert_eq!(set_ref.live_len, 0);
    assert!(set_ref.entries.is_empty());
    assert!(set_ref.lookup.is_empty());
    assert_eq!(set_ref.clear_epoch, original_set_epoch.wrapping_add(1));

    let promoted_map = runtime.insert_map(Vec::new()).expect("map should allocate");
    for index in 0..COLLECTION_LOOKUP_PROMOTION_LEN {
        runtime
            .map_set(
                promoted_map,
                Value::String(format!("key-{index}")),
                Value::Number(index as f64),
            )
            .expect("promoted map insert should succeed");
    }
    let promoted_map_ref = runtime.maps.get(promoted_map).expect("map should exist");
    assert_eq!(promoted_map_ref.live_len, COLLECTION_LOOKUP_PROMOTION_LEN);
    assert_eq!(promoted_map_ref.lookup.len(), COLLECTION_LOOKUP_PROMOTION_LEN);
    assert!(promoted_map_ref
        .lookup
        .contains_key(&CollectionLookupKey::String("key-0")));

    let promoted_set = runtime.insert_set(Vec::new()).expect("set should allocate");
    for index in 0..COLLECTION_LOOKUP_PROMOTION_LEN {
        runtime
            .set_add(promoted_set, Value::String(format!("key-{index}")))
            .expect("promoted set insert should succeed");
    }
    let promoted_set_ref = runtime.sets.get(promoted_set).expect("set should exist");
    assert_eq!(promoted_set_ref.live_len, COLLECTION_LOOKUP_PROMOTION_LEN);
    assert_eq!(promoted_set_ref.lookup.len(), COLLECTION_LOOKUP_PROMOTION_LEN);
    assert!(promoted_set_ref
        .lookup
        .contains_key(&CollectionLookupKey::String("key-0")));
}
