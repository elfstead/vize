use crate::ir::{InsertionAnchor, InsertionState};

use super::super::context::GenerateContext;

pub(super) fn emit_insertion_state(ctx: &mut GenerateContext, insertion: Option<InsertionState>) {
    let Some(insertion) = insertion else {
        return;
    };

    ctx.use_helper("setInsertionState");
    match insertion.anchor {
        InsertionAnchor::Prepend if insertion.logical_index == 0 => {
            ctx.push_line_fmt(format_args!("_setInsertionState(n{}, 0)", insertion.parent));
        }
        InsertionAnchor::Prepend => {
            ctx.push_line_fmt(format_args!(
                "_setInsertionState(n{}, 0, {})",
                insertion.parent, insertion.logical_index
            ));
        }
        InsertionAnchor::Before(anchor) => {
            ctx.push_line_fmt(format_args!(
                "_setInsertionState(n{}, n{}, {})",
                insertion.parent, anchor, insertion.logical_index
            ));
        }
        InsertionAnchor::Append => {
            ctx.push_line_fmt(format_args!(
                "_setInsertionState(n{}, null, {})",
                insertion.parent, insertion.logical_index
            ));
        }
    }
}
