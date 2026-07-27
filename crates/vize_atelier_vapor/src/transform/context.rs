//! Transform context for tracking state during AST-to-IR transformation.

use crate::ir::{BlockIRNode, IREffect, OperationNode};
use vize_carton::{Bump, FxHashMap, FxHashSet, String, Vec};

/// Transform context
pub(crate) struct TransformContext<'a> {
    pub(crate) allocator: &'a Bump,
    temp_id: usize,
    pub(crate) templates: Vec<'a, String>,
    pub(crate) element_template_map: FxHashMap<usize, usize>,
    pub(crate) standalone_text_elements: FxHashSet<usize>,
    non_reactive_scopes: usize,
    suppressed_key_transforms: usize,
    suppressed_non_reactive_classification: usize,
    pub(crate) diagnostics: std::vec::Vec<String>,
}

impl<'a> TransformContext<'a> {
    pub(crate) fn new(allocator: &'a Bump) -> Self {
        Self {
            allocator,
            temp_id: 0,
            templates: Vec::new_in(allocator),
            element_template_map: FxHashMap::default(),
            standalone_text_elements: FxHashSet::default(),
            non_reactive_scopes: 0,
            suppressed_key_transforms: 0,
            suppressed_non_reactive_classification: 0,
            diagnostics: std::vec::Vec::new(),
        }
    }

    pub(crate) fn next_id(&mut self) -> usize {
        let id = self.temp_id;
        self.temp_id += 1;
        id
    }

    pub(crate) fn add_template(&mut self, element_id: usize, template: String) -> usize {
        let template_index = self.templates.len();
        self.templates.push(template);
        self.element_template_map.insert(element_id, template_index);
        template_index
    }

    pub(crate) fn enter_non_reactive_scope(&mut self) {
        self.non_reactive_scopes += 1;
    }

    pub(crate) fn exit_non_reactive_scope(&mut self) {
        self.non_reactive_scopes = self.non_reactive_scopes.saturating_sub(1);
    }

    pub(crate) fn is_non_reactive(&self) -> bool {
        self.non_reactive_scopes > 0
    }

    pub(crate) fn suppress_next_key_transform(&mut self) {
        self.suppressed_key_transforms += 1;
    }

    pub(crate) fn take_suppressed_key_transform(&mut self) -> bool {
        let suppressed = self.suppressed_key_transforms > 0;
        self.suppressed_key_transforms = self.suppressed_key_transforms.saturating_sub(1);
        suppressed
    }

    pub(crate) fn suppress_next_non_reactive_classification(&mut self) {
        self.suppressed_non_reactive_classification += 1;
    }

    pub(crate) fn take_suppressed_non_reactive_classification(&mut self) -> bool {
        let suppressed = self.suppressed_non_reactive_classification > 0;
        self.suppressed_non_reactive_classification = self
            .suppressed_non_reactive_classification
            .saturating_sub(1);
        suppressed
    }

    pub(crate) fn push_dynamic_operation(
        &mut self,
        block: &mut BlockIRNode<'a>,
        operation: OperationNode<'a>,
    ) {
        if self.is_non_reactive() {
            block.operation.push(operation);
            return;
        }

        let mut effect_ops = Vec::new_in(self.allocator);
        effect_ops.push(operation);
        block.effect.push(IREffect {
            operations: effect_ops,
        });
    }

    pub(crate) fn push_diagnostic(&mut self, message: impl Into<String>) {
        self.diagnostics.push(message.into());
    }
}
