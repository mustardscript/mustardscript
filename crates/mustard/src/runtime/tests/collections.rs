use super::*;

#[test]
fn maps_preserve_insertion_order_and_same_value_zero_updates() {
    let program = lower_to_bytecode(&compile("0;").expect("source should compile"))
        .expect("lowering should succeed");
    let mut runtime = Runtime::new(program, ExecutionOptions::default()).expect("runtime init");
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

    let entries = &runtime.maps.get(map).expect("map should exist").entries;
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
