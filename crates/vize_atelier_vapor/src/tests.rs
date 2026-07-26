//! Integration tests for the Vapor compiler entry points.

use super::{compile_vapor, compile_vapor_with_template_syntax};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use vize_atelier_core::TemplateSyntaxMode;
use vize_carton::Bump;

fn normalize_code(code: &str) -> String {
    code.lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_parses_as_module(code: &str) {
    let allocator = Allocator::default();
    let parsed = Parser::new(
        &allocator,
        code,
        SourceType::default()
            .with_module(true)
            .with_typescript(true),
    )
    .parse();

    assert!(
        parsed.errors.is_empty(),
        "generated code should parse, got: {:?}\n\n{}",
        parsed.errors,
        code
    );
}

#[test]
fn test_compile_simple_element() {
    let allocator = Bump::new();
    let result = compile_vapor(&allocator, "<div>hello</div>", Default::default());

    assert!(result.error_messages.is_empty(), "Expected no errors");

    let code = normalize_code(&result.code);

    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_interpolation() {
    let allocator = Bump::new();
    let result = compile_vapor(&allocator, "<div>{{ msg }}</div>", Default::default());

    assert!(result.error_messages.is_empty(), "Expected no errors");

    let code = normalize_code(&result.code);

    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_event() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<button @click="handleClick">Click</button>"#,
        Default::default(),
    );

    assert!(result.error_messages.is_empty(), "Expected no errors");

    let code = normalize_code(&result.code);

    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_v_if() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<div v-if="show">visible</div>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);

    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_v_for() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<div v-for="item in items">{{ item }}</div>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);

    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_nested_dynamic_child_attrs_and_events() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<div><button :class="cls" @click="onClick">x</button></div>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_nested_component_child() {
    let allocator = Bump::new();
    let result = compile_vapor(&allocator, "<div><MyComp /></div>", Default::default());

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_nested_components_use_logical_insertion_indices() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        "<header><Brand /><nav><Link /><Link /></nav><button @click=\"count++\">{{ count }}</button></header>",
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    assert_eq!(code.matches("_setInsertionState(").count(), 3);
    assert_eq!(
        code.lines()
            .filter(|line| line.starts_with("_setInsertionState(")
                && line.ends_with(", 0)")
                && !line.contains("null"))
            .count(),
        1
    );
    assert_eq!(code.matches(", null, 0)").count(), 1);
    assert_eq!(code.matches(", null, 1)").count(), 1);
    assert!(
        code.lines()
            .any(|line| line.contains(" = _child(") && line.ends_with(", 1)"))
    );
    assert!(
        code.lines()
            .any(|line| line.contains(" = _next(") && line.ends_with(", 2)"))
    );
}

#[test]
fn test_compile_middle_block_run_uses_one_anchor_and_distinct_dom_indices() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<div><span/><A/><B/><button :class="x"/></div>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    assert!(code.contains("<div><span></span><!><button"));
    assert!(code.contains("const n3 = _next(_child(n4), 1)"), "{code}");
    assert!(code.contains("const n2 = _next(n3, 3)"), "{code}");
    assert!(code.contains("_setInsertionState(n4, n3, 1)"), "{code}");
    assert!(code.contains("_setInsertionState(n4, n3, 2)"), "{code}");
    assert_eq!(code.matches("<!>").count(), 1);
}

#[test]
fn test_compile_hydration_refs_keep_physical_and_logical_indices_distinct() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<div><Comp/><div/><span/><div v-if="true"/><div><button :disabled="foo"/></div></div>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    assert!(code.contains("_nthChild(n4, 2, 3)"), "{code}");
    assert!(code.contains("_next(n3, 4)"), "{code}");
    assert!(code.contains("_setInsertionState(n4, 0)"), "{code}");
    assert!(code.contains("_setInsertionState(n4, n3, 3)"), "{code}");
}

#[test]
fn test_compile_all_dynamic_children_append_in_logical_order() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<div><Comp1/><div v-if="show">if</div><div v-else>else</div><Comp2/></div>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    assert!(!code.contains("<!>"), "{code}");
    assert!(code.contains("_setInsertionState(n3, null, 0)"), "{code}");
    assert!(code.contains("_setInsertionState(n3, null, 1)"), "{code}");
    assert!(code.contains("_setInsertionState(n3, null, 2)"), "{code}");
}

#[test]
fn test_compile_middle_slot_and_control_flow_use_typed_insertion_state() {
    let allocator = Bump::new();
    for source in [
        "<div><span/><slot/><button/></div>",
        r#"<div><span/><template v-if="ok"><b/></template><button/></div>"#,
        r#"<div><span/><template v-for="item in items"><b/></template><button/></div>"#,
    ] {
        let result = compile_vapor(&allocator, source, Default::default());
        assert!(
            result.error_messages.is_empty(),
            "Expected no errors for {source}: {:?}",
            result.error_messages
        );

        let code = normalize_code(&result.code);
        assert!(code.contains("<div><span></span><!><button"), "{code}");
        assert!(
            code.lines()
                .any(|line| line.starts_with("_setInsertionState(")
                    && line.contains(", n")
                    && line.ends_with(", 1)")),
            "{code}"
        );
        assert!(
            code.lines()
                .filter(|line| line.starts_with("_setInsertionState("))
                .all(|line| !line.contains("true")),
            "{code}"
        );
    }
}

#[test]
fn test_compile_nested_slot_outlet_child() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<div><slot :row="item" :index="i"></slot></div>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    assert_parses_as_module(&code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_component_v_model_uses_update_listener_getter() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<InputBase v-model="searchQuery" />"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_component_props_are_getters() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<NuxtLink :to="to" target="_blank" @click="onClick">about</NuxtLink>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_component_multiline_event_handler_parses() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"
        <AnotherComponent
          @click.stop="
            hoge();
            hoge();
          "
        />
        "#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );
    assert_parses_as_module(&result.code);

    let code = normalize_code(&result.code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_branch_component_under_existing_parent() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<main><template v-if="ok"><MyComp /></template></main>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_component_resolution_is_scoped_per_branch() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"
        <div>
          <template v-if="first"><CodeHighlight /></template>
          <template v-else-if="second"><CodeHighlight /></template>
          <template v-else><CodeHighlight /></template>
        </div>
        "#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_component_resolution_reuses_outer_scope_inside_branch() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"
        <div>
          <CodeHighlight />
          <template v-if="visible"><CodeHighlight /></template>
        </div>
        "#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_nested_if_under_existing_child() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<div><button><template v-if="ok"><span>a</span></template></button></div>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_control_flow_uses_parent_specific_insertion_state() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"
        <div>
          <button>
            <template v-if="dark"><span>a</span></template>
          </button>
          <main>
            <template v-if="tab"><MyComp /></template>
          </main>
        </div>
        "#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_mixed_text_static_and_if_children_preserves_template_shape() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"
        <span class="date-line" :class="{ weekend: isWeekend }">
          {{ dateLabel }}
          <em class="day-offset">same day</em>
          <em v-if="isWeekend" class="weekend-label">Weekend</em>
        </span>
        "#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_separated_dynamic_text_uses_direct_child_ref() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<div><span></span>{{ value }}</div>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    assert!(!code.contains("txt as _txt"), "{code}");
    assert!(
        code.contains("_setText(n0, _toDisplayString(_ctx.value))"),
        "{code}"
    );
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_multiple_dynamic_text_runs_get_distinct_refs() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<div>{{ first }}<span></span>{{ second }}</div>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    assert!(!code.contains("txt as _txt"), "{code}");
    assert_eq!(code.matches("_setText(n").count(), 2, "{code}");
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_dynamic_text_after_component_keeps_logical_index() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<div><Comp />{{ value }}</div>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    assert!(!code.contains("txt as _txt"), "{code}");
    assert!(code.contains("_child(n2, 1)"), "{code}");
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_dynamic_text_around_component_shares_ordered_layout() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<div>{{ before }}<Comp />{{ after }}</div>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    assert!(code.contains("_setInsertionState(n4, n3, 1)"), "{code}");
    assert!(
        code.contains("_setText(n0, _toDisplayString(_ctx.before))"),
        "{code}"
    );
    assert!(
        code.contains("_setText(n2, _toDisplayString(_ctx.after))"),
        "{code}"
    );
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_dynamic_component_dispatch_precedes_child_layout() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<component :is="view"><span>{{ message }}</span></component>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    assert!(code.contains("createDynamicComponent"), "{code}");
    assert!(!code.contains("_template(\"<component"), "{code}");
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_nested_control_flow_avoids_unused_root_insertion_state() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"
        <div>
          <template v-if="ok">
            <section>
              <template v-if="inner"><span>a</span></template>
              <template v-if="more"><i>b</i></template>
            </section>
          </template>
        </div>
        "#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_static_template_ref_uses_template_ref_setter() {
    let allocator = Bump::new();
    let result = compile_vapor(&allocator, r#"<div ref="el"></div>"#, Default::default());

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_dynamic_template_ref_uses_resolved_expression() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<div :ref="setEl"></div>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_v_html_resolves_ctx_and_v_for_aliases() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<div v-for="diagnostic in diagnostics"><div v-html="formatHelp(diagnostic.help)"></div></div>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_nested_static_template_ref_uses_child_ref() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<div><span ref="inner"></span></div>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_template_syntax_quirks_accepts_invalid_html_self_closing() {
    let allocator = Bump::new();
    let result = compile_vapor_with_template_syntax(
        &allocator,
        "<div /><span></span>",
        Default::default(),
        TemplateSyntaxMode::Quirks,
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );
    assert!(!result.code.is_empty());
}

#[test]
fn test_compile_standard_rewrites_invalid_html_self_closing() {
    let allocator = Bump::new();
    let result = compile_vapor(&allocator, "<div /><span></span>", Default::default());

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );
    assert!(!result.code.is_empty());
}

#[test]
fn test_compile_strict_rejects_invalid_html_self_closing() {
    let allocator = Bump::new();
    let result = compile_vapor_with_template_syntax(
        &allocator,
        "<div /><span></span>",
        Default::default(),
        TemplateSyntaxMode::Strict,
    );

    assert!(
        result
            .error_messages
            .iter()
            .any(|message| message.contains("Invalid self-closing syntax"))
    );
    assert!(result.code.is_empty());
}

#[test]
fn test_compile_complex_comparison_expression() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<button :class="['main-tab', { active: tab === 'atelier' }]">x</button>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_v_for_aliases_in_complex_expressions() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<ul><li v-for="item in items" :class="['row', { active: selected.has(item.id) }, `kind-${item.kind}`]" @click="pick(item.id)">{{ item.name }}</li></ul>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_v_for_destructured_aliases_resolve_source_paths() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<ul><li v-for="{ id: itemId, user: { name }, meta: { count: total = 0 } } in rows" :key="itemId" :title="name">{{ total }}</li></ul>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    assert_parses_as_module(&result.code);
    assert!(
        result
            .code
            .contains(r#""title", _for_item0.value.user.name)"#),
        "{}",
        result.code
    );
    assert!(
        result
            .code
            .contains("_toDisplayString(_for_item0.value.meta.count))"),
        "{}",
        result.code
    );
    assert!(
        result.code.contains(
            "}, ({ id: itemId, user: { name }, meta: { count: total = 0 } }) => (itemId))"
        ),
        "{}",
        result.code
    );
}

#[test]
fn test_compile_nested_v_for_key_uses_outer_alias() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<div><template v-for="n in 4" :key="`set-${n}`"><span v-for="(icon, i) in icons" :key="`${n}-${i}`" :class="icon">{{ icon }}</span></template></div>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_first_dynamic_child_after_static_sibling() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<div><span>static</span><button :class="cls">x</button></div>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_dynamic_child_after_multiple_static_siblings() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<div><header>one</header><p>two</p><button :class="cls">x</button></div>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    assert!(!code.contains("child as _child"), "{code}");
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_first_dynamic_child_after_static_text() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<div><span>label <span :class="cls">{{ msg }}</span></span></div>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_self_closing_svg_children_stay_siblings() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<svg><path d="a" /><path d="b" /></svg>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_dynamic_siblings_around_control_flow_children() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"
        <section>
          <div class="tabs">
            <button :class="['tab', { active: activeTab === 'code' }]" @click="activeTab = 'code'">
              Code
            </button>
            <button
              v-if="inputMode === 'sfc'"
              :class="['tab', { active: activeTab === 'bindings' }]"
              @click="activeTab = 'bindings'"
            >
              Bindings
            </button>
            <button
              :class="['tab', { active: activeTab === 'helpers' }]"
              @click="activeTab = 'helpers'"
            >
              Helpers
            </button>
          </div>
        </section>
        "#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_dynamic_text_escapes_multiline_static_part() {
    let allocator = Bump::new();
    // Condense mode collapses the `\n` in the static part to a space
    // (Vue parity, #960), so a `<pre>` wrapper preserves it for this
    // escape-handling check.
    let result = compile_vapor(
        &allocator,
        r#"<pre :class="cls">{{ count }} all
selected</pre>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    assert!(result.code.contains(r#"" all\nselected""#));
    assert_parses_as_module(result.code.as_str());
}

#[test]
fn test_compile_slot_outlet_preserves_static_name_props_and_fallback() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<slot name="header" :item="x"><span>fallback</span></slot>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    assert_parses_as_module(&code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_slot_outlet_preserves_dynamic_name() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<slot :name="slotName" :item="x">fallback</slot>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );

    let code = normalize_code(&result.code);
    assert_parses_as_module(&code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_custom_directive_preserves_payloads() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<div v-focus:[placement].lazy="handler" />"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );
    assert_parses_as_module(&result.code);

    let code = normalize_code(&result.code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_v_cloak_uses_builtin_lowering() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<div v-cloak>{{ msg }}</div>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );
    assert_parses_as_module(&result.code);

    let code = normalize_code(&result.code);
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_v_once_lowers_without_runtime_directives() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<div v-once>{{ msg }}</div>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );
    assert_parses_as_module(&result.code);

    let code = normalize_code(&result.code);
    assert!(!code.contains("_withDirectives"), "{code}");
    assert!(!code.contains("_renderEffect"), "{code}");
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_v_memo_empty_array_lowers_without_runtime_directives() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<div v-memo="[]">{{ msg }}</div>"#,
        Default::default(),
    );

    assert!(
        result.error_messages.is_empty(),
        "Expected no errors: {:?}",
        result.error_messages
    );
    assert_parses_as_module(&result.code);

    let code = normalize_code(&result.code);
    assert!(!code.contains("_withDirectives"), "{code}");
    assert!(!code.contains("_renderEffect"), "{code}");
    insta::assert_snapshot!(code.as_str());
}

#[test]
fn test_compile_v_memo_with_dependencies_reports_diagnostic() {
    let allocator = Bump::new();
    let result = compile_vapor(
        &allocator,
        r#"<div v-memo="[msg]">{{ msg }}</div>"#,
        Default::default(),
    );

    assert_eq!(
        result.error_messages,
        vec![String::from(
            "v-memo with dependencies is not supported in Vapor yet. Use v-once or v-memo=\"[]\" until memo guards are implemented.",
        )]
    );
    assert_parses_as_module(&result.code);
    assert!(!result.code.contains("_withDirectives"), "{}", result.code);
    assert!(!result.code.contains("_memo"), "{}", result.code);
}
