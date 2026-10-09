// SPDX-License-Identifier: GPL-3.0-only
//! Wire unit tests: the balanced-brace repair_json cases, unit-pinned pure
//! over inputs and outputs.

use super::repair_json;

#[test]
fn repair_json_extracts_the_first_balanced_object_from_prose() {
    assert_eq!(
        repair_json(r#"Here you go: {"verdict":"spam"} hope it helps"#),
        Some(r#"{"verdict":"spam"}"#),
    );
    assert_eq!(
        repair_json(r#"{"outer":{"inner":"deep""#),
        None,
        "an unbalanced object is not repaired",
    );
    assert_eq!(repair_json("no braces at all"), None);
    assert_eq!(repair_json(""), None);
}

#[test]
fn repair_json_ignores_braces_and_quotes_inside_strings() {
    assert_eq!(
        repair_json(r#"prose {"text":"a } brace and an \" escape"} tail"#),
        Some(r#"{"text":"a } brace and an \" escape"}"#),
    );
    assert_eq!(
        repair_json(r#"{"a":{"b":"{ nested }"},"c":1}"#),
        Some(r#"{"a":{"b":"{ nested }"},"c":1}"#),
        "the whole outer object is the first balanced one",
    );
}
