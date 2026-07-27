use crate::ir::{KeyIRNode, SetBlockKeyIRNode};
use vize_carton::{FxHashMap, cstr};

use super::{
    super::{context::GenerateContext, generate_block, setup::escape_js_string_literal},
    insertion::emit_insertion_state,
};

pub(super) fn generate_key(
    ctx: &mut GenerateContext,
    key: &KeyIRNode<'_>,
    element_template_map: &FxHashMap<usize, usize>,
) {
    ctx.use_helper("createKeyedFragment");
    emit_insertion_state(ctx, key.insertion);

    let value = ctx.resolve_expression(&key.value.content);
    ctx.push_line(&cstr!(
        "const n{} = _createKeyedFragment(() => ({}), () => {{",
        key.id,
        value
    ));
    let was_fragment = ctx.is_fragment;
    ctx.is_fragment = true;
    ctx.indent();
    ctx.push_component_scope();
    generate_block(ctx, &key.render, element_template_map);
    ctx.pop_component_scope();
    ctx.deindent();
    ctx.is_fragment = was_fragment;
    ctx.push_line("})");
}

pub(super) fn generate_set_block_key(ctx: &mut GenerateContext, key: &SetBlockKeyIRNode<'_>) {
    ctx.use_helper("setBlockKey");
    let value = if key.value.is_static {
        cstr!(
            "\"{}\"",
            escape_js_string_literal(key.value.content.as_str())
        )
    } else {
        ctx.resolve_expression(&key.value.content)
    };
    ctx.push_line(&cstr!("_setBlockKey(n{}, {})", key.element, value));
}
