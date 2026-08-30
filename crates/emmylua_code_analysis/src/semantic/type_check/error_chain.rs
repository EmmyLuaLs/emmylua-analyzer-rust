use std::rc::Rc;

use crate::{DbIndex, LuaMemberKey, LuaType, RenderLevel, humanize_type};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowKind {
    Recursion,
    Budget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainMessage {
    NotAssignable { source: String, target: String },
    Field { name: String },
    Index { index: String },
    TupleElement { index: usize },
    ArrayElement,
    FunctionParameter { index: usize },
    FunctionReturn { index: usize },
    GenericArgument { index: usize },
    MissingMembers(MissingMembersMessage),
    MissingTupleElement { index: usize },
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorChainNode {
    next: Option<ErrorChain>,
    message: ChainMessage,
}

pub type ErrorChain = Rc<ErrorChainNode>;

impl ErrorChainNode {
    pub fn message(&self) -> &ChainMessage {
        &self.message
    }

    pub fn next(&self) -> Option<&ErrorChain> {
        self.next.as_ref()
    }
}

pub fn chain_node(message: ChainMessage, next: Option<ErrorChain>) -> ErrorChain {
    Rc::new(ErrorChainNode { next, message })
}

pub fn push_message(head: Option<ErrorChain>, message: ChainMessage) -> ErrorChain {
    let Some(head_chain) = head else {
        return chain_node(message, None);
    };

    if let ChainMessage::Field { name } = &message {
        let mut scan: Option<&ErrorChainNode> = Some(&head_chain);
        let mut fold_target = None;
        while let Some(node) = scan {
            match node.message() {
                // 跳过数组元素标记
                ChainMessage::ArrayElement => {
                    scan = node.next().map(|next| &**next);
                }
                ChainMessage::Field { name: inner } => {
                    fold_target = Some((inner.as_str(), node.next().cloned()));
                    break;
                }
                _ => break,
            }
        }
        if let Some((inner, tail)) = fold_target {
            return chain_node(
                ChainMessage::Field {
                    name: combine_path(name, inner),
                },
                tail,
            );
        }
        return chain_node(message, Some(head_chain));
    }

    // 抑制规则
    if let ChainMessage::NotAssignable { source, target } = &message {
        let covered = match head_chain.message() {
            ChainMessage::NotAssignable {
                source: head_source,
                target: head_target,
            } => head_source == source && head_target == target,
            ChainMessage::MissingMembers(
                MissingMembersMessage::Single {
                    source: missing_source,
                    target: missing_target,
                    ..
                }
                | MissingMembersMessage::List {
                    source: missing_source,
                    target: missing_target,
                    ..
                }
                | MissingMembersMessage::Truncated {
                    source: missing_source,
                    target: missing_target,
                    ..
                },
            ) => missing_source == source && missing_target == target,
            _ => false,
        };
        if covered {
            return head_chain;
        }
    }

    chain_node(message, Some(head_chain))
}

/// 路径合成:
/// - `a` + `b` => `a.b`
/// - `a` + `[1]` => `a[1]`
fn combine_path(head: &str, tail: &str) -> String {
    if tail.starts_with('[') {
        format!("{head}{tail}")
    } else {
        format!("{head}.{tail}")
    }
}

pub(crate) fn not_assignable_message(
    db: &DbIndex,
    source: &LuaType,
    target: &LuaType,
) -> ChainMessage {
    ChainMessage::NotAssignable {
        source: humanize_type(db, source, RenderLevel::Simple),
        target: humanize_type(db, target, RenderLevel::Simple),
    }
}

pub(crate) fn property_message(key: &LuaMemberKey) -> ChainMessage {
    ChainMessage::Field {
        name: key.to_path(),
    }
}

pub(crate) fn index_message(db: &DbIndex, key_type: &LuaType) -> ChainMessage {
    ChainMessage::Index {
        index: humanize_type(db, key_type, RenderLevel::Simple),
    }
}

pub(crate) fn missing_members_message(
    db: &DbIndex,
    source: &LuaType,
    target: &LuaType,
    keys: &[LuaMemberKey],
) -> ChainMessage {
    let mut names = keys
        .iter()
        .filter_map(member_key_to_field_name)
        .collect::<Vec<_>>();
    names.sort_unstable();
    names.dedup();
    let Some(first) = names.first() else {
        return not_assignable_message(db, source, target);
    };

    if names.len() == 1 {
        return ChainMessage::MissingMembers(MissingMembersMessage::Single {
            source: humanize_type(db, source, RenderLevel::Simple),
            field: first.clone(),
            target: humanize_type(db, target, RenderLevel::Simple),
        });
    }

    const MAX_DISPLAY_FIELDS: usize = 4;

    let source = humanize_type(db, source, RenderLevel::Simple);
    let target = humanize_type(db, target, RenderLevel::Simple);
    if names.len() <= MAX_DISPLAY_FIELDS {
        return ChainMessage::MissingMembers(MissingMembersMessage::List {
            source,
            target,
            fields: names.join(", "),
        });
    }

    ChainMessage::MissingMembers(MissingMembersMessage::Truncated {
        source,
        target,
        fields: names[..MAX_DISPLAY_FIELDS].join(", "),
        hidden: names.len() - MAX_DISPLAY_FIELDS,
    })
}

/// 缺失字段消息
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissingMembersMessage {
    Single {
        source: String,
        field: String,
        target: String,
    },
    List {
        source: String,
        target: String,
        fields: String,
    },
    Truncated {
        source: String,
        target: String,
        fields: String,
        hidden: usize,
    },
}

fn member_key_to_field_name(key: &LuaMemberKey) -> Option<String> {
    match key {
        LuaMemberKey::Name(name) => Some(name.to_string()),
        LuaMemberKey::Integer(index) => Some(format!("[{}]", index)),
        LuaMemberKey::None | LuaMemberKey::TypeKey(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_all(messages: Vec<ChainMessage>) -> Option<ErrorChain> {
        let mut head = None;
        for message in messages {
            head = Some(push_message(head, message));
        }
        head
    }

    fn messages(chain: &ErrorChain) -> Vec<ChainMessage> {
        let mut result = Vec::new();
        let mut current = Some(chain);
        while let Some(node) = current {
            result.push(node.message().clone());
            current = node.next();
        }
        result
    }

    fn field(name: &str) -> ChainMessage {
        ChainMessage::Field {
            name: name.to_string(),
        }
    }

    fn not_assignable(source: &str, target: &str) -> ChainMessage {
        ChainMessage::NotAssignable {
            source: source.to_string(),
            target: target.to_string(),
        }
    }

    #[test]
    fn test_fold_adjacent_properties() {
        let chain = push_all(vec![field("inner"), field("middle")]).unwrap();
        assert_eq!(messages(&chain), vec![field("middle.inner"),]);
    }

    #[test]
    fn test_fold_multi_level_properties() {
        let chain = push_all(vec![
            not_assignable("string", "number"),
            field("inner"),
            field("middle"),
            field("root"),
        ])
        .unwrap();
        assert_eq!(
            messages(&chain),
            vec![
                field("root.middle.inner"),
                not_assignable("string", "number")
            ]
        );
    }

    #[test]
    fn test_whitelist_suppression() {
        let chain = push_all(vec![ChainMessage::MissingMembers(
            MissingMembersMessage::Single {
                source: "a".to_string(),
                field: "x".to_string(),
                target: "b".to_string(),
            },
        )])
        .unwrap();
        let chain = push_message(Some(chain), not_assignable("a", "b"));
        assert_eq!(
            messages(&chain),
            vec![ChainMessage::MissingMembers(
                MissingMembersMessage::Single {
                    source: "a".to_string(),
                    field: "x".to_string(),
                    target: "b".to_string(),
                }
            )]
        );

        // 同一类型对重复压入
        let chain = push_all(vec![not_assignable("string", "number")]).unwrap();
        let chain = push_message(Some(chain), not_assignable("string", "number"));
        assert_eq!(messages(&chain), vec![not_assignable("string", "number")]);

        // 非重复压入
        let chain = push_all(vec![not_assignable("string", "number")]).unwrap();
        let chain = push_message(Some(chain), not_assignable("string?", "number?"));
        assert_eq!(
            messages(&chain),
            vec![
                not_assignable("string?", "number?"),
                not_assignable("string", "number")
            ]
        );
    }

    #[test]
    fn test_fold_across_array_element_marker() {
        // 数组元素标记是折叠透明段: 数组套对象的字段路径跨层折叠为点路径.
        // 压栈顺序遵循 unwind: 叶子 → 内层字段 → 数组层 → 外层字段.
        let chain = push_all(vec![
            not_assignable("string", "number"),
            field("id"),
            ChainMessage::ArrayElement,
        ])
        .unwrap();
        let chain = push_message(Some(chain), field("items"));
        assert_eq!(
            messages(&chain),
            vec![field("items.id"), not_assignable("string", "number")]
        );
    }

    #[test]
    fn test_combine_path_formats() {
        assert_eq!(combine_path("a", "b"), "a.b");
        assert_eq!(combine_path("a", "[1]"), "a[1]");
        assert_eq!(combine_path("a.b", "c"), "a.b.c");
    }
}
