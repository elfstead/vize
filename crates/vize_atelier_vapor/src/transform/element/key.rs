use crate::ir::{BlockIRNode, KeyIRNode, OperationNode};
use vize_atelier_core::{ElementNode, ExpressionNode, PropNode, SimpleExpressionNode};
use vize_carton::Box;

use super::{TransformContext, transform_element_without_key};

pub(super) fn has_dynamic_key(element: &ElementNode<'_>) -> bool {
    !has_directive(element, "for")
        && !has_directive(element, "once")
        && dynamic_key_source(element).is_some()
}

pub(super) fn transform_keyed_element<'a>(
    ctx: &mut TransformContext<'a>,
    element: &ElementNode<'a>,
    block: &mut BlockIRNode<'a>,
    existing_id: Option<usize>,
    insertion: Option<crate::ir::InsertionState>,
    add_return: bool,
) {
    let Some(value) = dynamic_key_expression(ctx, element) else {
        transform_element_without_key(ctx, element, block, existing_id, insertion, add_return);
        return;
    };

    let id = existing_id.unwrap_or_else(|| ctx.next_id());
    let mut render = BlockIRNode::new(ctx.allocator);
    transform_element_without_key(ctx, element, &mut render, None, None, true);

    block.operation.push(OperationNode::Key(Box::new_in(
        KeyIRNode {
            id,
            value,
            render,
            insertion,
        },
        ctx.allocator,
    )));
    if add_return {
        block.returns.push(id);
    }
}

pub(super) fn static_key_expression<'a>(
    ctx: &TransformContext<'a>,
    element: &ElementNode<'a>,
) -> Option<Box<'a, SimpleExpressionNode<'a>>> {
    element.props.iter().find_map(|prop| {
        let PropNode::Attribute(attribute) = prop else {
            return None;
        };
        if attribute.name.as_str() != "key" {
            return None;
        }
        let value = attribute.value.as_ref()?;
        Some(Box::new_in(
            SimpleExpressionNode::new(value.content.clone(), true, value.loc.clone()),
            ctx.allocator,
        ))
    })
}

fn dynamic_key_expression<'a>(
    ctx: &TransformContext<'a>,
    element: &ElementNode<'a>,
) -> Option<Box<'a, SimpleExpressionNode<'a>>> {
    let directive = dynamic_key_source(element)?;
    let expression = match directive.exp.as_ref() {
        Some(ExpressionNode::Simple(expression)) => SimpleExpressionNode::new(
            expression.content.clone(),
            expression.is_static,
            expression.loc.clone(),
        ),
        _ => {
            let ExpressionNode::Simple(argument) = directive.arg.as_ref()? else {
                return None;
            };
            SimpleExpressionNode::new(argument.content.clone(), false, argument.loc.clone())
        }
    };
    Some(Box::new_in(expression, ctx.allocator))
}

fn dynamic_key_source<'a>(
    element: &'a ElementNode<'a>,
) -> Option<&'a vize_atelier_core::DirectiveNode<'a>> {
    element.props.iter().find_map(|prop| {
        let PropNode::Directive(directive) = prop else {
            return None;
        };
        if directive.name.as_str() != "bind" {
            return None;
        }
        let Some(ExpressionNode::Simple(argument)) = directive.arg.as_ref() else {
            return None;
        };
        (argument.content.as_str() == "key").then_some(&**directive)
    })
}

fn has_directive(element: &ElementNode<'_>, name: &str) -> bool {
    element
        .props
        .iter()
        .any(|prop| matches!(prop, PropNode::Directive(dir) if dir.name.as_str() == name))
}
