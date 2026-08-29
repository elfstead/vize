//! Regression tests for Vue Vapor RC.6 sibling navigation.
//!
//! RC.6's `next(node)` advances one logical sibling in both client rendering
//! and hydration. Multi-step jumps use `nthChild(parent, index)`:
//!
//! ```js
//! function next(node) {
//!   if (isHydrating) return nextLogicalSibling(node)
//!   return _next(node)
//! }
//! ```

use super::compile_vapor;
use vize_carton::{Allocator, String};

fn compile(template: &str) -> String {
    let allocator = Allocator::new();
    let result = compile_vapor(&allocator, template, Default::default());
    assert!(
        result.error_messages.is_empty(),
        "expected no errors: {:?}",
        result.error_messages
    );
    result.code.clone()
}

/// Split every `_next(base, index)` call in `code` into its two arguments,
/// balancing parentheses so a call base like `_child(n2)` stays intact. Calls
/// without a top-level comma yield `None` as the index.
fn next_calls(code: &str) -> Vec<(&str, Option<&str>)> {
    let bytes = code.as_bytes();
    let mut calls = Vec::new();
    let mut from = 0;
    while let Some(at) = code[from..].find("_next(") {
        let open = from + at + "_next(".len();
        let (mut depth, mut i, mut comma) = (1usize, open, None);
        while i < bytes.len() && depth > 0 {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                b',' if depth == 1 && comma.is_none() => comma = Some(i),
                _ => {}
            }
            i += 1;
        }
        let end = if depth == 0 { i - 1 } else { bytes.len() };
        match comma {
            Some(comma) => calls.push((&code[open..comma], Some(code[comma + 1..end].trim()))),
            None => calls.push((&code[open..end], None)),
        }
        from = open;
    }
    calls
}

/// The reporter's minimal template (#3330): the compiler must navigate three
/// siblings from the first child to reach `<ul>`.
#[test]
fn multi_step_navigation_uses_nth_child_not_a_counted_next() {
    let code = compile(
        r##"<div>
  <svg role="presentation"><use href="#x" /></svg>
  <h2><span>Title</span></h2>
  <p><span>Sub</span></p>
  <ul>
    <li><a href="https://example.com"><img :src="logo" alt="" /><span>A</span></a></li>
    <li><a href="https://example.org"><img :src="logo" alt="" /><span>B</span></a></li>
  </ul>
</div>"##,
    );

    // The `<ul>` is the parent's child at index 3; one `_next` call cannot
    // reach it, so the jump must be an absolute lookup.
    assert!(
        code.contains("_nthChild(n1, 3)"),
        "expected an absolute lookup for the three-sibling jump:\n{code}"
    );
    assert!(
        !code.contains("_next(_child(n1), 3)"),
        "a counted _next advances only one sibling at runtime:\n{code}"
    );
    // The single-sibling hop between the two `<li>` elements stays a `_next`.
    assert!(
        code.contains("_next(n2)"),
        "expected a single-step _next for the adjacent sibling:\n{code}"
    );
    assert!(
        code.contains("nthChild as _nthChild"),
        "the nthChild helper must be imported:\n{code}"
    );
}

/// RC.6 removed the hydration-index argument from `_next` entirely.
#[test]
fn no_emitted_next_call_carries_an_index() {
    // Plain HTML tags only: an unknown tag resolves as a component and never
    // reaches the sibling-navigation path, which would make this vacuous.
    for template in [
        r#"<div><p/><p/><span :id="x"><i/></span></div>"#,
        r#"<div><p/><p/><p/><span :id="x"><i/></span></div>"#,
        r#"<div><span :id="x"><i/></span><p/><p/><span :id="y"><i/></span></div>"#,
        r#"<div><p/><span :id="x"><i/></span><p/><p/><span :id="y"><i/></span><p/><span :id="z"><i/></span></div>"#,
        // A nested parent: navigation inside `<section>` hangs off a `_child`
        // of something other than the root, so the guard below must hold for
        // every parent, not just `n1`.
        r#"<div><section><p/><span :id="x"><i/></span><span :id="y"><i/></span></section><span :id="z"><i/></span></div>"#,
    ] {
        let mut checked_child_bases = 0usize;
        let code = compile(template);
        assert!(
            code.contains("_next(") || code.contains("_nthChild("),
            "expected this template to exercise sibling navigation:\n{template}\n{code}"
        );
        // Every `_next(base)` advances exactly one sibling. Larger jumps must
        // use `_nthChild(parent, index)`.
        for (base, index) in next_calls(&code) {
            assert_eq!(
                index, None,
                "RC.6 next() accepts no hydration index:\n{template}\n{code}"
            );
            checked_child_bases += usize::from(base.starts_with("_child("));
        }
        assert!(
            checked_child_bases > 0 || code.contains("_nthChild("),
            "expected a _child-anchored hop or an absolute lookup:\n{template}\n{code}"
        );
    }
}

/// Index 0 and index 1 keep their existing, correct shapes.
#[test]
fn single_step_navigation_shapes_are_unchanged() {
    let first = compile(r#"<div><a :id="x"/><b/></div>"#);
    assert!(
        first.contains("_child(n1)"),
        "index 0 stays a plain _child:\n{first}"
    );
    assert!(
        !first.contains("_nthChild"),
        "index 0 must not need an absolute lookup:\n{first}"
    );

    let second = compile(r#"<div><a/><b :id="x"/></div>"#);
    assert!(
        second.contains("_next(_child(n1))"),
        "index 1 stays one _next step from the first child:\n{second}"
    );
    assert!(
        !second.contains("_nthChild"),
        "index 1 must not need an absolute lookup:\n{second}"
    );
}

/// Adjacent referenced children use one argument-free `_next` call.
#[test]
fn single_step_next_uses_rc6_signature() {
    // `<span :id="y">` is the parent's child at index 3, exactly one rendered
    // sibling past `<span :id="x">` at index 2.
    let code = compile(r#"<div><p/><p/><span :id="x"><i/></span><span :id="y"><i/></span></div>"#);

    assert_eq!(next_calls(&code), vec![("n0", None)], "{code}");
}

/// Every emitted `_next` call must use the RC.6 one-argument contract.
#[test]
fn every_next_call_is_bare() {
    for template in [
        r#"<div><p/><span :id="x"><i/></span><p/><p/><span :id="y"><i/></span></div>"#,
        r#"<div><span :id="x"><i/></span><p/><p/><p/><span :id="y"><i/></span></div>"#,
    ] {
        let code = compile(template);
        for line in code.lines() {
            let mut from = 0;
            while let Some(at) = line[from..].find("_next(") {
                let open = from + at + "_next(".len();
                // Balance parentheses: the base may itself be a call, e.g.
                // `_next(_child(n2), 1)`.
                let (mut depth, mut i, mut has_top_level_comma) = (1usize, open, false);
                let bytes = line.as_bytes();
                while i < bytes.len() && depth > 0 {
                    match bytes[i] {
                        b'(' => depth += 1,
                        b')' => depth -= 1,
                        b',' if depth == 1 => has_top_level_comma = true,
                        _ => {}
                    }
                    i += 1;
                }
                assert!(
                    !has_top_level_comma,
                    "RC.6 _next must not carry a hydration index:\n{template}\n{line}"
                );
                from = open;
            }
        }
    }
}
