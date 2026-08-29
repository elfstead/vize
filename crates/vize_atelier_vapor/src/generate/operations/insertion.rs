use crate::ir::{InsertionAnchor, InsertionState};

use super::super::context::GenerateContext;

pub(super) fn emit_insertion_state(ctx: &mut GenerateContext, insertion: Option<InsertionState>) {
    let Some(insertion) = insertion else {
        return;
    };

    ctx.use_helper("setInsertionState");
    match insertion.anchor {
        InsertionAnchor::Before(anchor) => {
            ctx.push_line_fmt(format_args!(
                "_setInsertionState(n{}, n{})",
                insertion.parent, anchor
            ));
        }
        InsertionAnchor::Append if insertion.logical_index == 0 => {
            ctx.push_line_fmt(format_args!("_setInsertionState(n{})", insertion.parent));
        }
        InsertionAnchor::Append => {
            ctx.push_line_fmt(format_args!(
                "_setInsertionState(n{}, {})",
                insertion.parent, insertion.logical_index
            ));
        }
    }
}
