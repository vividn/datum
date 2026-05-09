//! Inject a named argument (like `field` or `state`) into the positional
//! `data` stream as if the user had supplied it with `key=value` syntax.
//!
//! Port of datum's `src/input/flexiblePositional.ts`. The TS version
//! mutates an untyped argparse Namespace in place; we take the two pieces
//! the logic actually cares about (the `data` list and the `keys` list)
//! via a [`FlexibleTarget`] so callers can back it with any struct.

/// A builder-friendly view of the two lists `flexible_positional` touches.
///
/// Most callers will hold a `DataArgs` (step 2b/N) that implements this
/// via straightforward field access. Exposing the trait lets ad-hoc
/// callers (and tests) drive the function without wrapping unrelated
/// fields in a larger struct.
pub trait FlexibleTarget {
    fn data(&mut self) -> &mut Vec<String>;
    fn keys(&mut self) -> &mut Vec<String>;
}

/// If `value.is_some()`, prepend it onto `target.data()` and — unless
/// `skip_key` is true — add `key_spec` to `target.keys()` (prepended by
/// default, appended when `after_other_args` is true).
///
/// Mirrors the TS semantics exactly: "call in reverse order" is still
/// the caller's responsibility, because each invocation stacks on top of
/// the previous (prepending to both lists).
pub fn flexible_positional<T: FlexibleTarget>(
    target: &mut T,
    value: Option<String>,
    key_spec: &str,
    skip_key: bool,
    after_other_args: bool,
) {
    let value = match value {
        Some(v) => v,
        None => return,
    };

    if !skip_key {
        if after_other_args {
            target.keys().push(key_spec.to_string());
        } else {
            target.keys().insert(0, key_spec.to_string());
        }
    }

    target.data().insert(0, value);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test double with just the two fields flexible_positional reads.
    #[derive(Default, Debug, PartialEq, Eq)]
    struct TestArgs {
        data: Vec<String>,
        keys: Vec<String>,
    }

    impl FlexibleTarget for TestArgs {
        fn data(&mut self) -> &mut Vec<String> {
            &mut self.data
        }
        fn keys(&mut self) -> &mut Vec<String> {
            &mut self.keys
        }
    }

    fn args(data: &[&str], keys: &[&str]) -> TestArgs {
        TestArgs {
            data: data.iter().map(|s| s.to_string()).collect(),
            keys: keys.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn injects_value_and_key_at_front() {
        // Direct analogue of the first TS case: "extraKey" injected with
        // spec "extraKey=default", then "field" injected with spec "field".
        let mut a = args(&["arg1", "arg2"], &["key1", "key2"]);

        flexible_positional(
            &mut a,
            Some("extraArg".into()),
            "extraKey=default",
            false,
            false,
        );
        assert_eq!(
            a,
            args(
                &["extraArg", "arg1", "arg2"],
                &["extraKey=default", "key1", "key2"]
            )
        );

        flexible_positional(&mut a, Some("fieldArg".into()), "field", false, false);
        assert_eq!(
            a,
            args(
                &["fieldArg", "extraArg", "arg1", "arg2"],
                &["field", "extraKey=default", "key1", "key2"],
            )
        );
    }

    #[test]
    fn skip_key_only_moves_value_into_data() {
        // --fieldless path: positional still captured but not bound to a key.
        let mut a = args(&["arg2", "arg3"], &["key1", "key2", "key3"]);
        flexible_positional(&mut a, Some("arg1".into()), "field", true, false);
        assert_eq!(
            a,
            args(&["arg1", "arg2", "arg3"], &["key1", "key2", "key3"])
        );
    }

    #[test]
    fn undefined_value_is_a_noop() {
        let mut a = args(&["arg1", "arg2"], &["key1", "key2"]);
        flexible_positional(&mut a, None, "field", false, false);
        assert_eq!(a, args(&["arg1", "arg2"], &["key1", "key2"]));
    }

    #[test]
    fn after_other_args_appends_key_at_end() {
        let mut a = args(&["arg1", "arg2"], &["key1", "key2"]);
        flexible_positional(&mut a, Some("fieldArg".into()), "field", false, true);
        assert_eq!(
            a,
            args(&["fieldArg", "arg1", "arg2"], &["key1", "key2", "field"],)
        );
    }

    #[test]
    fn empty_target_accepts_injection() {
        let mut a = TestArgs::default();
        flexible_positional(&mut a, Some("v".into()), "k", false, false);
        assert_eq!(a.data, vec!["v"]);
        assert_eq!(a.keys, vec!["k"]);
    }
}
