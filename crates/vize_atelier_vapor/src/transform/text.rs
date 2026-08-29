//! Text and interpolation transformation.
//!
//! Handles `TextNode`, `InterpolationNode`, and mixed text/interpolation children.

use vize_carton::{Box, Vec};

use crate::ir::{BlockIRNode, OperationNode, SetTextIRNode};
use vize_atelier_core::{
    ExpressionNode, InterpolationNode, SimpleExpressionNode, SourceLocation, TemplateChildNode,
    TextNode,
};

use super::context::TransformContext;

/// Transform text node
pub(crate) fn transform_text<'a>(
    ctx: &mut TransformContext<'a>,
    text: &TextNode,
    block: &mut BlockIRNode<'a>,
) {
    let element_id = ctx.next_id();
    let template: vize_carton::String = text.content.into();
    ctx.add_template(element_id, template);
    block.returns.push(element_id);
}

/// Transform interpolation node (standalone, not inside element)
pub(crate) fn transform_interpolation<'a>(
    ctx: &mut TransformContext<'a>,
    interp: &InterpolationNode<'a>,
    block: &mut BlockIRNode<'a>,
) {
    let element_id = ctx.next_id();

    // Register a space placeholder template for standalone interpolations
    // (when not inside a parent element that already provides the template)
    ctx.add_template(element_id, vize_carton::String::from(" "));
    ctx.standalone_text_elements.insert(element_id);

    // Create SetText operation
    let values = match &interp.content {
        ExpressionNode::Simple(simple) => {
            let mut v = Vec::new_in(&ctx.allocator);
            let exp = SimpleExpressionNode::from_node(simple);
            v.push(Box::new_in(exp, &ctx.allocator));
            v
        }
        _ => Vec::new_in(&ctx.allocator),
    };

    let set_text = SetTextIRNode {
        element: element_id,
        values,
    };

    ctx.push_dynamic_operation(block, OperationNode::SetText(set_text));

    block.returns.push(element_id);
}

/// Transform one contiguous text/interpolation run.
pub(crate) fn transform_text_children<'a, 'node>(
    ctx: &mut TransformContext<'a>,
    children: impl IntoIterator<Item = &'node TemplateChildNode<'a>>,
    target_id: usize,
    block: &mut BlockIRNode<'a>,
) where
    'a: 'node,
{
    let mut values = Vec::new_in(&ctx.allocator);

    for child in children {
        match child {
            TemplateChildNode::Text(text) => {
                // Static text part
                let exp = SimpleExpressionNode::new(
                    text.content,
                    true, // is_static = true
                    SourceLocation::STUB,
                );
                values.push(Box::new_in(exp, &ctx.allocator));
            }
            TemplateChildNode::Interpolation(interp) => {
                // Dynamic interpolation
                if let ExpressionNode::Simple(simple) = &interp.content {
                    let exp = SimpleExpressionNode::from_node(simple);
                    values.push(Box::new_in(exp, &ctx.allocator));
                }
            }
            _ => {}
        }
    }

    if !values.is_empty() {
        let set_text = SetTextIRNode {
            element: target_id,
            values,
        };

        ctx.push_dynamic_operation(block, OperationNode::SetText(set_text));
    }
}
