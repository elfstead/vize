//! Lowering for elements whose direct children need runtime work.

use crate::ir::{
    ChildRefIRNode, InsertionAnchor, InsertionState, NextRefIRNode, OperationNode, SlotOutletIRNode,
};
use vize_atelier_core::{ElementNode, ElementType, PropNode, TemplateChildNode};

use super::super::control::{transform_for_node_into_parent, transform_if_node_into_parent};
use super::child_layout::{ChildLayout, DynamicChild, InsertionPlan};
use super::component::transform_component;
use super::template::{generate_element_template, transform_template_ref};
use super::{
    BlockIRNode, TransformContext, get_slot_outlet_name, get_slot_outlet_props, transform_children,
    transform_directive, transform_text_children,
};

#[derive(Clone, Copy)]
struct RefTarget {
    id: usize,
    element_index: usize,
    logical_index: usize,
}

/// Transform an element after reserving IDs for all dynamic children.
pub(super) fn transform_element_with_dynamic_children<'a>(
    ctx: &mut TransformContext<'a>,
    element: &ElementNode<'a>,
    block: &mut BlockIRNode<'a>,
) {
    let layout = ChildLayout::new(&element.children);
    let child_ids = allocate_ids(ctx, layout.dynamic().len());
    let anchor_ids = allocate_ids(ctx, layout.anchors().len());
    let parent_id = ctx.next_id();

    transform_element_runtime_work(ctx, element, parent_id, block);
    transform_layout_children(ctx, &layout, parent_id, block, &child_ids, &anchor_ids);

    ctx.add_template(parent_id, generate_element_template(element));
    block.returns.push(parent_id);
}

fn transform_element_runtime_work<'a>(
    ctx: &mut TransformContext<'a>,
    element: &ElementNode<'a>,
    element_id: usize,
    block: &mut BlockIRNode<'a>,
) {
    for prop in element.props.iter() {
        if let PropNode::Directive(directive) = prop {
            transform_directive(ctx, directive, element_id, element, block);
        }
    }

    transform_template_ref(ctx, element, element_id, block);

    let has_text = element.children.iter().any(|child| {
        matches!(
            child,
            TemplateChildNode::Text(_) | TemplateChildNode::Interpolation(_)
        )
    });
    let has_interpolation = element
        .children
        .iter()
        .any(|child| matches!(child, TemplateChildNode::Interpolation(_)));

    if has_text && has_interpolation {
        transform_text_children(ctx, &element.children, element_id, block);
    }
}

fn transform_layout_children<'node, 'a>(
    ctx: &mut TransformContext<'a>,
    layout: &ChildLayout<'node, 'a>,
    parent_id: usize,
    block: &mut BlockIRNode<'a>,
    child_ids: &[usize],
    anchor_ids: &[usize],
) {
    emit_ref_operations(layout, parent_id, block, child_ids, anchor_ids);

    for (dynamic, child_id) in layout.dynamic().iter().zip(child_ids.iter().copied()) {
        let child = layout.children()[dynamic.flat_index];
        match child {
            TemplateChildNode::Element(element)
                if element.tag_type == ElementType::Element
                    && element.tag.as_str() != "component" =>
            {
                transform_existing_element(ctx, element, child_id, block);
            }
            TemplateChildNode::Element(element) if element.tag_type == ElementType::Slot => {
                transform_slot_outlet_child(
                    ctx,
                    element,
                    child_id,
                    block,
                    resolve_insertion(parent_id, *dynamic, anchor_ids),
                );
            }
            TemplateChildNode::Element(element) => {
                transform_component(
                    ctx,
                    element,
                    block,
                    Some(child_id),
                    resolve_insertion(parent_id, *dynamic, anchor_ids),
                    false,
                );
            }
            TemplateChildNode::If(if_node) => {
                transform_if_node_into_parent(
                    ctx,
                    if_node,
                    block,
                    child_id,
                    resolve_insertion(parent_id, *dynamic, anchor_ids)
                        .expect("v-if children require insertion state"),
                );
            }
            TemplateChildNode::For(for_node) => {
                transform_for_node_into_parent(
                    ctx,
                    for_node,
                    block,
                    child_id,
                    resolve_insertion(parent_id, *dynamic, anchor_ids)
                        .expect("v-for children require insertion state"),
                );
            }
            _ => {}
        }
    }
}

fn transform_existing_element<'a>(
    ctx: &mut TransformContext<'a>,
    element: &ElementNode<'a>,
    element_id: usize,
    block: &mut BlockIRNode<'a>,
) {
    transform_element_runtime_work(ctx, element, element_id, block);

    let layout = ChildLayout::new(&element.children);
    if !layout.has_dynamic_children() {
        return;
    }

    let child_ids = allocate_ids(ctx, layout.dynamic().len());
    let anchor_ids = allocate_ids(ctx, layout.anchors().len());
    transform_layout_children(ctx, &layout, element_id, block, &child_ids, &anchor_ids);
}

fn transform_slot_outlet_child<'a>(
    ctx: &mut TransformContext<'a>,
    element: &ElementNode<'a>,
    element_id: usize,
    block: &mut BlockIRNode<'a>,
    insertion: Option<InsertionState>,
) {
    let name = get_slot_outlet_name(ctx, element);
    let props = get_slot_outlet_props(ctx, element);
    let fallback =
        (!element.children.is_empty()).then(|| transform_children(ctx, &element.children));
    block
        .operation
        .push(OperationNode::SlotOutlet(SlotOutletIRNode {
            id: element_id,
            name,
            props,
            fallback,
            insertion,
        }));
}

fn emit_ref_operations<'a>(
    layout: &ChildLayout<'_, 'a>,
    parent_id: usize,
    block: &mut BlockIRNode<'a>,
    child_ids: &[usize],
    anchor_ids: &[usize],
) {
    let mut targets = std::vec::Vec::with_capacity(child_ids.len() + anchor_ids.len());

    for (dynamic, id) in layout.dynamic().iter().zip(child_ids.iter().copied()) {
        if let Some(element_index) = dynamic.element_index {
            targets.push(RefTarget {
                id,
                element_index,
                logical_index: dynamic.logical_index,
            });
        }
    }
    for (anchor, id) in layout.anchors().iter().zip(anchor_ids.iter().copied()) {
        targets.push(RefTarget {
            id,
            element_index: anchor.element_index,
            logical_index: anchor.logical_index,
        });
    }

    targets.sort_by_key(|target| target.element_index);
    let mut previous: Option<RefTarget> = None;
    for target in targets {
        if let Some(prev) = previous
            && target.element_index == prev.element_index + 1
        {
            block.operation.push(OperationNode::NextRef(NextRefIRNode {
                child_id: target.id,
                prev_id: prev.id,
                logical_index: target.logical_index,
            }));
        } else {
            block
                .operation
                .push(OperationNode::ChildRef(ChildRefIRNode {
                    child_id: target.id,
                    parent_id,
                    element_index: target.element_index,
                    logical_index: target.logical_index,
                }));
        }
        previous = Some(target);
    }
}

fn resolve_insertion(
    parent: usize,
    dynamic: DynamicChild,
    anchor_ids: &[usize],
) -> Option<InsertionState> {
    let anchor = match dynamic.insertion? {
        InsertionPlan::Prepend => InsertionAnchor::Prepend,
        InsertionPlan::Before(anchor_index) => InsertionAnchor::Before(anchor_ids[anchor_index]),
        InsertionPlan::Append => InsertionAnchor::Append,
    };
    Some(InsertionState {
        parent,
        anchor,
        logical_index: dynamic.logical_index,
    })
}

fn allocate_ids(ctx: &mut TransformContext<'_>, count: usize) -> std::vec::Vec<usize> {
    (0..count).map(|_| ctx.next_id()).collect()
}
