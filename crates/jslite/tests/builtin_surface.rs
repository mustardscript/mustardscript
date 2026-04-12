use indexmap::IndexMap;

use jslite::{ExecutionOptions, StructuredValue, compile, execute};

fn number(value: f64) -> StructuredValue {
    StructuredValue::from(value)
}

#[test]
fn array_helpers_cover_mutation_search_and_slicing() {
    let program = compile(
        r#"
        const values = [1, 2];
        const nan = 0 / 0;
        const pushed = values.push(3, 4);
        const popped = values.pop();
        [
          pushed,
          popped,
          values.slice(1, 3),
          values.join("-"),
          values.includes(2),
          values.includes(nan),
          [1, nan].includes(nan),
          values.indexOf(2),
          values.indexOf(9),
        ];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            number(4.0),
            number(4.0),
            StructuredValue::Array(vec![number(2.0), number(3.0)]),
            "1-2-3".into(),
            StructuredValue::Bool(true),
            StructuredValue::Bool(false),
            StructuredValue::Bool(true),
            number(1.0),
            number(-1.0),
        ])
    );
}

#[test]
fn array_callback_helpers_cover_transform_search_and_reduction() {
    let program = compile(
        r#"
        const values = [1, 2, 3];
        let seen = 0;
        const mapped = values.map(function (value, index) {
          seen += this.step;
          return value + index + this.offset;
        }, { step: 10, offset: 4 });
        const filtered = values.filter((value) => value % 2 === 1);
        const found = values.find((value) => value > 2);
        const foundIndex = values.findIndex((value) => value > 2);
        const some = values.some((value) => value === 2);
        const every = values.every((value) => value > 0);
        const reduced = values.reduce((acc, value) => acc + value, 5);
        values.forEach((value) => {
          seen += value;
        });
        [mapped, filtered, found, foundIndex, some, every, reduced, seen];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            StructuredValue::Array(vec![number(5.0), number(7.0), number(9.0)]),
            StructuredValue::Array(vec![number(1.0), number(3.0)]),
            number(3.0),
            number(2.0),
            StructuredValue::Bool(true),
            StructuredValue::Bool(true),
            number(11.0),
            number(36.0),
        ])
    );
}

#[test]
fn array_find_helpers_visit_sparse_holes_as_undefined() {
    let program = compile(
        r#"
        const values = [1, , 3];
        const visits = [];
        const found = values.find((value, index) => {
          visits[visits.length] = [index, value, index in values];
          return index === 1;
        });
        const foundIndex = values.findIndex((value, index) => {
          visits[visits.length] = [index + 10, value, index in values];
          return value === undefined;
        });
        [found, foundIndex, visits];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            StructuredValue::Undefined,
            number(1.0),
            StructuredValue::Array(vec![
                StructuredValue::Array(
                    vec![number(0.0), number(1.0), StructuredValue::Bool(true),]
                ),
                StructuredValue::Array(vec![
                    number(1.0),
                    StructuredValue::Undefined,
                    StructuredValue::Bool(false),
                ]),
                StructuredValue::Array(vec![
                    number(10.0),
                    number(1.0),
                    StructuredValue::Bool(true),
                ]),
                StructuredValue::Array(vec![
                    number(11.0),
                    StructuredValue::Undefined,
                    StructuredValue::Bool(false),
                ]),
            ]),
        ])
    );
}

#[test]
fn array_length_updates_change_helper_traversal_and_find_last_visits_holes() {
    let program = compile(
        r#"
        const someValues = [0, 1, 2];
        const someVisits = [];
        someValues.some((value, index, array) => {
          someVisits[someVisits.length] = [index, value, index in array];
          if (index === 0) {
            array.length = 1;
          }
          return false;
        });

        const reduced = [1, 2, 3];
        const reduceVisits = [];
        const reduceResult = reduced.reduce((acc, value, index, array) => {
          reduceVisits[reduceVisits.length] = index;
          if (index === 0) {
            array.length = 1;
          }
          return acc + value;
        }, 0);

        const reducedRight = [1, 2, 3];
        const reduceRightVisits = [];
        const reduceRightResult = reducedRight.reduceRight((acc, value, index, array) => {
          reduceRightVisits[reduceRightVisits.length] = index;
          if (index === 2) {
            array.length = 0;
          }
          return acc + value;
        }, 0);

        const sparse = [0, , 2];
        const findLastVisits = [];
        const findLastIndex = sparse.findLastIndex((value, index, array) => {
          findLastVisits[findLastVisits.length] = [index, value, index in array];
          return index === 1;
        });

        const invalidLength = (() => {
          try {
            const values = [1];
            values.length = 1.5;
            return "unreachable";
          } catch (error) {
            return [error.name, error.message];
          }
        })();

        [
          someVisits,
          Object.keys(someValues),
          someValues.length,
          reduceResult,
          reduceVisits,
          Object.keys(reduced),
          reduced.length,
          reduceRightResult,
          reduceRightVisits,
          Object.keys(reducedRight),
          reducedRight.length,
          findLastIndex,
          findLastVisits,
          invalidLength,
        ];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            StructuredValue::Array(vec![StructuredValue::Array(vec![
                number(0.0),
                number(0.0),
                StructuredValue::Bool(true),
            ])]),
            StructuredValue::Array(vec!["0".into()]),
            number(1.0),
            number(1.0),
            StructuredValue::Array(vec![number(0.0)]),
            StructuredValue::Array(vec!["0".into()]),
            number(1.0),
            number(3.0),
            StructuredValue::Array(vec![number(2.0)]),
            StructuredValue::Array(vec![]),
            number(0.0),
            number(1.0),
            StructuredValue::Array(vec![
                StructuredValue::Array(
                    vec![number(2.0), number(2.0), StructuredValue::Bool(true),]
                ),
                StructuredValue::Array(vec![
                    number(1.0),
                    StructuredValue::Undefined,
                    StructuredValue::Bool(false),
                ]),
            ]),
            StructuredValue::Array(vec!["RangeError".into(), "Invalid array length".into()]),
        ])
    );
}

#[test]
fn array_callback_helpers_fail_closed_for_invalid_callbacks_and_empty_reduce() {
    let map_error = compile("([1]).map(1);").expect("source should compile");
    let error =
        execute(&map_error, ExecutionOptions::default()).expect_err("execution should fail");
    assert!(
        error
            .to_string()
            .contains("Array.prototype.map expects a callable callback")
    );

    let reduce_error =
        compile("([].reduce((acc, value) => acc + value));").expect("source should compile");
    let error =
        execute(&reduce_error, ExecutionOptions::default()).expect_err("execution should fail");
    assert!(
        error
            .to_string()
            .contains("Array.prototype.reduce requires an initial value for empty arrays")
    );
}

#[test]
fn string_helpers_cover_trimming_queries_and_case_changes() {
    let program = compile(
        r#"
        const value = "  MiXeD Example  ";
        const csv = "alpha,beta,gamma";
        [
          value.trim(),
          value.includes("XeD"),
          value.startsWith("Mi", 2),
          value.endsWith("ple  "),
          value.slice(2, -2),
          value.substring(8, 3),
          value.toLowerCase(),
          value.toUpperCase(),
          csv.split(",", 2),
          value.replace("MiXeD", "Mixed"),
          "a-b-a".replaceAll("a", "z"),
          value.search("Example"),
          value.match("Example"),
        ];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            "MiXeD Example".into(),
            StructuredValue::Bool(true),
            StructuredValue::Bool(true),
            StructuredValue::Bool(true),
            "MiXeD Example".into(),
            "iXeD ".into(),
            "  mixed example  ".into(),
            "  MIXED EXAMPLE  ".into(),
            StructuredValue::Array(vec!["alpha".into(), "beta".into()]),
            "  Mixed Example  ".into(),
            "z-b-z".into(),
            number(8.0),
            StructuredValue::Array(vec!["Example".into()]),
        ])
    );
}

#[test]
fn regex_helpers_cover_patterns_callbacks_and_state() {
    let program = compile(
        r#"
        const exec = /(?<letters>[a-z]+)(\d+)/g;
        const first = exec.exec("ab12cd34");
        const firstLast = exec.lastIndex;
        const second = exec.exec("ab12cd34");
        const secondLast = exec.lastIndex;
        const third = exec.exec("ab12cd34");
        const thirdLast = exec.lastIndex;
        const sticky = /a/y;
        sticky.lastIndex = 1;
        const stickyFirst = sticky.exec("ba");
        const stickyFirstLast = sticky.lastIndex;
        const stickySecond = sticky.exec("ba");
        const stickySecondLast = sticky.lastIndex;
        const matched = "abc123".match(/(?<letters>[a-z]+)(\d+)/);
        ({
          split: "a1b2".split(/(\d)/),
          replaceLiteralCallback: "abc".replace("a", (match, offset, input) => `${match}:${offset}:${input}`),
          replaceRegexTemplate: "abc123".replace(/(?<letters>[a-z]+)(\d+)/, "$<letters>-$2"),
          replaceAllRegexCallback: "alpha-1 beta-2".replaceAll(
            /([a-z]+)-(\d)/g,
            (match, word, digit, offset, input) => `${word.toUpperCase()}:${digit}:${offset}:${input.length}`
          ),
          search: "abc123".search(/\d+/),
          matchSingle: [matched[0], matched[1], matched[2], matched.index, matched.input, matched.groups.letters],
          matchGlobal: "ab12cd34".match(/\d+/g),
          firstExec: [first[0], first[1], first[2], first.index, first.input, first.groups.letters, firstLast],
          secondExec: [second[0], second.index, secondLast],
          thirdExec: [third === null, thirdLast],
          testState: (() => {
            const regex = /a/g;
            return [regex.test("ba"), regex.lastIndex, regex.test("ba"), regex.lastIndex];
          })(),
          stickyState: [stickyFirst[0], stickyFirst.index, stickyFirstLast, stickySecond === null, stickySecondLast],
          ctor: [RegExp("a", "gi").flags, new RegExp(/b/g).source, new RegExp(/b/g).flags],
        });
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Object(IndexMap::from([
            (
                "split".to_string(),
                StructuredValue::Array(vec![
                    "a".into(),
                    "1".into(),
                    "b".into(),
                    "2".into(),
                    "".into(),
                ]),
            ),
            ("replaceLiteralCallback".to_string(), "a:0:abcbc".into(),),
            ("replaceRegexTemplate".to_string(), "abc-123".into()),
            (
                "replaceAllRegexCallback".to_string(),
                "ALPHA:1:0:14 BETA:2:8:14".into(),
            ),
            ("search".to_string(), number(3.0)),
            (
                "matchSingle".to_string(),
                StructuredValue::Array(vec![
                    "abc123".into(),
                    "abc".into(),
                    "123".into(),
                    number(0.0),
                    "abc123".into(),
                    "abc".into(),
                ]),
            ),
            (
                "matchGlobal".to_string(),
                StructuredValue::Array(vec!["12".into(), "34".into()]),
            ),
            (
                "firstExec".to_string(),
                StructuredValue::Array(vec![
                    "ab12".into(),
                    "ab".into(),
                    "12".into(),
                    number(0.0),
                    "ab12cd34".into(),
                    "ab".into(),
                    number(4.0),
                ]),
            ),
            (
                "secondExec".to_string(),
                StructuredValue::Array(vec!["cd34".into(), number(4.0), number(8.0)]),
            ),
            (
                "thirdExec".to_string(),
                StructuredValue::Array(vec![StructuredValue::Bool(true), number(0.0)]),
            ),
            (
                "testState".to_string(),
                StructuredValue::Array(vec![
                    StructuredValue::Bool(true),
                    number(2.0),
                    StructuredValue::Bool(false),
                    number(0.0),
                ]),
            ),
            (
                "stickyState".to_string(),
                StructuredValue::Array(vec![
                    "a".into(),
                    number(1.0),
                    number(2.0),
                    StructuredValue::Bool(true),
                    number(0.0),
                ]),
            ),
            (
                "ctor".to_string(),
                StructuredValue::Array(vec!["gi".into(), "b".into(), "g".into()]),
            ),
        ]))
    );
}

#[test]
fn regex_helpers_fail_closed_for_unsupported_flags_and_sync_host_replacements() {
    let invalid_flags = compile(r#"new RegExp("a", "dg");"#).expect("source should compile");
    let error =
        execute(&invalid_flags, ExecutionOptions::default()).expect_err("execution should fail");
    assert!(
        error
            .to_string()
            .contains("unsupported regular expression flag `d`")
    );

    let replace_all = compile(r#""abc".replaceAll(/a/, "z");"#).expect("source should compile");
    let error =
        execute(&replace_all, ExecutionOptions::default()).expect_err("execution should fail");
    assert!(
        error
            .to_string()
            .contains("String.prototype.replaceAll requires a global RegExp")
    );

    let host_callback =
        compile(r#""abc".replace("a", fetch_data);"#).expect("source should compile");
    let error = execute(
        &host_callback,
        ExecutionOptions {
            capabilities: vec!["fetch_data".to_string()],
            ..ExecutionOptions::default()
        },
    )
    .expect_err("execution should fail");
    assert!(error.to_string().contains(
        "String.prototype.replace callback replacements do not support host suspensions"
    ));
}

#[test]
fn object_helpers_enumerate_plain_objects_and_arrays_deterministically() {
    let program = compile(
        r#"
        const object = { zebra: 1, alpha: 2 };
        const array = [4, 5];
        array.extra = 6;
        const assignedObjectTarget = { alpha: 1 };
        const assignedObject = Object.assign(
          assignedObjectTarget,
          { zebra: 2 },
          undefined,
          { beta: 3 },
        );
        const assignedArrayTarget = [4];
        assignedArrayTarget.label = "seed";
        const assignedArray = Object.assign(
          assignedArrayTarget,
          { 1: 5 },
          [6],
          null,
          { extra: 7 },
        );
        ({
          objectKeys: Object.keys(object),
          objectValues: Object.values(object),
          objectEntries: Object.entries(object),
          arrayKeys: Object.keys(array),
          arrayValues: Object.values(array),
          arrayEntries: Object.entries(array),
          hasOwnAlpha: Object.hasOwn(object, "alpha"),
          hasOwnMissing: Object.hasOwn(object, "missing"),
          assignedObjectIdentity: assignedObject === assignedObjectTarget,
          assignedObjectEntries: Object.entries(assignedObject),
          assignedArrayIdentity: assignedArray === assignedArrayTarget,
          assignedArrayEntries: Object.entries(assignedArray),
        });
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Object(IndexMap::from([
            (
                "objectKeys".to_string(),
                StructuredValue::Array(vec!["zebra".into(), "alpha".into()]),
            ),
            (
                "objectValues".to_string(),
                StructuredValue::Array(vec![number(1.0), number(2.0)]),
            ),
            (
                "objectEntries".to_string(),
                StructuredValue::Array(vec![
                    StructuredValue::Array(vec!["zebra".into(), number(1.0)]),
                    StructuredValue::Array(vec!["alpha".into(), number(2.0)]),
                ]),
            ),
            (
                "arrayKeys".to_string(),
                StructuredValue::Array(vec!["0".into(), "1".into(), "extra".into()]),
            ),
            (
                "arrayValues".to_string(),
                StructuredValue::Array(vec![number(4.0), number(5.0), number(6.0)]),
            ),
            (
                "arrayEntries".to_string(),
                StructuredValue::Array(vec![
                    StructuredValue::Array(vec!["0".into(), number(4.0)]),
                    StructuredValue::Array(vec!["1".into(), number(5.0)]),
                    StructuredValue::Array(vec!["extra".into(), number(6.0)]),
                ]),
            ),
            ("hasOwnAlpha".to_string(), StructuredValue::Bool(true)),
            ("hasOwnMissing".to_string(), StructuredValue::Bool(false)),
            (
                "assignedObjectIdentity".to_string(),
                StructuredValue::Bool(true),
            ),
            (
                "assignedObjectEntries".to_string(),
                StructuredValue::Array(vec![
                    StructuredValue::Array(vec!["alpha".into(), number(1.0)]),
                    StructuredValue::Array(vec!["zebra".into(), number(2.0)]),
                    StructuredValue::Array(vec!["beta".into(), number(3.0)]),
                ]),
            ),
            (
                "assignedArrayIdentity".to_string(),
                StructuredValue::Bool(true),
            ),
            (
                "assignedArrayEntries".to_string(),
                StructuredValue::Array(vec![
                    StructuredValue::Array(vec!["0".into(), number(6.0)]),
                    StructuredValue::Array(vec!["1".into(), number(5.0)]),
                    StructuredValue::Array(vec!["label".into(), "seed".into()]),
                    StructuredValue::Array(vec!["extra".into(), number(7.0)]),
                ]),
            ),
        ]))
    );
}

#[test]
fn json_stringify_matches_node_ordering_and_omission_rules() {
    let program = compile(
        r#"
        const object = {};
        object.beta = 2;
        object[10] = 10;
        object.alpha = 1;
        object[2] = 3;
        object["01"] = 4;
        const values = [1, undefined, () => 3, (0 / 0), -0, (1 / 0)];
        ({
          objectKeys: Object.keys(object),
          arrayStringified: JSON.stringify(values),
          objectStringified: JSON.stringify(object),
          wrapperStringified: JSON.stringify({
            keep: 1,
            skipUndefined: undefined,
            skipFunction: () => 1,
            nested: object,
          }),
          topLevelUndefined: JSON.stringify(undefined),
        });
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Object(IndexMap::from([
            (
                "objectKeys".to_string(),
                StructuredValue::Array(vec![
                    "2".into(),
                    "10".into(),
                    "beta".into(),
                    "alpha".into(),
                    "01".into(),
                ]),
            ),
            (
                "arrayStringified".to_string(),
                StructuredValue::String("[1,null,null,null,0,null]".to_string()),
            ),
            (
                "objectStringified".to_string(),
                StructuredValue::String(r#"{"2":3,"10":10,"beta":2,"alpha":1,"01":4}"#.to_string()),
            ),
            (
                "wrapperStringified".to_string(),
                StructuredValue::String(
                    r#"{"keep":1,"nested":{"2":3,"10":10,"beta":2,"alpha":1,"01":4}}"#.to_string(),
                ),
            ),
            ("topLevelUndefined".to_string(), StructuredValue::Undefined),
        ]))
    );
}

#[test]
fn math_helpers_cover_numeric_transforms() {
    let program = compile(
        r#"
        [
          Math.pow(2, 5),
          Math.sqrt(81),
          Math.trunc(-3.9),
          Math.sign(-12),
          Math.sign(-0),
        ];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    match result {
        StructuredValue::Array(values) => {
            assert_eq!(values[0], number(32.0));
            assert_eq!(values[1], number(9.0));
            assert_eq!(values[2], number(-3.0));
            assert_eq!(values[3], number(-1.0));
            assert_eq!(values[4], StructuredValue::from(-0.0));
        }
        other => panic!("expected array result, got {other:?}"),
    }
}

#[test]
fn array_of_concat_at_log_and_random_cover_supported_surface() {
    let program = compile(
        r#"
        const single = Array.of(7);
        const merged = Array.of(1, 2, 3).concat([4, 5], 6);
        const random = Math.random();
        [
          single.length,
          single[0],
          merged,
          merged.at(0),
          merged.at(-2),
          merged.at(99),
          Math.log(1),
          Math.round(Math.log(8) / Math.log(2)),
          typeof random === "number",
          random >= 0 && random < 1,
          random === random,
        ];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            number(1.0),
            number(7.0),
            StructuredValue::Array(vec![
                number(1.0),
                number(2.0),
                number(3.0),
                number(4.0),
                number(5.0),
                number(6.0),
            ]),
            number(1.0),
            number(5.0),
            StructuredValue::Undefined,
            number(0.0),
            number(3.0),
            StructuredValue::Bool(true),
            StructuredValue::Bool(true),
            StructuredValue::Bool(true),
        ])
    );
}

#[test]
fn array_splice_flat_and_flat_map_cover_supported_surface() {
    let program = compile(
        r#"
        const values = [1, 2, 3, 4];
        values.label = "seed";
        const removed = values.splice(-3, 2, 9, 10, 11);
        const untouched = [7, 8];
        const untouchedRemoved = untouched.splice();
        [
          Object.entries(values),
          removed,
          untouched,
          untouchedRemoved,
          [1, [2, [3]], 4].flat(undefined),
          [1, [2, [3, [4]]], 5].flat(2),
          [1, 2, 3].flatMap(function (value, index) {
            return [value + this.offset, [index]];
          }, { offset: 4 }),
        ];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            StructuredValue::Array(vec![
                StructuredValue::Array(vec!["0".into(), number(1.0)]),
                StructuredValue::Array(vec!["1".into(), number(9.0)]),
                StructuredValue::Array(vec!["2".into(), number(10.0)]),
                StructuredValue::Array(vec!["3".into(), number(11.0)]),
                StructuredValue::Array(vec!["4".into(), number(4.0)]),
                StructuredValue::Array(vec!["label".into(), "seed".into()]),
            ]),
            StructuredValue::Array(vec![number(2.0), number(3.0)]),
            StructuredValue::Array(vec![number(7.0), number(8.0)]),
            StructuredValue::Array(Vec::new()),
            StructuredValue::Array(vec![
                number(1.0),
                number(2.0),
                StructuredValue::Array(vec![number(3.0)]),
                number(4.0),
            ]),
            StructuredValue::Array(vec![
                number(1.0),
                number(2.0),
                number(3.0),
                StructuredValue::Array(vec![number(4.0)]),
                number(5.0),
            ]),
            StructuredValue::Array(vec![
                number(5.0),
                StructuredValue::Array(vec![number(0.0)]),
                number(6.0),
                StructuredValue::Array(vec![number(1.0)]),
                number(7.0),
                StructuredValue::Array(vec![number(2.0)]),
            ]),
        ])
    );
}

#[test]
fn date_number_string_and_reverse_array_helpers_cover_supported_surface() {
    let program = compile(
        r#"
        const date = new Date("2026-04-10T14:05:06.789Z");
        ({
          iso: date.toISOString(),
          json: JSON.stringify({ date }),
          utc: [
            date.getUTCFullYear(),
            date.getUTCMonth(),
            date.getUTCDate(),
            date.getUTCHours(),
            date.getUTCMinutes(),
            date.getUTCSeconds(),
          ],
          parsedInt: Number.parseInt("  -0x10"),
          globalParsedInt: parseInt("08"),
          parsedFloat: Number.parseFloat("  -10.25ms"),
          isNaN: Number.isNaN(0 / 0),
          isNaNString: Number.isNaN("NaN"),
          globalIsNaN: isNaN(NaN),
          isFinite: Number.isFinite(12.5),
          isFiniteInfinite: Number.isFinite(1 / 0),
          globalIsFinite: isFinite(12.5),
          isInteger: Number.isInteger(12),
          isSafeInteger: Number.isSafeInteger(Number.MAX_SAFE_INTEGER),
          maxSafeInteger: Number.MAX_SAFE_INTEGER,
          minSafeInteger: Number.MIN_SAFE_INTEGER,
          epsilon: Number.EPSILON > 0 && Number.EPSILON < 1,
          numberNaN: Number.isNaN(Number.NaN),
          positiveInfinity: Number.POSITIVE_INFINITY,
          negativeInfinity: Number.NEGATIVE_INFINITY,
          globalInfinity: Infinity,
          trimStart: "  padded  ".trimStart(),
          trimEnd: "  padded  ".trimEnd(),
          padStart: "7".padStart(3, "0"),
          padEnd: "7".padEnd(3, "0"),
          reduceRight: [1, 2, 3].reduceRight((acc, value) => acc + ":" + value, "tail"),
          findLast: [1, 2, 3, 4].findLast((value) => value % 2 === 0),
          findLastIndex: [1, 2, 3, 4].findLastIndex((value) => value % 2 === 0),
          mathPiRounded: Math.round(Math.PI * 1000) / 1000,
          mathExpRounded: Math.round(Math.exp(1) * 1000) / 1000,
          mathLog2: Math.log2(8),
          mathLog10: Math.log10(1000),
          mathSinRounded: Math.round(Math.sin(Math.PI / 2) * 1000) / 1000,
          mathCosRounded: Math.round(Math.cos(Math.PI) * 1000) / 1000,
          mathAtan2Rounded: Math.round(Math.atan2(0, -1) * 1000) / 1000,
          mathHypot: Math.hypot(3, 4),
          mathCbrt: Math.cbrt(27),
          syntaxError: [
            new SyntaxError("bad").name,
            new SyntaxError("bad") instanceof SyntaxError,
            new SyntaxError("bad") instanceof Error,
          ],
        });
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Object(IndexMap::from([
            ("iso".to_string(), "2026-04-10T14:05:06.789Z".into()),
            (
                "json".to_string(),
                r#"{"date":"2026-04-10T14:05:06.789Z"}"#.into(),
            ),
            (
                "utc".to_string(),
                StructuredValue::Array(vec![
                    number(2026.0),
                    number(3.0),
                    number(10.0),
                    number(14.0),
                    number(5.0),
                    number(6.0),
                ]),
            ),
            ("parsedInt".to_string(), number(-16.0)),
            ("globalParsedInt".to_string(), number(8.0)),
            ("parsedFloat".to_string(), number(-10.25)),
            ("isNaN".to_string(), StructuredValue::Bool(true)),
            ("isNaNString".to_string(), StructuredValue::Bool(false)),
            ("globalIsNaN".to_string(), StructuredValue::Bool(true)),
            ("isFinite".to_string(), StructuredValue::Bool(true)),
            ("isFiniteInfinite".to_string(), StructuredValue::Bool(false)),
            ("globalIsFinite".to_string(), StructuredValue::Bool(true)),
            ("isInteger".to_string(), StructuredValue::Bool(true)),
            ("isSafeInteger".to_string(), StructuredValue::Bool(true)),
            (
                "maxSafeInteger".to_string(),
                number(9_007_199_254_740_991.0)
            ),
            (
                "minSafeInteger".to_string(),
                number(-9_007_199_254_740_991.0)
            ),
            ("epsilon".to_string(), StructuredValue::Bool(true)),
            ("numberNaN".to_string(), StructuredValue::Bool(true)),
            ("positiveInfinity".to_string(), number(f64::INFINITY)),
            ("negativeInfinity".to_string(), number(f64::NEG_INFINITY)),
            ("globalInfinity".to_string(), number(f64::INFINITY)),
            ("trimStart".to_string(), "padded  ".into()),
            ("trimEnd".to_string(), "  padded".into()),
            ("padStart".to_string(), "007".into()),
            ("padEnd".to_string(), "700".into()),
            ("reduceRight".to_string(), "tail:3:2:1".into()),
            ("findLast".to_string(), number(4.0)),
            ("findLastIndex".to_string(), number(3.0)),
            (
                "mathPiRounded".to_string(),
                number((std::f64::consts::PI * 1000.0).round() / 1000.0),
            ),
            (
                "mathExpRounded".to_string(),
                number((std::f64::consts::E * 1000.0).round() / 1000.0),
            ),
            ("mathLog2".to_string(), number(3.0)),
            ("mathLog10".to_string(), number(3.0)),
            ("mathSinRounded".to_string(), number(1.0)),
            ("mathCosRounded".to_string(), number(-1.0)),
            (
                "mathAtan2Rounded".to_string(),
                number((std::f64::consts::PI * 1000.0).round() / 1000.0),
            ),
            ("mathHypot".to_string(), number(5.0)),
            ("mathCbrt".to_string(), number(3.0)),
            (
                "syntaxError".to_string(),
                StructuredValue::Array(vec![
                    "SyntaxError".into(),
                    StructuredValue::Bool(true),
                    StructuredValue::Bool(true),
                ]),
            ),
        ]))
    );
}

#[test]
fn additional_string_and_array_helpers_cover_supported_surface() {
    let program = compile(
        r#"
        const values = [1, , 3, 1];
        values.label = "seed";
        const reversed = values.reverse();
        const filled = Array(4);
        filled.fill("x", 1, 3);
        ({
          stringIndexOf: "banana".indexOf("na", 1),
          stringLastIndexOf: "banana".lastIndexOf("na"),
          charAt: "hello".charAt(1),
          at: "hello".at(-2),
          missingAt: "hello".at(9),
          repeat: "ha".repeat(3),
          concat: "alpha".concat("-", 2, true),
          reversedIdentity: reversed === values,
          reversedKeys: Object.keys(values),
          reversedValues: Array.from(values.values()),
          reversedLastIndexOf: values.lastIndexOf(1),
          filledKeys: Object.keys(filled),
          filledValues: Array.from(filled.values()),
        });
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Object(IndexMap::from([
            ("stringIndexOf".to_string(), number(2.0)),
            ("stringLastIndexOf".to_string(), number(4.0)),
            ("charAt".to_string(), "e".into()),
            ("at".to_string(), "l".into()),
            ("missingAt".to_string(), StructuredValue::Undefined),
            ("repeat".to_string(), "hahaha".into()),
            ("concat".to_string(), "alpha-2true".into()),
            ("reversedIdentity".to_string(), StructuredValue::Bool(true)),
            (
                "reversedKeys".to_string(),
                StructuredValue::Array(vec!["0".into(), "1".into(), "3".into(), "label".into(),]),
            ),
            (
                "reversedValues".to_string(),
                StructuredValue::Array(vec![
                    number(1.0),
                    number(3.0),
                    StructuredValue::Undefined,
                    number(1.0),
                ]),
            ),
            ("reversedLastIndexOf".to_string(), number(3.0)),
            (
                "filledKeys".to_string(),
                StructuredValue::Array(vec!["1".into(), "2".into(),]),
            ),
            (
                "filledValues".to_string(),
                StructuredValue::Array(vec![
                    StructuredValue::Undefined,
                    "x".into(),
                    "x".into(),
                    StructuredValue::Undefined,
                ]),
            ),
        ]))
    );
}

#[test]
fn intl_subset_covers_supported_surface() {
    let program = compile(
        r#"
        const date = new Date("2026-04-10T14:05:06.789Z");
        const dateFormatter = new Intl.DateTimeFormat("en-US", {
          timeZone: "UTC",
          year: "numeric",
          month: "2-digit",
          day: "2-digit",
        });
        const numberFormatter = Intl.NumberFormat("en-US", {
          style: "currency",
          currency: "USD",
          minimumFractionDigits: 2,
          maximumFractionDigits: 2,
        });
        ({
          date: dateFormatter.format(date),
          dateOptions: dateFormatter.resolvedOptions(),
          currency: numberFormatter.format(1234.5),
          negativeCurrency: numberFormatter.format(-1.23),
          currencyOptions: numberFormatter.resolvedOptions(),
          hourMinute: Intl.DateTimeFormat("en-US", {
            timeZone: "UTC",
            hour: "numeric",
            minute: "2-digit",
          }).format(date),
          decimal: Intl.NumberFormat("en-US", {
            useGrouping: false,
            minimumFractionDigits: 1,
            maximumFractionDigits: 1,
          }).format(12),
        });
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Object(IndexMap::from([
            ("date".to_string(), "04/10/2026".into()),
            (
                "dateOptions".to_string(),
                StructuredValue::Object(IndexMap::from([
                    ("locale".to_string(), "en-US".into()),
                    ("timeZone".to_string(), "UTC".into()),
                    ("year".to_string(), "numeric".into()),
                    ("month".to_string(), "2-digit".into()),
                    ("day".to_string(), "2-digit".into()),
                ])),
            ),
            ("currency".to_string(), "$1,234.50".into()),
            ("negativeCurrency".to_string(), "-$1.23".into()),
            (
                "currencyOptions".to_string(),
                StructuredValue::Object(IndexMap::from([
                    ("locale".to_string(), "en-US".into()),
                    ("style".to_string(), "currency".into()),
                    ("currency".to_string(), "USD".into()),
                    ("minimumFractionDigits".to_string(), number(2.0)),
                    ("maximumFractionDigits".to_string(), number(2.0)),
                    ("useGrouping".to_string(), StructuredValue::Bool(true)),
                ])),
            ),
            ("hourMinute".to_string(), "2:05 PM".into()),
            ("decimal".to_string(), "12.0".into()),
        ]))
    );
}

#[test]
fn intl_rejects_unsupported_options_and_invalid_dates() {
    let weekday_error = compile(r#"Intl.DateTimeFormat("en-US", { weekday: "long" });"#)
        .expect("source should compile");
    let error =
        execute(&weekday_error, ExecutionOptions::default()).expect_err("execution should fail");
    assert!(
        error
            .to_string()
            .contains("Intl.DateTimeFormat does not support the `weekday` option")
    );

    let notation_error = compile(r#"Intl.NumberFormat("en-US", { notation: "scientific" });"#)
        .expect("source should compile");
    let error =
        execute(&notation_error, ExecutionOptions::default()).expect_err("execution should fail");
    assert!(
        error
            .to_string()
            .contains("Intl.NumberFormat does not support the `notation` option")
    );

    let invalid_date = compile(
        r#"Intl.DateTimeFormat("en-US", { timeZone: "UTC", year: "numeric" }).format(new Date(0 / 0));"#,
    )
    .expect("source should compile");
    let error =
        execute(&invalid_date, ExecutionOptions::default()).expect_err("execution should fail");
    assert!(error.to_string().contains("RangeError: Invalid time value"));
}

#[test]
fn iterable_conversion_helpers_cover_supported_iterables() {
    let program = compile(
        r#"
        const set = new Set(["b", "a", "b"]);
        const mapped = Array.from(set, (value, index) => value + index);
        const fromEntries = Object.fromEntries(new Map([["alpha", 1], ["beta", 2]]));
        const rows = [
          { name: "low", score: 1 },
          { name: "high", score: 3 },
          { name: "mid", score: 2 },
        ];
        const sameRef = rows.sort((left, right) => right.score - left.score);
        [mapped, fromEntries.alpha, fromEntries.beta, sameRef === rows, rows.map((row) => row.name)];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            StructuredValue::Array(vec!["b0".into(), "a1".into()]),
            number(1.0),
            number(2.0),
            StructuredValue::Bool(true),
            StructuredValue::Array(vec!["high".into(), "mid".into(), "low".into()]),
        ])
    );
}

#[test]
fn match_all_and_date_helpers_cover_supported_surface() {
    let program = compile(
        r#"
        const matches = [];
        for (const match of "alpha-1 beta-2".matchAll(/([a-z]+)-(\d)/g)) {
          matches.push([match[0], match[1], match[2], match.index, match.input]);
        }
        const before = Date.now();
        function declared() {
          return "ok";
        }
        function sum(left, right) {
          return this.base + left + right;
        }
        const bound = sum.bind({ base: 10 }, 1);
        const parsed = new Date("2026-04-10T14:00:00Z").getTime();
        const dateOnly = new Date("1970-01-01").getTime();
        const extended = new Date("+010000-01-01T00:00:00.000Z").getTime();
        const cloned = new Date(new Date(5)).getTime();
        const invalid = new Date("not-a-date").getTime();
        const clipped = new Date(8640000000000001).getTime();
        const invalidFullYear = new Date("not-a-date").getUTCFullYear();
        const invalidMonth = new Date("not-a-date").getUTCMonth();
        const negativeZeroIsClipped = new Date(-0.1).getTime() === 0;
        const negativeZeroReciprocalPositive = (1 / new Date(-0.1).getTime()) > 0;
        const maxIso = new Date(8640000000000000).toISOString();
        const maxJson = new Date(8640000000000000).toJSON();
        const negativeYearJson = new Date(-62198755200000).toJSON();
        const after = Date.now();
        [
          matches,
          before <= after,
          globalThis.declared === declared,
          [typeof sum.call, typeof sum.apply, typeof sum.bind],
          sum.call({ base: 4 }, 5, 6),
          sum.apply({ base: 7 }, [8, 9]),
          [typeof bound, bound(2)],
          [typeof Date.prototype.getTime, typeof Date.prototype.valueOf],
          parsed,
          dateOnly,
          extended,
          cloned,
          clipped !== clipped,
          new Date(5).valueOf(),
          Object("  hi  ").trim(),
          invalid !== invalid,
          invalidFullYear !== invalidFullYear,
          invalidMonth !== invalidMonth,
          negativeZeroIsClipped,
          negativeZeroReciprocalPositive,
          maxIso,
          maxJson,
          negativeYearJson
        ];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            StructuredValue::Array(vec![
                StructuredValue::Array(vec![
                    "alpha-1".into(),
                    "alpha".into(),
                    "1".into(),
                    number(0.0),
                    "alpha-1 beta-2".into(),
                ]),
                StructuredValue::Array(vec![
                    "beta-2".into(),
                    "beta".into(),
                    "2".into(),
                    number(8.0),
                    "alpha-1 beta-2".into(),
                ]),
            ]),
            StructuredValue::Bool(true),
            StructuredValue::Bool(true),
            StructuredValue::Array(vec![
                "function".into(),
                "function".into(),
                "function".into(),
            ]),
            number(15.0),
            number(24.0),
            StructuredValue::Array(vec!["function".into(), number(13.0)]),
            StructuredValue::Array(vec!["function".into(), "function".into()]),
            number(1_775_829_600_000.0),
            number(0.0),
            number(253_402_300_800_000.0),
            number(5.0),
            StructuredValue::Bool(true),
            number(5.0),
            "hi".into(),
            StructuredValue::Bool(true),
            StructuredValue::Bool(true),
            StructuredValue::Bool(true),
            StructuredValue::Bool(true),
            StructuredValue::Bool(true),
            "+275760-09-13T00:00:00.000Z".into(),
            "+275760-09-13T00:00:00.000Z".into(),
            "-000001-01-01T00:00:00.000Z".into(),
        ])
    );
}

#[test]
fn error_constructor_options_match_supported_surface() {
    let program = compile(
        r#"
        const empty = new Error(undefined);
        const caused = new Error("boom", { cause: 1 });
        let unsupported;
        try {
          new Error("boom", 1);
        } catch (error) {
          unsupported = [error.name, error.message];
        }
        [
          empty.message,
          caused.message,
          caused.cause,
          caused.constructor === Error,
          unsupported
        ];
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            "".into(),
            "boom".into(),
            number(1.0),
            StructuredValue::Bool(true),
            StructuredValue::Array(vec![
                "TypeError".into(),
                "Error options must be an object in the supported surface".into(),
            ]),
        ])
    );
}

#[test]
fn new_builtins_fail_closed_for_unsupported_inputs() {
    let object_assign = compile("Object.assign(1, { alpha: 1 });").expect("source should compile");
    let error =
        execute(&object_assign, ExecutionOptions::default()).expect_err("execution should fail");
    assert!(
        error
            .to_string()
            .contains("Object helpers currently only support plain objects and arrays")
    );

    let array_from = compile("Array.from({ length: 1, 0: 'a' });").expect("source should compile");
    let error =
        execute(&array_from, ExecutionOptions::default()).expect_err("execution should fail");
    assert!(
        error
            .to_string()
            .contains("value is not iterable in the supported surface")
    );

    let from_entries = compile("Object.fromEntries([1]);").expect("source should compile");
    let error =
        execute(&from_entries, ExecutionOptions::default()).expect_err("execution should fail");
    assert!(
        error
            .to_string()
            .contains("Object.fromEntries expects an iterable of [key, value] pairs")
    );

    let object_create = compile("Object.create(null);").expect("source should compile");
    let error =
        execute(&object_create, ExecutionOptions::default()).expect_err("execution should fail");
    assert!(
        error
            .to_string()
            .contains("Object.create is unsupported because prototype semantics are deferred")
    );

    let object_freeze = compile("Object.freeze({ alpha: 1 });").expect("source should compile");
    let error =
        execute(&object_freeze, ExecutionOptions::default()).expect_err("execution should fail");
    assert!(error.to_string().contains(
        "Object.freeze is unsupported because property descriptor semantics are deferred"
    ));

    let object_seal = compile("Object.seal({ alpha: 1 });").expect("source should compile");
    let error =
        execute(&object_seal, ExecutionOptions::default()).expect_err("execution should fail");
    assert!(
        error.to_string().contains(
            "Object.seal is unsupported because property descriptor semantics are deferred"
        )
    );

    let concat_receiver =
        compile("const concat = [1].concat; concat([2]);").expect("source should compile");
    let error =
        execute(&concat_receiver, ExecutionOptions::default()).expect_err("execution should fail");
    assert!(
        error
            .to_string()
            .contains("Array.prototype.concat called on incompatible receiver")
    );

    let at_receiver = compile("const at = [1].at; at(0);").expect("source should compile");
    let error =
        execute(&at_receiver, ExecutionOptions::default()).expect_err("execution should fail");
    assert!(
        error
            .to_string()
            .contains("Array.prototype.at called on incompatible receiver")
    );

    let splice_receiver =
        compile("const splice = [1].splice; splice(0, 1);").expect("source should compile");
    let error =
        execute(&splice_receiver, ExecutionOptions::default()).expect_err("execution should fail");
    assert!(
        error
            .to_string()
            .contains("Array.prototype.splice called on incompatible receiver")
    );

    let flat_receiver = compile("const flat = [1].flat; flat();").expect("source should compile");
    let error =
        execute(&flat_receiver, ExecutionOptions::default()).expect_err("execution should fail");
    assert!(
        error
            .to_string()
            .contains("Array.prototype.flat called on incompatible receiver")
    );

    let flat_map_receiver = compile("const flatMap = [1].flatMap; flatMap((value) => [value]);")
        .expect("source should compile");
    let error = execute(&flat_map_receiver, ExecutionOptions::default())
        .expect_err("execution should fail");
    assert!(
        error
            .to_string()
            .contains("Array.prototype.flatMap called on incompatible receiver")
    );

    let flat_map_callback = compile("([1]).flatMap(1);").expect("source should compile");
    let error = execute(&flat_map_callback, ExecutionOptions::default())
        .expect_err("execution should fail");
    assert!(
        error
            .to_string()
            .contains("Array.prototype.flatMap expects a callable callback")
    );

    let flat_map_host = compile("[1].flatMap(fetch_data);").expect("source should compile");
    let error = execute(
        &flat_map_host,
        ExecutionOptions {
            capabilities: vec!["fetch_data".to_string()],
            ..ExecutionOptions::default()
        },
    )
    .expect_err("execution should fail");
    assert!(
        error
            .to_string()
            .contains("array callback helpers do not support synchronous host suspensions")
    );

    let match_all = compile(r#""abc".matchAll(/a/);"#).expect("source should compile");
    let error =
        execute(&match_all, ExecutionOptions::default()).expect_err("execution should fail");
    assert!(
        error
            .to_string()
            .contains("String.prototype.matchAll requires a global RegExp")
    );

    let date_call = compile("Date();").expect("source should compile");
    let error =
        execute(&date_call, ExecutionOptions::default()).expect_err("execution should fail");
    assert!(
        error
            .to_string()
            .contains("Date constructor must be called with new")
    );

    let date_result = compile("new Date(0);").expect("source should compile");
    let error =
        execute(&date_result, ExecutionOptions::default()).expect_err("execution should fail");
    assert!(
        error
            .to_string()
            .contains("Date values cannot cross the structured host boundary")
    );
}
