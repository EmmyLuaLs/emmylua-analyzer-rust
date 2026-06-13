use std::{collections::HashSet, fmt::Write};

use emmylua_code_analysis::{
    AsyncState, DbIndex, LuaDocReturnInfo, LuaDocReturnOverloadInfo, LuaFunctionType, LuaMember,
    LuaMemberOwner, LuaSemanticDeclId, LuaType, RenderLevel, VariadicType, humanize_type,
};

use crate::handlers::hover::{
    HoverBuilder,
    humanize_types::{
        extract_owner_name_from_element, extract_parent_type_from_element, hover_humanize_type,
    },
    infer_prefix_global_name,
};

/// 函数签名渲染上下文，封装 `hover_doc_function_type` 所需的全部参数
pub(super) struct FunctionRenderContext<'a> {
    pub func: &'a LuaFunctionType,
    pub semantic_decl: &'a LuaSemanticDeclId,
    pub owner_member: Option<&'a LuaMember>,
    pub return_docs: Vec<LuaDocReturnInfo>,
    pub ret_detail: Option<String>,
}

/// 根据函数类型分派渲染
pub(super) fn process_function_type(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    typ: &LuaType,
    semantic_decl: &LuaSemanticDeclId,
    function_member: Option<&LuaMember>,
) -> Option<Vec<String>> {
    match typ {
        LuaType::DocFunction(lua_func) => {
            let ctx = FunctionRenderContext {
                func: lua_func,
                semantic_decl,
                owner_member: function_member,
                return_docs: convert_function_return_to_docs(lua_func),
                ret_detail: None,
            };
            let content = render_function(builder, db, ctx)?;
            Some(vec![content])
        }
        LuaType::Signature(signature_id) => {
            let signature = db.get_signature_index().get(&signature_id)?;
            let fake_doc_function = signature.to_doc_func_type();
            let mut contents = Vec::with_capacity(signature.overloads.len() + 1);
            for (i, overload) in std::iter::once(fake_doc_function.as_ref())
                .chain(signature.overloads.iter().map(|overload| overload.as_ref()))
                .enumerate()
            {
                // 提前计算 return_docs 和 ret_detail 的差异, 免重复的 hover_doc_function_type 调用
                let (return_docs, ret_detail) = if i == 0 && !signature.return_overloads.is_empty()
                {
                    let detail =
                        build_function_return_overload_rows(builder, &signature.return_overloads);
                    (Vec::new(), Some(detail))
                } else {
                    let docs = if i == 0 {
                        if signature.return_docs.is_empty() {
                            convert_function_return_to_docs(overload)
                        } else {
                            signature.return_docs.clone()
                        }
                    } else {
                        convert_function_return_to_docs(overload)
                    };
                    (docs, None)
                };

                let ctx = FunctionRenderContext {
                    func: overload,
                    semantic_decl,
                    owner_member: function_member,
                    return_docs,
                    ret_detail,
                };
                contents.push(render_function(builder, db, ctx)?);
            }
            Some(contents)
        }
        LuaType::Union(union) => {
            let mut contents = Vec::new();
            for typ in union.into_vec() {
                if let Some(content) =
                    process_function_type(builder, db, &typ, semantic_decl, function_member)
                {
                    contents.extend(content);
                }
            }
            Some(contents)
        }
        _ => None,
    }
}

/// 渲染单个函数签名的完整 hover 文本
pub(super) fn render_function(
    builder: &mut HoverBuilder,
    db: &DbIndex,
    ctx: FunctionRenderContext,
) -> Option<String> {
    let FunctionRenderContext {
        func,
        semantic_decl,
        owner_member,
        return_docs,
        ret_detail,
    } = ctx;

    let async_label = match func.get_async_state() {
        AsyncState::Async => "async ",
        AsyncState::Sync => "sync ",
        _ => "",
    };
    let mut is_method = func.is_colon_define();
    let mut type_label = if owner_member.is_none() && semantic_decl_is_local(db, semantic_decl) {
        "local function "
    } else {
        "function "
    };

    // 有可能来源于类. 例如: `local add = class.add`, `add()`应被视为类方法
    let full_name = if let Some(owner_member) = owner_member {
        if semantic_decl_is_field(db, semantic_decl, owner_member) {
            type_label = "(field) ";
        }

        let member_key = owner_member.get_key().to_path();
        let mut name = String::with_capacity(member_key.len() + 16);

        let mut push_typed_owner_prefix = |prefix: &str, type_decl_id| {
            name.push_str(prefix);
            let owner_ty = LuaType::Ref(type_decl_id);
            is_method = func.is_method(builder.semantic_model, Some(&owner_ty));
            if is_method {
                type_label = "(method) ";
            }
            name.push(if is_method { ':' } else { '.' });
        };

        let parent_owner = db
            .get_member_index()
            .get_current_owner(&owner_member.get_id());
        if let Some(parent_owner) = parent_owner {
            match parent_owner {
                LuaMemberOwner::Type(type_decl_id) => {
                    let prefix = infer_prefix_global_name(builder.semantic_model, owner_member)
                        .unwrap_or_else(|| type_decl_id.get_simple_name());
                    push_typed_owner_prefix(prefix, type_decl_id.clone());
                }
                LuaMemberOwner::Element(element_id) => {
                    if let Some(LuaType::Ref(type_decl_id) | LuaType::Def(type_decl_id)) =
                        extract_parent_type_from_element(builder.semantic_model, element_id)
                    {
                        push_typed_owner_prefix(
                            type_decl_id.get_simple_name(),
                            type_decl_id.clone(),
                        );
                    } else if let Some(owner_name) =
                        extract_owner_name_from_element(builder.semantic_model, element_id)
                    {
                        name.push_str(&owner_name);
                        if is_method {
                            type_label = "(method) ";
                        }
                        name.push(if is_method { ':' } else { '.' });
                    }
                }
                _ => {}
            }
        }

        name.push_str(&member_key);
        name
    } else {
        semantic_decl_function_name(db, semantic_decl)?
    };

    let is_vararg = func.is_variadic();
    let last_idx = func.get_params().len().saturating_sub(1);

    let params = func
        .get_params()
        .iter()
        .enumerate()
        .map(|(index, param)| {
            let mut name = param.0.clone();
            if is_vararg && index == last_idx && name != "..." {
                name = format!("...{}", name);
            }
            if index == 0 && is_method && !func.is_colon_define() {
                "".to_string()
            } else if let Some(ty) = &param.1 {
                format!("{}: {}", name, humanize_type(db, ty, RenderLevel::Simple))
            } else {
                name.to_string()
            }
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();

    let ret_detail = ret_detail.unwrap_or_else(|| build_function_returns(builder, return_docs));
    Some(format_function_type(
        type_label,
        async_label,
        full_name,
        params.join(", "),
        ret_detail,
    ))
}

fn semantic_decl_is_field(
    db: &DbIndex,
    semantic_decl: &LuaSemanticDeclId,
    owner_member: &LuaMember,
) -> bool {
    if let LuaSemanticDeclId::Member(member_id) = semantic_decl {
        if db
            .get_member_index()
            .get_member(member_id)
            .is_some_and(|member| member.is_field())
        {
            return true;
        }
    }

    let member_index = db.get_member_index();
    let Some(owner) = member_index.get_current_owner(&owner_member.get_id()) else {
        return false;
    };
    member_index.get_members(owner).is_some_and(|members| {
        members
            .iter()
            .any(|member| member.get_key() == owner_member.get_key() && member.is_field())
    })
}

fn semantic_decl_is_local(db: &DbIndex, semantic_decl: &LuaSemanticDeclId) -> bool {
    match semantic_decl {
        LuaSemanticDeclId::LuaDecl(decl_id) => db
            .get_decl_index()
            .get_decl(decl_id)
            .is_some_and(|decl| decl.is_local()),
        _ => false,
    }
}

fn semantic_decl_function_name(db: &DbIndex, semantic_decl: &LuaSemanticDeclId) -> Option<String> {
    match semantic_decl {
        LuaSemanticDeclId::LuaDecl(decl_id) => Some(
            db.get_decl_index()
                .get_decl(decl_id)?
                .get_name()
                .to_string(),
        ),
        LuaSemanticDeclId::Member(member_id) => Some(
            db.get_member_index()
                .get_member(member_id)?
                .get_key()
                .to_path(),
        ),
        _ => None,
    }
}

fn format_function_type(
    type_label: &str,
    async_label: &str,
    full_name: String,
    params: String,
    rets: String,
) -> String {
    let prefix = if type_label.starts_with("function") {
        format!("{}{}", async_label, type_label)
    } else {
        format!("{}{}", type_label, async_label)
    };
    format!("{}{}({}){}", prefix, full_name, params, rets)
}

pub(super) fn convert_function_return_to_docs(func: &LuaFunctionType) -> Vec<LuaDocReturnInfo> {
    match func.get_ret() {
        LuaType::Variadic(variadic) => match variadic.as_ref() {
            VariadicType::Base(base) => vec![LuaDocReturnInfo {
                name: None,
                type_ref: base.clone(),
                description: None,
                attributes: None,
            }],
            VariadicType::Multi(types) => types
                .iter()
                .map(|ty| LuaDocReturnInfo {
                    name: None,
                    type_ref: ty.clone(),
                    description: None,
                    attributes: None,
                })
                .collect(),
        },
        _ => vec![LuaDocReturnInfo {
            name: None,
            type_ref: func.get_ret().clone(),
            description: None,
            attributes: None,
        }],
    }
}

fn build_function_returns(
    builder: &mut HoverBuilder,
    return_docs: Vec<LuaDocReturnInfo>,
) -> String {
    let mut result = String::new();
    // 如果不是补全且存在名称, 我们需要多行显示
    let has_multiline = !builder.is_completion
        && return_docs
            .iter()
            .any(|return_info| return_info.name.is_some());

    for (i, return_info) in return_docs.iter().enumerate() {
        if i == 0 && return_info.type_ref.is_nil() {
            continue;
        }
        let type_text = build_return_type_text(builder, &return_info.type_ref, i);

        if has_multiline {
            if i == 0 {
                result.push('\n');
                result.push_str("  -> ");
            } else {
                let _ = write!(result, "  {}. ", i + 1);
            }
            if let Some(name) = return_info.name.as_deref().filter(|name| !name.is_empty()) {
                let _ = write!(result, "{}: ", name);
            }
            result.push_str(&type_text);
            result.push('\n');
        } else if i == 0 {
            result.push_str(" -> ");
            result.push_str(&type_text);
        } else {
            result.push_str(", ");
            result.push_str(&type_text);
        }
    }

    result
}

pub(super) fn build_function_return_overload_rows(
    builder: &mut HoverBuilder,
    return_overloads: &[LuaDocReturnOverloadInfo],
) -> String {
    let mut result = String::new();

    for (row_idx, row) in return_overloads.iter().enumerate() {
        if row.type_refs.is_empty() {
            continue;
        }

        if row_idx == 0 {
            result.push('\n');
        }
        result.push_str("  -> ");
        for (i, typ) in row.type_refs.iter().enumerate() {
            if i > 0 {
                result.push_str(", ");
            }
            result.push_str(&build_return_type_text(builder, typ, i));
        }
        result.push('\n');
    }

    result
}

fn build_return_type_text(builder: &mut HoverBuilder, typ: &LuaType, i: usize) -> String {
    let type_expansion_count = builder.get_type_expansion_count();
    // 在这个过程中可能会设置`type_expansion`
    let type_text = hover_humanize_type(builder, typ, Some(RenderLevel::Simple));
    if builder.get_type_expansion_count() > type_expansion_count {
        // 重新设置`type_expansion`
        if let Some(pop_type_expansion) =
            builder.pop_type_expansion(type_expansion_count, builder.get_type_expansion_count())
        {
            let mut new_type_expansion = format!("return #{}", i + 1);
            let mut seen = HashSet::new();
            for type_expansion in pop_type_expansion {
                for line in type_expansion.lines().skip(1) {
                    if seen.insert(line.to_string()) {
                        new_type_expansion.push('\n');
                        new_type_expansion.push_str(line);
                    }
                }
            }
            builder.add_type_expansion(new_type_expansion);
        }
    };
    type_text
}
