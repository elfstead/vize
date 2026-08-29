use crate::ir::{
    ChildRefIRNode, GetTextChildIRNode, InsertNodeIRNode, NextRefIRNode, PrependNodeIRNode,
    SetTemplateRefIRNode,
};
use vize_carton::cstr;

use super::super::context::GenerateContext;

/// Generate SetTemplateRef
pub(super) fn generate_set_template_ref(
    ctx: &mut GenerateContext,
    set_ref: &SetTemplateRefIRNode<'_>,
) {
    let element = cstr!("n{}", set_ref.element);

    let value = if set_ref.value.is_static {
        cstr!("\"{}\"", set_ref.value.content)
    } else {
        ctx.resolve_expression_node(&set_ref.value)
    };

    if set_ref.ref_for {
        ctx.push_line_fmt(format_args!(
            "_setRef({}, {}, undefined, true)",
            element, value
        ));
    } else {
        ctx.push_line_fmt(format_args!("_setRef({}, {})", element, value));
    }
}

/// Generate InsertNode
///
/// The Vapor runtime signature is `insert(block, parent, anchor?)`; multiple
/// blocks are passed as one array block, a single block stays bare.
pub(super) fn generate_insert_node(ctx: &mut GenerateContext, insert: &InsertNodeIRNode) {
    ctx.use_helper("insert");
    let parent = cstr!("n{}", insert.parent);
    let elements = insert
        .elements
        .iter()
        .map(|e| cstr!("n{e}"))
        .collect::<std::vec::Vec<_>>()
        .join(", ");
    let block = if insert.elements.len() > 1 {
        cstr!("[{}]", elements)
    } else {
        cstr!("{}", elements)
    };

    if let Some(anchor) = insert.anchor {
        ctx.push_line_fmt(format_args!("_insert({}, {}, n{})", block, parent, anchor));
    } else {
        ctx.push_line_fmt(format_args!("_insert({}, {})", block, parent));
    }
}

/// Generate PrependNode
///
/// The Vapor runtime signature is `prepend(parent, ...blocks)`.
pub(super) fn generate_prepend_node(ctx: &mut GenerateContext, prepend: &PrependNodeIRNode) {
    ctx.use_helper("prepend");
    let parent = cstr!("n{}", prepend.parent);
    let elements = prepend
        .elements
        .iter()
        .map(|e| cstr!("n{e}"))
        .collect::<std::vec::Vec<_>>()
        .join(", ");

    ctx.push_line_fmt(format_args!("_prepend({}, {})", parent, elements));
}

/// Generate GetTextChild
pub(super) fn generate_get_text_child(ctx: &mut GenerateContext, get_text: &GetTextChildIRNode) {
    let parent = cstr!("n{}", get_text.parent);
    let child = ctx.next_temp();

    ctx.push_line_fmt(format_args!("const {} = {}.firstChild", child, parent));
}

/// Generate a direct child reference with the cheapest matching runtime helper.
pub(super) fn generate_child_ref(ctx: &mut GenerateContext, child_ref: &ChildRefIRNode) {
    match child_ref.element_index {
        0 => {
            ctx.use_helper("child");
            ctx.push_line_fmt(format_args!(
                "const n{} = _child(n{})",
                child_ref.child_id, child_ref.parent_id
            ));
        }
        1 => {
            ctx.use_helper("child");
            ctx.use_helper("next");
            ctx.push_line_fmt(format_args!(
                "const n{} = _next(_child(n{}))",
                child_ref.child_id, child_ref.parent_id
            ));
        }
        _ => {
            ctx.use_helper("nthChild");
            ctx.push_line_fmt(format_args!(
                "const n{} = _nthChild(n{}, {})",
                child_ref.child_id, child_ref.parent_id, child_ref.element_index
            ));
        }
    }
}

/// Generate NextRef (_next helper)
pub(super) fn generate_next_ref(ctx: &mut GenerateContext, next_ref: &NextRefIRNode) {
    ctx.use_helper("next");
    ctx.push_line_fmt(format_args!(
        "const n{} = _next(n{})",
        next_ref.child_id, next_ref.prev_id
    ));
}
