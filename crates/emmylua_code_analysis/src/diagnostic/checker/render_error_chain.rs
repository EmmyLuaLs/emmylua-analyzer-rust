use crate::{
    ChainMessage, DbIndex, ErrorChain, LuaType, MissingMembersMessage, RenderLevel, humanize_type,
};

/// 错误链是否可以独立表达完整的失败
pub fn chain_stands_alone(db: &DbIndex, chain: Option<&ErrorChain>, source: &LuaType) -> bool {
    let Some(head) = chain else {
        return false;
    };
    match head.message() {
        ChainMessage::NotAssignable {
            source: head_source,
            ..
        } => *head_source == humanize_type(db, source, RenderLevel::Simple),
        _ => false,
    }
}

/// 按错误链渲染诊断详情
pub fn render_error_chain(chain: Option<&ErrorChain>, indent_first_line: bool) -> Option<String> {
    let mut current = chain;
    let mut output = String::new();
    let mut depth = if indent_first_line { 1 } else { 0 };

    while let Some(node) = current {
        match node.message() {
            // 数组元素定位标记, 只承担路径语义, 不产生渲染行
            ChainMessage::ArrayElement => {}
            message => {
                start_line(&mut output, depth);
                output.push_str(&render_message(message));
                depth += 1;
            }
        }
        current = node.next();
    }

    (!output.is_empty()).then_some(output)
}

fn render_message(message: &ChainMessage) -> String {
    match message {
        ChainMessage::NotAssignable { source, target } => t!(
            "Type `%{source}` is not assignable to type `%{target}`.",
            source = source,
            target = target,
        )
        .to_string(),
        ChainMessage::Field { name } => t!(
            "The types of field `%{name}` are incompatible.",
            name = name
        )
        .to_string(),
        ChainMessage::Index { index } => {
            t!("Index type `%{index}` is incompatible.", index = index).to_string()
        }
        ChainMessage::TupleElement { index } => t!(
            "Type at position %{index} in source is not compatible with type at position %{index} in target.",
            index = index + 1
        )
        .to_string(),
        ChainMessage::ArrayElement => String::new(),
        ChainMessage::FunctionParameter { index } => {
            t!("Function parameter %{index} is incompatible.", index = index + 1).to_string()
        }
        ChainMessage::FunctionReturn { index } => {
            t!("Function return %{index} is incompatible.", index = index + 1).to_string()
        }
        ChainMessage::GenericArgument { index } => {
            t!("Generic argument %{index} is incompatible.", index = index + 1).to_string()
        }
        ChainMessage::MissingMembers(missing) => render_missing_members(missing),
        ChainMessage::MissingTupleElement { index } => {
            t!("Tuple element %{index} is missing.", index = index + 1).to_string()
        }
        ChainMessage::Text(text) => text.clone(),
    }
}

fn render_missing_members(missing: &MissingMembersMessage) -> String {
    match missing {
        MissingMembersMessage::Single {
            source,
            field,
            target,
        } => t!(
            "Type `%{source}` is missing the `%{field}` field from type `%{target}`.",
            source = source,
            field = field,
            target = target,
        )
        .to_string(),
        MissingMembersMessage::List {
            source,
            target,
            fields,
        } => t!(
            "Type `%{source}` is missing the following fields from type `%{target}`: %{fields}",
            source = source,
            target = target,
            fields = fields,
        )
        .to_string(),
        MissingMembersMessage::Truncated {
            source,
            target,
            fields,
            hidden,
        } => t!(
            "Type `%{source}` is missing the following fields from type `%{target}`: %{fields}, and %{count} more.",
            source = source,
            target = target,
            fields = fields,
            count = hidden,
        )
        .to_string(),
    }
}

fn start_line(output: &mut String, depth: usize) {
    if !output.is_empty() {
        output.push('\n');
    }
    for _ in 0..depth {
        output.push_str("  ");
    }
}
