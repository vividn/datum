//! Combining two data objects key by key — the original datum's
//! `combineData` (vividn/datum c8599d84, `src/documentControl/combineData.ts`).
//!
//! A strategy is one row of a table: what happens to a key only in A, a key
//! only in B, and a key they disagree on (one they hold the same is kept).
//! An object in both is combined again, row and all. `update --strategy` calls it with
//! A the stored data and B the payload; resolving a conflict calls it with A
//! and B the two versions, so the neutral constants carry no old/new.
//!
//! Seven of datum's thirteen rows are kept; intersection, removeConflicting,
//! xor, prepend, mergeSort and appendSort are left out (vividn, da-zd0vx).
//!
//! Two departures from datum, both where its `===` compared by reference:
//! values that are EQUAL count as the same, so two equal arrays are `same`
//! rather than a conflict (datum's `append` doubled them), and a merge's
//! dedup compares values, nested ones too.
//! Its `rekey` is a function, not a row; daydumb's lives in `update`.

use serde_json::{Map, Value};

use crate::error::{ParseError, Result};

/// What a key both sides hold, and disagree on, becomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conflict {
    A,
    B,
    /// One array of both, A's first, duplicates removed.
    Merge,
    /// One array of both, A's first, duplicates kept.
    Append,
}

/// One row of datum's strategy table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Combine {
    pub just_a: bool,
    pub just_b: bool,
    pub conflict: Conflict,
}

const fn row(just_a: bool, just_b: bool, conflict: Conflict) -> Combine {
    Combine {
        just_a,
        just_b,
        conflict,
    }
}

impl Combine {
    pub const USE_A: Combine = row(true, false, Conflict::A);
    pub const USE_B: Combine = row(false, true, Conflict::B);
    pub const PREFER_A: Combine = row(true, true, Conflict::A);
    pub const PREFER_B: Combine = row(true, true, Conflict::B);
    pub const MERGE: Combine = row(true, true, Conflict::Merge);
    pub const APPEND: Combine = row(true, true, Conflict::Append);

    /// The row datum named `name` (any case), old being A and new B.
    pub fn named(name: &str) -> Option<Combine> {
        STRATEGIES
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|&(_, c)| c)
    }
}

/// datum's `updateStrategies` that daydumb keeps, in its order, `rekey` aside.
pub const STRATEGIES: &[(&str, Combine)] = &[
    ("useOld", Combine::USE_A),
    ("useNew", Combine::USE_B),
    ("preferOld", Combine::PREFER_A),
    ("update", Combine::PREFER_B),
    ("preferNew", Combine::PREFER_B),
    ("merge", Combine::MERGE),
    ("append", Combine::APPEND),
];

/// Combine `a` and `b` by `how`. Errs only when a merging conflict holds a
/// value that cannot go in an array (an object, or a bool), as datum's
/// `MergeError` did.
pub fn combine(
    a: &Map<String, Value>,
    b: &Map<String, Value>,
    how: Combine,
) -> Result<Map<String, Value>> {
    let mut out = Map::new();
    for (k, av) in a {
        let Some(bv) = b.get(k) else {
            if how.just_a {
                out.insert(k.clone(), av.clone());
            }
            continue;
        };
        let v = match (av, bv) {
            (Value::Object(ao), Value::Object(bo)) => Value::Object(combine(ao, bo, how)?),
            _ if same(av, bv) => av.clone(),
            _ => match how.conflict {
                Conflict::A => av.clone(),
                Conflict::B => bv.clone(),
                Conflict::Merge => merge_values(av, bv, true)?,
                Conflict::Append => merge_values(av, bv, false)?,
            },
        };
        out.insert(k.clone(), v);
    }
    if how.just_b {
        for (k, bv) in b.iter().filter(|(k, _)| !a.contains_key(*k)) {
            out.insert(k.clone(), bv.clone());
        }
    }
    Ok(out)
}

/// datum's `mergeValues`: a string, number, null or array each, as one array.
fn merge_values(a: &Value, b: &Value, unique: bool) -> Result<Value> {
    let items = |v: &Value| match v {
        Value::Array(xs) => Ok(xs.clone()),
        Value::String(_) | Value::Number(_) | Value::Null => Ok(vec![v.clone()]),
        _ => Err(ParseError::InvalidArg(format!(
            "cannot merge {a} with {b}: only strings, numbers, nulls and arrays merge"
        ))),
    };
    let mut out = items(a)?;
    for v in items(b)? {
        if !(unique && out.iter().any(|w| same(w, &v))) {
            out.push(v);
        }
    }
    Ok(Value::Array(out))
}

/// Equal as JSON: numbers by value, so `1` is `1.0`.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(x, y)| same(x, y))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn obj(v: Value) -> Map<String, Value> {
        match v {
            Value::Object(m) => m,
            _ => unreachable!(),
        }
    }

    /// One pair for every strategy: datum's own `aData` / `bData` from
    /// combineData.test.ts, plus its `someDataDuplicated` arrays and a nested
    /// object on both sides.
    fn pair() -> (Map<String, Value>, Map<String, Value>) {
        (
            obj(json!({
                "justA": "aaa", "bothSame": "same", "different": "Aa",
                "someDataDuplicated": ["unique1", "duplicate"],
                "nested": {"innerA": 1, "innerSame": 2, "innerDifferent": 3},
            })),
            obj(json!({
                "justB": "bbb", "bothSame": "same", "different": "bB",
                "someDataDuplicated": ["unique2", "duplicate"],
                "nested": {"innerB": 1, "innerSame": 2, "innerDifferent": 30},
            })),
        )
    }

    /// Expected values from running datum's combineData.ts at c8599d84 on
    /// `pair()` (node 24, type stripping; lodash.isplainobject stubbed).
    #[test]
    fn every_strategy_on_one_pair() {
        let (a, b) = pair();
        let cases = [
            (
                "useOld",
                json!({"justA":"aaa","bothSame":"same","different":"Aa","someDataDuplicated":["unique1","duplicate"],"nested":{"innerA":1,"innerSame":2,"innerDifferent":3}}),
            ),
            (
                "useNew",
                json!({"bothSame":"same","different":"bB","someDataDuplicated":["unique2","duplicate"],"nested":{"innerSame":2,"innerDifferent":30,"innerB":1},"justB":"bbb"}),
            ),
            (
                "preferOld",
                json!({"justA":"aaa","bothSame":"same","different":"Aa","someDataDuplicated":["unique1","duplicate"],"nested":{"innerA":1,"innerSame":2,"innerDifferent":3,"innerB":1},"justB":"bbb"}),
            ),
            (
                "update",
                json!({"justA":"aaa","bothSame":"same","different":"bB","someDataDuplicated":["unique2","duplicate"],"nested":{"innerA":1,"innerSame":2,"innerDifferent":30,"innerB":1},"justB":"bbb"}),
            ),
            (
                "preferNew",
                json!({"justA":"aaa","bothSame":"same","different":"bB","someDataDuplicated":["unique2","duplicate"],"nested":{"innerA":1,"innerSame":2,"innerDifferent":30,"innerB":1},"justB":"bbb"}),
            ),
            (
                "merge",
                json!({"justA":"aaa","bothSame":"same","different":["Aa","bB"],"someDataDuplicated":["unique1","duplicate","unique2"],"nested":{"innerA":1,"innerSame":2,"innerDifferent":[3,30],"innerB":1},"justB":"bbb"}),
            ),
            (
                "append",
                json!({"justA":"aaa","bothSame":"same","different":["Aa","bB"],"someDataDuplicated":["unique1","duplicate","unique2","duplicate"],"nested":{"innerA":1,"innerSame":2,"innerDifferent":[3,30],"innerB":1},"justB":"bbb"}),
            ),
        ];
        assert_eq!(cases.len(), STRATEGIES.len());
        for (name, want) in cases {
            let got = combine(&a, &b, Combine::named(name).unwrap()).unwrap();
            assert_eq!(Value::Object(got), want, "{name}");
        }
    }

    /// datum's mergeValues tests, the ones JSON can express.
    #[test]
    fn merge_values_as_datum() {
        let m = |a, b, u| merge_values(&a, &b, u).unwrap();
        assert_eq!(m(json!("string"), json!(3), true), json!(["string", 3]));
        assert_eq!(m(json!(5), json!(null), true), json!([5, null]));
        assert_eq!(
            m(json!(null), json!([3, "string"]), true),
            json!([null, 3, "string"])
        );
        assert_eq!(
            m(json!([1, "u", "d", 2]), json!([2, "a", "d", 5]), true),
            json!([1, "u", "d", 2, "a", 5])
        );
        for (a, b) in [
            (json!({"abc": 123}), json!("string")),
            (json!(3), json!({"abc": 123})),
            (json!([1, 2, 3]), json!({"q": "a"})),
            (json!(true), json!(false)),
        ] {
            assert!(merge_values(&a, &b, true).is_err(), "{a} {b}");
        }
    }

    #[test]
    fn a_key_both_hold_as_objects_recurses_and_an_object_against_a_scalar_conflicts() {
        // datum's "it recursively calls update if the key is an Object in both".
        let a =
            obj(json!({"outer": {"a": "aaa", "s": "s", "d": "fromA"}, "onlyObjForA": {"k": "v"}}));
        let b = obj(
            json!({"outer": {"b": "bbb", "s": "s", "d": "fromB"}, "onlyObjForA": "bHasAString"}),
        );
        assert_eq!(
            Value::Object(combine(&a, &b, Combine::PREFER_B).unwrap()),
            json!({"outer": {"a": "aaa", "b": "bbb", "s": "s", "d": "fromB"}, "onlyObjForA": "bHasAString"})
        );
        assert!(combine(&a, &b, Combine::MERGE).is_err());
    }

    /// Where datum compared references: an equal array on both sides is the
    /// same key, which datum's `append` doubled.
    #[test]
    fn equal_arrays_are_the_same() {
        let a = obj(json!({"tags": [1, 2]}));
        for name in ["merge", "append"] {
            let got = combine(&a, &a, Combine::named(name).unwrap()).unwrap();
            assert_eq!(Value::Object(got), json!({"tags": [1, 2]}), "{name}");
        }
    }

    #[test]
    fn names_match_in_any_case() {
        assert_eq!(Combine::named("PREFEROLD"), Some(Combine::PREFER_A));
        assert_eq!(Combine::named("rekey"), None);
        assert_eq!(Combine::named("xor"), None);
    }
}
