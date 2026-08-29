//! Ordered physical-template and logical-hydration layout for direct children.

use vize_atelier_core::{ElementType, TemplateChildNode};

use super::{is_plain_element, key::has_dynamic_key, template::is_static_element};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AnchorSlot(usize);

impl AnchorSlot {
    pub fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InsertionPlan {
    Before(AnchorSlot),
    Append,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum LayoutItem {
    Element {
        flat_index: usize,
        element_index: usize,
        referenced: bool,
    },
    TextRun {
        start: usize,
        end: usize,
        element_index: usize,
        dynamic: bool,
    },
    Comment {
        flat_index: usize,
    },
    Inserted {
        flat_index: usize,
        logical_index: usize,
        insertion: InsertionPlan,
    },
    Anchor {
        slot: AnchorSlot,
        element_index: usize,
    },
}

impl LayoutItem {
    pub fn requires_runtime_id(self) -> bool {
        matches!(
            self,
            Self::Element {
                referenced: true,
                ..
            } | Self::TextRun { dynamic: true, .. }
                | Self::Inserted { .. }
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct PendingInsertion {
    flat_index: usize,
    logical_index: usize,
}

/// One linear plan shared by child lowering and static-template generation.
///
/// Plain `<template>` wrappers are transparent. `element_index` addresses the
/// client template. Concrete anchors keep that index aligned with hydration;
/// `logical_index` is retained only for blocks appended after the template.
pub(super) struct ChildLayout<'node, 'alloc> {
    children: std::vec::Vec<&'node TemplateChildNode<'alloc>>,
    items: std::vec::Vec<LayoutItem>,
    anchor_count: usize,
    process_dynamic_keys: bool,
}

impl<'node, 'alloc> ChildLayout<'node, 'alloc> {
    pub fn new(children: &'node [TemplateChildNode<'alloc>], process_dynamic_keys: bool) -> Self {
        let mut flat_children = std::vec::Vec::new();
        flatten_children(children, &mut flat_children);

        let mut layout = Self {
            children: flat_children,
            items: std::vec::Vec::new(),
            anchor_count: 0,
            process_dynamic_keys,
        };
        layout.analyze();
        layout
    }

    pub fn items(&self) -> &[LayoutItem] {
        &self.items
    }

    pub fn child(&self, flat_index: usize) -> &'node TemplateChildNode<'alloc> {
        self.children[flat_index]
    }

    pub fn text_run(&self, start: usize, end: usize) -> &[&'node TemplateChildNode<'alloc>] {
        &self.children[start..end]
    }

    pub fn runtime_items(&self) -> impl Iterator<Item = &LayoutItem> {
        self.items.iter().filter(|item| item.requires_runtime_id())
    }

    pub fn runtime_item_count(&self) -> usize {
        self.runtime_items().count()
    }

    pub fn anchor_count(&self) -> usize {
        self.anchor_count
    }

    pub fn process_dynamic_keys(&self) -> bool {
        self.process_dynamic_keys
    }

    pub fn parent_text_run(&self) -> Option<(usize, usize)> {
        match self.items.as_slice() {
            [
                LayoutItem::TextRun {
                    start,
                    end,
                    dynamic: true,
                    ..
                },
            ] => Some((*start, *end)),
            _ => None,
        }
    }

    pub fn has_dynamic_children(&self) -> bool {
        self.items.iter().any(|item| match item {
            LayoutItem::Element {
                referenced: true, ..
            }
            | LayoutItem::Inserted { .. } => true,
            LayoutItem::TextRun { dynamic: true, .. } => self.items.len() != 1,
            _ => false,
        })
    }

    fn analyze(&mut self) {
        let mut element_index = 0usize;
        let mut logical_index = 0usize;
        let mut pending_insertions = std::vec::Vec::new();

        for flat_index in 0..self.children.len() {
            let child = self.children[flat_index];
            match child {
                TemplateChildNode::Element(element)
                    if element.tag_type != ElementType::Element
                        || matches!(element.tag, "component" | "Component")
                        || is_plain_element(element)
                        || (self.process_dynamic_keys && has_dynamic_key(element)) =>
                {
                    pending_insertions.push(PendingInsertion {
                        flat_index,
                        logical_index,
                    });
                    logical_index += 1;
                }
                TemplateChildNode::If(_) | TemplateChildNode::For(_) => {
                    pending_insertions.push(PendingInsertion {
                        flat_index,
                        logical_index,
                    });
                    logical_index += 1;
                }
                TemplateChildNode::Element(element) => {
                    self.flush_insertions(
                        &mut pending_insertions,
                        &mut element_index,
                        Some(flat_index),
                    );
                    self.items.push(LayoutItem::Element {
                        flat_index,
                        element_index,
                        referenced: !is_static_element(element),
                    });
                    element_index += 1;
                    logical_index += 1;
                }
                TemplateChildNode::Text(_) | TemplateChildNode::Interpolation(_) => {
                    self.flush_insertions(
                        &mut pending_insertions,
                        &mut element_index,
                        Some(flat_index),
                    );

                    let dynamic = matches!(child, TemplateChildNode::Interpolation(_));
                    if let Some(LayoutItem::TextRun {
                        end,
                        dynamic: run_dynamic,
                        ..
                    }) = self.items.last_mut()
                    {
                        *end = flat_index + 1;
                        *run_dynamic |= dynamic;
                    } else {
                        self.items.push(LayoutItem::TextRun {
                            start: flat_index,
                            end: flat_index + 1,
                            element_index,
                            dynamic,
                        });
                        element_index += 1;
                        logical_index += 1;
                    }
                }
                TemplateChildNode::Comment(_) => {
                    self.flush_insertions(
                        &mut pending_insertions,
                        &mut element_index,
                        Some(flat_index),
                    );
                    self.items.push(LayoutItem::Comment { flat_index });
                    element_index += 1;
                    logical_index += 1;
                }
                _ => {}
            }
        }

        self.flush_insertions(&mut pending_insertions, &mut element_index, None);
    }

    fn flush_insertions(
        &mut self,
        pending: &mut std::vec::Vec<PendingInsertion>,
        element_index: &mut usize,
        next_flat_index: Option<usize>,
    ) {
        if pending.is_empty() {
            return;
        }

        for child in pending.drain(..) {
            let insertion = if let Some(next_flat_index) = next_flat_index {
                debug_assert!(child.flat_index < next_flat_index);
                InsertionPlan::Before(AnchorSlot(self.anchor_count))
            } else {
                InsertionPlan::Append
            };
            self.items.push(LayoutItem::Inserted {
                flat_index: child.flat_index,
                logical_index: child.logical_index,
                insertion,
            });

            if let InsertionPlan::Before(slot) = insertion {
                self.items.push(LayoutItem::Anchor {
                    slot,
                    element_index: *element_index,
                });
                self.anchor_count += 1;
                *element_index += 1;
            }
        }
    }
}

fn flatten_children<'node, 'alloc>(
    children: &'node [TemplateChildNode<'alloc>],
    flat: &mut std::vec::Vec<&'node TemplateChildNode<'alloc>>,
) {
    let mut pending = vize_atelier_core::walk_probe::vapor_children(children)
        .rev()
        .collect::<std::vec::Vec<_>>();
    while let Some(child) = pending.pop() {
        if let TemplateChildNode::Element(element) = child
            && element.tag_type == ElementType::Template
        {
            pending.extend(vize_atelier_core::walk_probe::vapor_children(&element.children).rev());
        } else {
            flat.push(child);
        }
    }
}
