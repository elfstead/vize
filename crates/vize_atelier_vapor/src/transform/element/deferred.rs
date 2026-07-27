//! Lowering for elements whose direct children need runtime work.

use crate::ir::{ChildRefIRNode, InsertionAnchor, InsertionState, NextRefIRNode, OperationNode};
use vize_atelier_core::{ElementNode, PropNode, TemplateChildNode};

use super::super::control::{transform_for_node_into_parent, transform_if_node_into_parent};
use super::super::directive::transform_directive;
use super::super::text::transform_text_children;
use super::child_layout::{ChildLayout, InsertionPlan, LayoutItem};
use super::template::{generate_element_template_with_layout, transform_template_ref};
use super::{BlockIRNode, TransformContext, transform_existing_element};

#[derive(Clone, Copy)]
struct RefTarget {
    id: usize,
    element_index: usize,
    logical_index: usize,
}

/// Transform an element after reserving IDs for all runtime child items.
pub(super) fn transform_element_with_dynamic_children<'node, 'a>(
    ctx: &mut TransformContext<'a>,
    element: &ElementNode<'a>,
    layout: &ChildLayout<'node, 'a>,
    block: &mut BlockIRNode<'a>,
    existing_id: Option<usize>,
    add_return: bool,
) -> usize {
    let child_ids = allocate_ids(ctx, layout.runtime_item_count());
    let anchor_ids = allocate_ids(ctx, layout.anchor_count());
    let parent_id = existing_id.unwrap_or_else(|| ctx.next_id());

    transform_element_runtime_work(ctx, element, layout, parent_id, block);
    transform_layout_children(ctx, layout, parent_id, block, &child_ids, &anchor_ids);

    if existing_id.is_none() {
        ctx.add_template(
            parent_id,
            generate_element_template_with_layout(element, layout),
        );
    }
    if add_return {
        block.returns.push(parent_id);
    }
    parent_id
}

pub(super) fn transform_element_runtime_work<'node, 'a>(
    ctx: &mut TransformContext<'a>,
    element: &ElementNode<'a>,
    layout: &ChildLayout<'node, 'a>,
    element_id: usize,
    block: &mut BlockIRNode<'a>,
) {
    for prop in element.props.iter() {
        if let PropNode::Directive(directive) = prop {
            transform_directive(ctx, directive, element_id, element, block);
        }
    }

    transform_template_ref(ctx, element, element_id, block);

    if let Some((start, end)) = layout.parent_text_run() {
        transform_text_children(
            ctx,
            layout.text_run(start, end).iter().copied(),
            element_id,
            block,
        );
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

    for (item, child_id) in layout.runtime_items().zip(child_ids.iter().copied()) {
        match *item {
            LayoutItem::Element {
                flat_index,
                referenced: true,
                ..
            } => {
                let TemplateChildNode::Element(element) = layout.child(flat_index) else {
                    unreachable!("element layout item must reference an element");
                };
                transform_existing_element(ctx, element, child_id, None, block);
            }
            LayoutItem::TextRun {
                start,
                end,
                dynamic: true,
                ..
            } => {
                ctx.standalone_text_elements.insert(child_id);
                transform_text_children(
                    ctx,
                    layout.text_run(start, end).iter().copied(),
                    child_id,
                    block,
                );
            }
            LayoutItem::Inserted {
                flat_index,
                logical_index,
                insertion,
            } => {
                let insertion = resolve_insertion(parent_id, logical_index, insertion, anchor_ids);
                match layout.child(flat_index) {
                    TemplateChildNode::Element(element) => {
                        transform_existing_element(ctx, element, child_id, Some(insertion), block);
                    }
                    TemplateChildNode::If(if_node) => {
                        transform_if_node_into_parent(ctx, if_node, block, child_id, insertion);
                    }
                    TemplateChildNode::For(for_node) => {
                        transform_for_node_into_parent(ctx, for_node, block, child_id, insertion);
                    }
                    _ => {}
                }
            }
            _ => unreachable!("runtime iterator returned a non-runtime layout item"),
        }
    }
}

fn emit_ref_operations<'a>(
    layout: &ChildLayout<'_, 'a>,
    parent_id: usize,
    block: &mut BlockIRNode<'a>,
    child_ids: &[usize],
    anchor_ids: &[usize],
) {
    let mut child_ids = child_ids.iter().copied();
    let mut previous: Option<RefTarget> = None;

    for item in layout.items() {
        let target = match *item {
            LayoutItem::Element {
                element_index,
                logical_index,
                referenced: true,
                ..
            }
            | LayoutItem::TextRun {
                element_index,
                logical_index,
                dynamic: true,
                ..
            } => Some(RefTarget {
                id: child_ids
                    .next()
                    .expect("runtime layout item must have an allocated ID"),
                element_index,
                logical_index,
            }),
            LayoutItem::Inserted { .. } => {
                child_ids
                    .next()
                    .expect("inserted layout item must have an allocated ID");
                None
            }
            LayoutItem::Anchor {
                slot,
                element_index,
                logical_index,
            } => Some(RefTarget {
                id: anchor_ids[slot.index()],
                element_index,
                logical_index,
            }),
            _ => None,
        };
        let Some(target) = target else {
            continue;
        };
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
    debug_assert!(child_ids.next().is_none());
}

fn resolve_insertion(
    parent: usize,
    logical_index: usize,
    insertion: InsertionPlan,
    anchor_ids: &[usize],
) -> InsertionState {
    let anchor = match insertion {
        InsertionPlan::Prepend => InsertionAnchor::Prepend,
        InsertionPlan::Before(slot) => InsertionAnchor::Before(anchor_ids[slot.index()]),
        InsertionPlan::Append => InsertionAnchor::Append,
    };
    InsertionState {
        parent,
        anchor,
        logical_index,
    }
}

fn allocate_ids(ctx: &mut TransformContext<'_>, count: usize) -> std::vec::Vec<usize> {
    (0..count).map(|_| ctx.next_id()).collect()
}
