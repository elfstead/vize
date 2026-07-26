//! Physical template and logical hydration positions for direct children.

use vize_atelier_core::{ElementType, TemplateChildNode};

use super::template::is_static_element;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InsertionPlan {
    Prepend,
    Before(usize),
    Append,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct DynamicChild {
    pub flat_index: usize,
    pub logical_index: usize,
    pub element_index: Option<usize>,
    pub insertion: Option<InsertionPlan>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Anchor {
    pub before_flat_index: usize,
    pub element_index: usize,
    pub logical_index: usize,
}

/// A single linear analysis of the DOM children materialized by one element.
///
/// `<template>` wrappers are transparent. `element_index` addresses nodes in
/// the client template, while `logical_index` addresses SSR output where
/// components and control-flow blocks already occupy sibling positions.
pub(super) struct ChildLayout<'node, 'alloc> {
    children: std::vec::Vec<&'node TemplateChildNode<'alloc>>,
    dynamic: std::vec::Vec<DynamicChild>,
    anchors: std::vec::Vec<Anchor>,
}

impl<'node, 'alloc> ChildLayout<'node, 'alloc> {
    pub fn new(children: &'node [TemplateChildNode<'alloc>]) -> Self {
        let mut flat_children = std::vec::Vec::new();
        flatten_children(children, &mut flat_children);

        let mut layout = Self {
            children: flat_children,
            dynamic: std::vec::Vec::new(),
            anchors: std::vec::Vec::new(),
        };
        layout.analyze();
        layout
    }

    pub fn children(&self) -> &[&'node TemplateChildNode<'alloc>] {
        &self.children
    }

    pub fn dynamic(&self) -> &[DynamicChild] {
        &self.dynamic
    }

    pub fn anchors(&self) -> &[Anchor] {
        &self.anchors
    }

    pub fn has_dynamic_children(&self) -> bool {
        !self.dynamic.is_empty()
    }

    fn analyze(&mut self) {
        let mut element_index = 0usize;
        let mut logical_index = 0usize;
        let mut pending_insertions = std::vec::Vec::new();
        let mut in_text_run = false;

        for flat_index in 0..self.children.len() {
            let child = self.children[flat_index];
            match child {
                TemplateChildNode::Element(element)
                    if element.tag_type != ElementType::Element
                        || element.tag.as_str() == "component" =>
                {
                    self.dynamic.push(DynamicChild {
                        flat_index,
                        logical_index,
                        element_index: None,
                        insertion: None,
                    });
                    pending_insertions.push(self.dynamic.len() - 1);
                    logical_index += 1;
                    in_text_run = false;
                }
                TemplateChildNode::If(_) | TemplateChildNode::For(_) => {
                    self.dynamic.push(DynamicChild {
                        flat_index,
                        logical_index,
                        element_index: None,
                        insertion: None,
                    });
                    pending_insertions.push(self.dynamic.len() - 1);
                    logical_index += 1;
                    in_text_run = false;
                }
                TemplateChildNode::Element(element) => {
                    self.flush_insertions(&mut pending_insertions, &mut element_index, flat_index);
                    if !is_static_element(element) {
                        self.dynamic.push(DynamicChild {
                            flat_index,
                            logical_index,
                            element_index: Some(element_index),
                            insertion: None,
                        });
                    }
                    element_index += 1;
                    logical_index += 1;
                    in_text_run = false;
                }
                TemplateChildNode::Text(_) | TemplateChildNode::Interpolation(_) => {
                    self.flush_insertions(&mut pending_insertions, &mut element_index, flat_index);
                    if !in_text_run {
                        element_index += 1;
                        logical_index += 1;
                        in_text_run = true;
                    }
                }
                _ => {}
            }
        }

        for dynamic_index in pending_insertions {
            self.dynamic[dynamic_index].insertion = Some(InsertionPlan::Append);
        }
    }

    fn flush_insertions(
        &mut self,
        pending: &mut std::vec::Vec<usize>,
        element_index: &mut usize,
        next_flat_index: usize,
    ) {
        if pending.is_empty() {
            return;
        }

        let insertion = if *element_index == 0 {
            InsertionPlan::Prepend
        } else {
            let anchor_index = self.anchors.len();
            let first = self.dynamic[pending[0]];
            self.anchors.push(Anchor {
                before_flat_index: first.flat_index.min(next_flat_index),
                element_index: *element_index,
                logical_index: first.logical_index,
            });
            *element_index += 1;
            InsertionPlan::Before(anchor_index)
        };

        for dynamic_index in pending.drain(..) {
            self.dynamic[dynamic_index].insertion = Some(insertion);
        }
    }
}

fn flatten_children<'node, 'alloc>(
    children: &'node [TemplateChildNode<'alloc>],
    flat: &mut std::vec::Vec<&'node TemplateChildNode<'alloc>>,
) {
    for child in children {
        if let TemplateChildNode::Element(element) = child
            && element.tag_type == ElementType::Template
        {
            flatten_children(&element.children, flat);
        } else {
            flat.push(child);
        }
    }
}
