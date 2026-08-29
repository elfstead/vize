//! Element transformation dispatch for Vapor IR lowering.

#[path = "element/child_layout.rs"]
mod child_layout;
#[path = "element/component.rs"]
mod component;
#[path = "element/deferred.rs"]
mod deferred;
#[path = "element/key.rs"]
mod key;
#[path = "element/template.rs"]
mod template;

use vize_carton::{Box, String, Vec, append, cstr, ensure_sufficient_stack};

use crate::ir::{
    BlockIRNode, ComponentKind, CreateComponentIRNode, IRProp, IRSlot, OperationNode,
    SetBlockKeyIRNode, SetTemplateRefIRNode, SlotOutletIRNode,
};
use vize_atelier_core::{
    CommentNode, ElementNode, ElementType, ExpressionNode, PropNode, SimpleExpressionNode,
    SourceLocation, TemplateChildNode,
};

use self::{
    child_layout::ChildLayout,
    component::transform_component,
    deferred::{transform_element_runtime_work, transform_element_with_dynamic_children},
    key::{has_dynamic_key, static_key_expression, transform_keyed_element},
    template::generate_element_template_with_layout,
};

use super::{
    context::TransformContext,
    control::{transform_for_node, transform_if_node},
    text::{transform_interpolation, transform_text},
    transform_children,
};

/// Lower an element-like AST node into Vapor IR operations.
pub(crate) fn transform_element<'a>(
    ctx: &mut TransformContext<'a>,
    el: &ElementNode<'a>,
    block: &mut BlockIRNode<'a>,
) {
    let non_reactive = if ctx.take_suppressed_non_reactive_classification() {
        NonReactiveDirective::default()
    } else {
        classify_non_reactive_directive(el)
    };
    if let Some(ref memo_error) = non_reactive.memo_error {
        ctx.push_diagnostic(memo_error.clone());
    }
    let entered_non_reactive = non_reactive.should_lower_as_once;
    if entered_non_reactive {
        ctx.enter_non_reactive_scope();
    }
    let process_current_key = !ctx.take_suppressed_key_transform() && !ctx.is_non_reactive();

    // Template elements don't consume an ID - they just wrap children
    if el.tag_type == ElementType::Template {
        for child in vize_atelier_core::walk_probe::vapor_children(&el.children) {
            match child {
                TemplateChildNode::Element(child_el) => {
                    ensure_sufficient_stack(|| transform_element(ctx, child_el, block));
                }
                TemplateChildNode::Text(text) => {
                    transform_text(ctx, text, block);
                }
                TemplateChildNode::Interpolation(interp) => {
                    transform_interpolation(ctx, interp, block);
                }
                TemplateChildNode::If(if_node) => {
                    transform_if_node(ctx, if_node, block);
                }
                TemplateChildNode::For(for_node) => {
                    transform_for_node(ctx, for_node, block);
                }
                _ => {}
            }
        }
        if entered_non_reactive {
            ctx.exit_non_reactive_scope();
        }
        return;
    }

    // Components handle their own ID allocation (slots consume IDs before the component).
    // The parser classifies `<component :is>` as an element, so dispatch it
    // before native child-layout analysis.
    if process_current_key && has_dynamic_key(el) {
        transform_keyed_element(ctx, el, block, None, None, true);
        if entered_non_reactive {
            ctx.exit_non_reactive_scope();
        }
        return;
    }

    if el.tag_type == ElementType::Component
        || matches!(el.tag, "component" | "Component")
        || is_plain_element(el)
    {
        let element_id = transform_component(ctx, el, block, None, None, true);
        emit_static_key(ctx, el, element_id, block);
        if entered_non_reactive {
            ctx.exit_non_reactive_scope();
        }
        return;
    }

    // Check if this element has non-static children that require
    // deferred ID allocation (so inner templates/IDs come first).
    let child_layout = (el.tag_type == ElementType::Element)
        .then(|| ChildLayout::new(&el.children, !ctx.is_non_reactive()));
    let has_dynamic_children = child_layout
        .as_ref()
        .is_some_and(ChildLayout::has_dynamic_children);

    if has_dynamic_children {
        let element_id = transform_element_with_dynamic_children(
            ctx,
            el,
            child_layout
                .as_ref()
                .expect("native elements have a child layout"),
            block,
            None,
            true,
        );
        emit_static_key(ctx, el, element_id, block);
        if entered_non_reactive {
            ctx.exit_non_reactive_scope();
        }
        return;
    }

    let element_id = ctx.next_id();

    match el.tag_type {
        ElementType::Element => {
            let layout = child_layout
                .as_ref()
                .expect("native elements have a child layout");
            let template = generate_element_template_with_layout(el, layout);
            transform_element_runtime_work(ctx, el, layout, element_id, block);

            // Register template (no deferred children to process)
            ctx.add_template(element_id, template);
        }
        ElementType::Component => {
            let mut props = Vec::new_in(&ctx.allocator);
            let slots = Vec::new_in(&ctx.allocator);

            // Process props (v-bind and v-on directives, and static attributes)
            for prop in el.props.iter() {
                match prop {
                    PropNode::Directive(dir) => {
                        if dir.name == "bind" {
                            // v-bind -> prop, v-bind="obj" -> ordered spread source
                            if let Some(ref arg) = dir.arg {
                                if let ExpressionNode::Simple(key_exp) = arg {
                                    let key_node = SimpleExpressionNode::from_node(key_exp);
                                    let key = Box::new_in(key_node, &ctx.allocator);

                                    let mut values = Vec::new_in(&ctx.allocator);
                                    if let Some(ref exp) = dir.exp
                                        && let ExpressionNode::Simple(val_exp) = exp
                                    {
                                        let val_node = SimpleExpressionNode::from_node(val_exp);
                                        values.push(Box::new_in(val_node, &ctx.allocator));
                                    }

                                    props.push(IRProp {
                                        key,
                                        values,
                                        is_component: true,
                                    });
                                }
                            } else if let Some(ref exp) = dir.exp
                                && let ExpressionNode::Simple(val_exp) = exp
                            {
                                let key_node =
                                    SimpleExpressionNode::new("$", true, SourceLocation::STUB);
                                let key = Box::new_in(key_node, &ctx.allocator);
                                let mut values = Vec::new_in(&ctx.allocator);
                                let val_node = SimpleExpressionNode::from_node(val_exp);
                                values.push(Box::new_in(val_node, &ctx.allocator));

                                props.push(IRProp {
                                    key,
                                    values,
                                    is_component: true,
                                });
                            }
                        } else if dir.name == "on" {
                            // v-on -> onXxx prop
                            if let Some(ref arg) = dir.arg
                                && let ExpressionNode::Simple(event_exp) = arg
                            {
                                let event_name = event_exp.content;
                                let on_name = if event_name.is_empty() {
                                    String::from("on")
                                } else {
                                    let mut s = String::from("on");
                                    let mut chars = event_name.chars();
                                    if let Some(c) = chars.next() {
                                        s.push(c.to_ascii_uppercase());
                                    }
                                    for c in chars {
                                        s.push(c);
                                    }
                                    s
                                };

                                let on_name = ctx.allocator.alloc_str(&on_name);
                                let key_node =
                                    SimpleExpressionNode::new(on_name, true, event_exp.loc.clone());
                                let key = Box::new_in(key_node, &ctx.allocator);

                                let mut values = Vec::new_in(&ctx.allocator);
                                if let Some(ref exp) = dir.exp
                                    && let ExpressionNode::Simple(val_exp) = exp
                                {
                                    let val_node = SimpleExpressionNode::from_node(val_exp);
                                    values.push(Box::new_in(val_node, &ctx.allocator));
                                }

                                props.push(IRProp {
                                    key,
                                    values,
                                    is_component: true,
                                });
                            }
                        } else if dir.name == "model" {
                            // v-model -> modelValue + onUpdate:modelValue props
                            let binding = if let Some(ref exp) = dir.exp {
                                match exp {
                                    ExpressionNode::Simple(s) => s.content.into(),
                                    _ => String::from(""),
                                }
                            } else {
                                String::from("")
                            };

                            // Determine prop name from argument (default: "modelValue")
                            let prop_name = dir
                                .arg
                                .as_ref()
                                .map(|arg| match arg {
                                    ExpressionNode::Simple(s) => s.content.into(),
                                    _ => String::from("modelValue"),
                                })
                                .unwrap_or_else(|| String::from("modelValue"));

                            // Add modelValue prop
                            let prop_name_ref = ctx.allocator.alloc_str(&prop_name);
                            let key_node = SimpleExpressionNode::new(
                                prop_name_ref,
                                true,
                                SourceLocation::STUB,
                            );
                            let key = Box::new_in(key_node, &ctx.allocator);
                            let mut values = Vec::new_in(&ctx.allocator);
                            let binding_ref = ctx.allocator.alloc_str(&binding);
                            let val_node =
                                SimpleExpressionNode::new(binding_ref, false, SourceLocation::STUB);
                            values.push(Box::new_in(val_node, &ctx.allocator));
                            props.push(IRProp {
                                key,
                                values,
                                is_component: true,
                            });

                            // Add onUpdate:propName event prop
                            let event_key = {
                                let mut s = String::from("onUpdate:");
                                s.push_str(prop_name.as_str());
                                s
                            };
                            let event_key = ctx.allocator.alloc_str(&event_key);
                            let event_key_node =
                                SimpleExpressionNode::new(event_key, true, SourceLocation::STUB);
                            let event_key_box = Box::new_in(event_key_node, &ctx.allocator);
                            // Handler getter: the Vapor runtime resolves raw
                            // component props lazily before emit invokes it.
                            let handler_content = {
                                let mut s = String::from("__RAW__() => _value => (_ctx.");
                                s.push_str(binding.as_str());
                                s.push_str(" = _value)");
                                s
                            };
                            let handler_content = ctx.allocator.alloc_str(&handler_content);
                            let handler_node = SimpleExpressionNode::new(
                                handler_content,
                                true,
                                SourceLocation::STUB,
                            );
                            let mut handler_values = Vec::new_in(&ctx.allocator);
                            handler_values.push(Box::new_in(handler_node, &ctx.allocator));
                            props.push(IRProp {
                                key: event_key_box,
                                values: handler_values,
                                is_component: true,
                            });

                            // Add modifiers prop if present
                            if !dir.modifiers.is_empty() {
                                let mod_key_name = if prop_name == "modelValue" {
                                    String::from("modelModifiers")
                                } else {
                                    let mut s = prop_name.clone();
                                    s.push_str("Modifiers");
                                    s
                                };
                                let mod_key_name = ctx.allocator.alloc_str(&mod_key_name);
                                let mod_key_node = SimpleExpressionNode::new(
                                    mod_key_name,
                                    true,
                                    SourceLocation::STUB,
                                );
                                let mod_key = Box::new_in(mod_key_node, &ctx.allocator);
                                // Build modifiers object content
                                let mut mod_content = String::from("__RAW__() => ({ ");
                                for (i, m) in dir.modifiers.iter().enumerate() {
                                    if i > 0 {
                                        mod_content.push_str(", ");
                                    }
                                    mod_content.push_str(m.content);
                                    mod_content.push_str(": true");
                                }
                                mod_content.push_str(" })");
                                let mod_content = ctx.allocator.alloc_str(&mod_content);
                                let mod_val_node = SimpleExpressionNode::new(
                                    mod_content,
                                    true,
                                    SourceLocation::STUB,
                                );
                                let mut mod_values = Vec::new_in(&ctx.allocator);
                                mod_values.push(Box::new_in(mod_val_node, &ctx.allocator));
                                props.push(IRProp {
                                    key: mod_key,
                                    values: mod_values,
                                    is_component: true,
                                });
                            }
                        }
                    }
                    PropNode::Attribute(attr) => {
                        // Static attribute -> prop
                        let key_node =
                            SimpleExpressionNode::new(attr.name, true, SourceLocation::STUB);
                        let key = Box::new_in(key_node, &ctx.allocator);

                        let mut values = Vec::new_in(&ctx.allocator);
                        if let Some(ref value) = attr.value {
                            let val_node = SimpleExpressionNode::new(
                                value.content,
                                true,
                                SourceLocation::STUB,
                            );
                            values.push(Box::new_in(val_node, &ctx.allocator));
                        }

                        props.push(IRProp {
                            key,
                            values,
                            is_component: true,
                        });
                    }
                }
            }

            let create_component = CreateComponentIRNode {
                id: element_id,
                tag: el.tag,
                props,
                slots,
                asset: true,
                once: false,
                dynamic_slots: false,
                kind: crate::ir::ComponentKind::Regular,
                is_expr: None,
                v_show: None,
                insertion: None,
            };

            block
                .operation
                .push(OperationNode::CreateComponent(create_component));
        }
        ElementType::Slot => {
            let name = get_slot_outlet_name(ctx, el);
            let props = get_slot_outlet_props(ctx, el);
            let fallback = (!el.children.is_empty()).then(|| transform_children(ctx, &el.children));
            let slot_outlet = SlotOutletIRNode {
                id: element_id,
                name,
                props,
                fallback,
                insertion: None,
            };

            block.operation.push(OperationNode::SlotOutlet(slot_outlet));
        }
        ElementType::Template => {
            // Panic path by transform invariant: `transform_element` peels
            // template nodes at function entry because they do not create DOM
            // elements. Reaching this arm means that guard was bypassed in a
            // future edit, so continuing would emit an invalid Vapor operation.
            unreachable!("Template elements handled at top of transform_element");
        }
    }

    emit_static_key(ctx, el, element_id, block);
    block.returns.push(element_id);

    if entered_non_reactive {
        ctx.exit_non_reactive_scope();
    }
}

pub(super) fn transform_existing_element<'a>(
    ctx: &mut TransformContext<'a>,
    el: &ElementNode<'a>,
    element_id: usize,
    insertion: Option<crate::ir::InsertionState>,
    block: &mut BlockIRNode<'a>,
) {
    let non_reactive = if ctx.take_suppressed_non_reactive_classification() {
        NonReactiveDirective::default()
    } else {
        classify_non_reactive_directive(el)
    };
    if let Some(ref memo_error) = non_reactive.memo_error {
        ctx.push_diagnostic(memo_error.clone());
    }
    let entered_non_reactive = non_reactive.should_lower_as_once;
    if entered_non_reactive {
        ctx.enter_non_reactive_scope();
    }
    let process_current_key = !ctx.take_suppressed_key_transform() && !ctx.is_non_reactive();

    if process_current_key && has_dynamic_key(el) {
        transform_keyed_element(ctx, el, block, Some(element_id), insertion, false);
    } else if el.tag_type == ElementType::Component
        || matches!(el.tag, "component" | "Component")
        || is_plain_element(el)
    {
        transform_component(ctx, el, block, Some(element_id), insertion, false);
        emit_static_key(ctx, el, element_id, block);
    } else if el.tag_type == ElementType::Slot {
        let name = get_slot_outlet_name(ctx, el);
        let props = get_slot_outlet_props(ctx, el);
        let fallback = (!el.children.is_empty()).then(|| transform_children(ctx, &el.children));
        block
            .operation
            .push(OperationNode::SlotOutlet(SlotOutletIRNode {
                id: element_id,
                name,
                props,
                fallback,
                insertion,
            }));
        emit_static_key(ctx, el, element_id, block);
    } else if el.tag_type == ElementType::Element {
        let layout = ChildLayout::new(&el.children, !ctx.is_non_reactive());
        if layout.has_dynamic_children() {
            transform_element_with_dynamic_children(
                ctx,
                el,
                &layout,
                block,
                Some(element_id),
                false,
            );
        } else {
            transform_element_runtime_work(ctx, el, &layout, element_id, block);
        }
        emit_static_key(ctx, el, element_id, block);
    } else if el.tag_type == ElementType::Template {
        for child in el.children.iter() {
            if let TemplateChildNode::Element(child) = child {
                transform_element(ctx, child, block);
            }
        }
    }

    if entered_non_reactive {
        ctx.exit_non_reactive_scope();
    }
}

pub(super) fn transform_element_without_key<'a>(
    ctx: &mut TransformContext<'a>,
    el: &ElementNode<'a>,
    block: &mut BlockIRNode<'a>,
    existing_id: Option<usize>,
    insertion: Option<crate::ir::InsertionState>,
    add_return: bool,
) {
    ctx.suppress_next_key_transform();
    ctx.suppress_next_non_reactive_classification();
    if let Some(element_id) = existing_id {
        transform_existing_element(ctx, el, element_id, insertion, block);
    } else {
        transform_element(ctx, el, block);
        if !add_return {
            block.returns.pop();
        }
    }
}

pub(super) fn is_plain_element(el: &ElementNode<'_>) -> bool {
    el.tag_type == ElementType::Element && el.tag == "template"
}

pub(crate) fn transform_comment<'a>(
    ctx: &mut TransformContext<'a>,
    comment: &CommentNode,
    block: &mut BlockIRNode<'a>,
) {
    let element_id = ctx.next_id();
    let mut template = cstr!("<!--");
    template.push_str(&self::template::escape_html_text(&comment.content));
    template.push_str("-->");
    ctx.add_template(element_id, template);
    block.returns.push(element_id);
}

fn emit_static_key<'a>(
    ctx: &TransformContext<'a>,
    el: &ElementNode<'a>,
    element_id: usize,
    block: &mut BlockIRNode<'a>,
) {
    let Some(value) = static_key_expression(ctx, el) else {
        return;
    };
    block
        .operation
        .push(OperationNode::SetBlockKey(SetBlockKeyIRNode {
            element: element_id,
            value,
        }));
}

#[derive(Default)]
struct NonReactiveDirective {
    should_lower_as_once: bool,
    memo_error: Option<String>,
}

fn classify_non_reactive_directive(el: &ElementNode<'_>) -> NonReactiveDirective {
    let has_once = el
        .props
        .iter()
        .any(|prop| matches!(prop, PropNode::Directive(dir) if dir.name == "once"));
    if has_once {
        return NonReactiveDirective {
            should_lower_as_once: true,
            memo_error: None,
        };
    }

    for prop in el.props.iter() {
        let PropNode::Directive(dir) = prop else {
            continue;
        };
        if dir.name != "memo" {
            continue;
        }

        let Some(ExpressionNode::Simple(exp)) = dir.exp.as_ref() else {
            return NonReactiveDirective {
                should_lower_as_once: false,
                memo_error: Some(vize_carton::String::from(
                    "v-memo is not supported in Vapor yet. Use v-once or v-memo=\"[]\" until memo guards are implemented.",
                )),
            };
        };

        if exp.content.trim() == "[]" {
            return NonReactiveDirective {
                should_lower_as_once: true,
                memo_error: None,
            };
        }

        return NonReactiveDirective {
            should_lower_as_once: false,
            memo_error: Some(vize_carton::String::from(
                "v-memo with dependencies is not supported in Vapor yet. Use v-once or v-memo=\"[]\" until memo guards are implemented.",
            )),
        };
    }

    NonReactiveDirective {
        should_lower_as_once: false,
        memo_error: None,
    }
}

fn get_slot_outlet_name<'a>(
    ctx: &TransformContext<'a>,
    el: &ElementNode<'a>,
) -> Box<'a, SimpleExpressionNode<'a>> {
    for prop in el.props.iter() {
        match prop {
            PropNode::Attribute(attr) => {
                if attr.name == "name"
                    && let Some(ref value) = attr.value
                {
                    return Box::new_in(
                        SimpleExpressionNode::new(value.content, true, SourceLocation::STUB),
                        &ctx.allocator,
                    );
                }
            }
            PropNode::Directive(dir) => {
                if dir.name == "bind"
                    && let Some(ExpressionNode::Simple(arg)) = dir.arg.as_ref()
                    && arg.content == "name"
                    && let Some(ExpressionNode::Simple(exp)) = dir.exp.as_ref()
                {
                    return Box::new_in(SimpleExpressionNode::from_node(exp), &ctx.allocator);
                }
            }
        }
    }

    Box::new_in(
        SimpleExpressionNode::new("default", true, SourceLocation::STUB),
        &ctx.allocator,
    )
}

fn get_slot_outlet_props<'a>(
    ctx: &TransformContext<'a>,
    el: &ElementNode<'a>,
) -> Vec<'a, IRProp<'a>> {
    let mut props = Vec::new_in(&ctx.allocator);

    for prop in el.props.iter() {
        match prop {
            PropNode::Attribute(attr) => {
                if attr.name == "name" {
                    continue;
                }

                let key = Box::new_in(
                    SimpleExpressionNode::new(attr.name, true, SourceLocation::STUB),
                    &ctx.allocator,
                );
                let mut values = Vec::new_in(&ctx.allocator);
                if let Some(ref value) = attr.value {
                    values.push(Box::new_in(
                        SimpleExpressionNode::new(value.content, true, SourceLocation::STUB),
                        &ctx.allocator,
                    ));
                }

                props.push(IRProp {
                    key,
                    values,
                    is_component: false,
                });
            }
            PropNode::Directive(dir) => {
                if dir.name != "bind" {
                    continue;
                }

                match (dir.arg.as_ref(), dir.exp.as_ref()) {
                    (Some(ExpressionNode::Simple(arg)), Some(ExpressionNode::Simple(exp))) => {
                        if arg.content == "name" {
                            continue;
                        }

                        let key = Box::new_in(SimpleExpressionNode::from_node(arg), &ctx.allocator);
                        let mut values = Vec::new_in(&ctx.allocator);
                        values.push(Box::new_in(
                            SimpleExpressionNode::from_node(exp),
                            &ctx.allocator,
                        ));

                        props.push(IRProp {
                            key,
                            values,
                            is_component: false,
                        });
                    }
                    (None, Some(ExpressionNode::Simple(exp))) => {
                        let key = Box::new_in(
                            SimpleExpressionNode::new("$", true, SourceLocation::STUB),
                            &ctx.allocator,
                        );
                        let mut values = Vec::new_in(&ctx.allocator);
                        values.push(Box::new_in(
                            SimpleExpressionNode::from_node(exp),
                            &ctx.allocator,
                        ));

                        props.push(IRProp {
                            key,
                            values,
                            is_component: false,
                        });
                    }
                    _ => {}
                }
            }
        }
    }

    props
}
