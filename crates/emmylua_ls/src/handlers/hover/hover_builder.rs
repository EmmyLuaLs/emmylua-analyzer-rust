use emmylua_code_analysis::{
    LuaFunctionType, LuaMember, LuaMemberOwner, LuaPropertyOwnerId, LuaType, SemanticModel,
};
use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaIndexExpr, LuaSyntaxKind, LuaSyntaxToken};
use lsp_types::{Hover, HoverContents, MarkedString, MarkupContent};

use crate::handlers::hover::std_hover::{hover_std_description, is_std_by_name};

use super::{
    build_hover::{add_signature_param_description, add_signature_ret_description},
    std_hover::is_std_by_path,
};

#[derive(Debug)]
pub struct HoverBuilder<'a> {
    /// 类型描述, 不包含 overload
    pub type_description: MarkedString,
    /// 类的全路径
    pub location_path: Option<MarkedString>,
    /// 函数重载签名, 第一个是重载签名
    pub signature_overload: Option<Vec<MarkedString>>,
    /// 注释描述, 包含函数参数与返回值描述
    pub annotation_description: Vec<MarkedString>,
    /// 类型展开, 常用于 alias 类型
    pub type_expansion: Option<Vec<String>>,

    pub is_completion: bool,
    trigger_token: Option<LuaSyntaxToken>,
    pub semantic_model: &'a SemanticModel<'a>,
}

impl<'a> HoverBuilder<'a> {
    pub fn new(
        semantic_model: &'a SemanticModel,
        token: Option<LuaSyntaxToken>,
        is_completion: bool,
    ) -> Self {
        Self {
            semantic_model,
            type_description: MarkedString::String("".to_string()),
            location_path: None,
            signature_overload: None,
            annotation_description: Vec::new(),
            is_completion,
            trigger_token: token,
            type_expansion: None,
        }
    }

    pub fn set_type_description(&mut self, type_description: String) {
        self.type_description =
            MarkedString::from_language_code("lua".to_string(), type_description);
    }

    pub fn set_location_path(&mut self, owner_member: Option<&LuaMember>) {
        if let Some(owner_member) = owner_member {
            if let LuaMemberOwner::Type(ty) = &owner_member.get_owner() {
                if ty.get_name() != ty.get_simple_name() {
                    self.location_path = Some(MarkedString::from_markdown(format!(
                        "{}{} `{}`",
                        "&nbsp;&nbsp;",
                        "in class",
                        ty.get_name()
                    )));
                }
            }
        }
    }

    pub fn add_signature_overload(&mut self, signature_overload: String) {
        if self.signature_overload.is_none() {
            self.signature_overload = Some(Vec::new());
        }
        self.signature_overload
            .as_mut()
            .unwrap()
            .push(MarkedString::from_language_code(
                "lua".to_string(),
                signature_overload,
            ));
    }

    pub fn add_type_expansion(&mut self, type_expansion: String) {
        if self.type_expansion.is_none() {
            self.type_expansion = Some(Vec::new());
        }
        self.type_expansion.as_mut().unwrap().push(type_expansion);
    }

    pub fn get_type_expansion_count(&self) -> usize {
        if let Some(type_expansion) = &self.type_expansion {
            type_expansion.len()
        } else {
            0
        }
    }

    pub fn pop_type_expansion(&mut self, start: usize, end: usize) -> Option<Vec<String>> {
        if let Some(type_expansion) = &mut self.type_expansion {
            let mut result = Vec::new();
            result.extend(type_expansion.drain(start..end));
            Some(result)
        } else {
            None
        }
    }

    pub fn add_annotation_description(&mut self, annotation_description: String) {
        self.annotation_description
            .push(MarkedString::from_markdown(annotation_description));
    }

    pub fn add_description(&mut self, property_owner: LuaPropertyOwnerId) -> Option<()> {
        let property = self
            .semantic_model
            .get_db()
            .get_property_index()
            .get_property(property_owner.clone())?;

        let detail = property.description.as_ref()?;
        let mut description = detail.to_string();

        match property_owner {
            LuaPropertyOwnerId::Member(id) => {
                if let Some(member) = self
                    .semantic_model
                    .get_db()
                    .get_member_index()
                    .get_member(&id)
                {
                    if let LuaMemberOwner::Type(ty) = &member.get_owner() {
                        if is_std_by_name(&ty.get_name()) {
                            let std_desc =
                                hover_std_description(ty.get_name(), member.get_key().get_name());
                            if !std_desc.is_empty() {
                                description = std_desc;
                            }
                        }
                    }
                }
            }
            LuaPropertyOwnerId::LuaDecl(id) => {
                if let Some(decl) = self.semantic_model.get_db().get_decl_index().get_decl(&id) {
                    if decl.is_global()
                        && is_std_by_name(&decl.get_name())
                        && is_std_by_path(self.semantic_model.get_db(), decl.get_file_id())
                            .unwrap_or(false)
                    {
                        let std_desc = hover_std_description(decl.get_name(), None);
                        if !std_desc.is_empty() {
                            description = std_desc;
                        }
                    }
                }
            }
            _ => {}
        }

        self.add_annotation_description(description);
        Some(())
    }

    pub fn add_signature_params_rets_description(&mut self, typ: LuaType) {
        if let LuaType::Signature(signature_id) = typ {
            add_signature_param_description(
                &self.semantic_model.get_db(),
                &mut self.annotation_description,
                signature_id,
            );
            add_signature_ret_description(
                &self.semantic_model.get_db(),
                &mut self.annotation_description,
                signature_id,
            );
        }
    }

    pub fn get_call_signature(&mut self) -> Option<LuaFunctionType> {
        if self.is_completion {
            return None;
        }
        // 根据当前输入的参数, 匹配完全匹配的签名
        if let Some(token) = self.trigger_token.clone() {
            if let Some(call_expr) = token.parent()?.parent() {
                match call_expr.kind().into() {
                    LuaSyntaxKind::CallExpr => {
                        let call_expr = LuaCallExpr::cast(call_expr)?;
                        let func = self
                            .semantic_model
                            .infer_call_expr_func(call_expr.clone(), None);
                        if let Some(func) = func {
                            // 确定参数量是否与当前输入的参数数量一致, 因为`infer_call_expr_func`必然返回一个有效的类型, 即使不是完全匹配的
                            let call_expr_args_count = call_expr.get_args_count();
                            if let Some(mut call_expr_args_count) = call_expr_args_count {
                                let func_params_count = func.get_params().len();
                                if !func.is_colon_define() && call_expr.is_colon_call() {
                                    // 不是冒号定义的函数, 但是是冒号调用
                                    call_expr_args_count += 1;
                                }
                                if call_expr_args_count == func_params_count {
                                    return Some((*func).clone());
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        None
    }

    /// 推断前缀是否为全局定义, 如果是, 则返回全局名称, 否则返回 None
    pub fn infer_prefix_global_name(&self, member: &LuaMember) -> Option<&str> {
        let root = self
            .semantic_model
            .get_db()
            .get_vfs()
            .get_syntax_tree(&member.get_file_id())?
            .get_red_root();
        let cur_node = member.get_syntax_id().to_node_from_root(&root)?;

        match cur_node.kind().into() {
            LuaSyntaxKind::IndexExpr => {
                let index_expr = LuaIndexExpr::cast(cur_node)?;
                let property_owner = self.semantic_model.get_property_owner_id(
                    index_expr
                        .get_prefix_expr()?
                        .get_syntax_id()
                        .to_node_from_root(&root)
                        .unwrap()
                        .into(),
                );
                if let Some(property_owner) = property_owner {
                    if let LuaPropertyOwnerId::LuaDecl(id) = property_owner {
                        if let Some(decl) =
                            self.semantic_model.get_db().get_decl_index().get_decl(&id)
                        {
                            if decl.is_global() {
                                return Some(decl.get_name());
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        None
    }

    pub fn build_hover_result(&self, range: Option<lsp_types::Range>) -> Option<Hover> {
        let mut result = String::new();
        match &self.type_description {
            MarkedString::String(s) => {
                result.push_str(&format!("\n{}\n", s));
            }
            MarkedString::LanguageString(s) => {
                result.push_str(&format!("\n```{}\n{}\n```\n", s.language, s.value));
            }
        }
        if let Some(location_path) = &self.location_path {
            match location_path {
                MarkedString::String(s) => {
                    result.push_str(&format!("\n{}\n", s));
                }
                _ => {}
            }
        }

        for marked_string in &self.annotation_description {
            match marked_string {
                MarkedString::String(s) => {
                    result.push_str(&format!("\n{}\n", s));
                }
                MarkedString::LanguageString(s) => {
                    result.push_str(&format!("\n```{}\n{}\n```\n", s.language, s.value));
                }
            }
        }

        if let Some(signature_overload) = &self.signature_overload {
            result.push_str("\n---\n");
            for signature in signature_overload {
                match signature {
                    MarkedString::String(s) => {
                        result.push_str(&format!("\n{}\n", s));
                    }
                    MarkedString::LanguageString(s) => {
                        result.push_str(&format!("\n```{}\n{}\n```\n", s.language, s.value));
                    }
                }
            }
        }

        if let Some(type_expansion) = &self.type_expansion {
            for type_expansion in type_expansion {
                result.push_str(&format!("\n```{}\n{}\n```\n", "lua", type_expansion));
            }
        }

        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value: result,
            }),
            range,
        })
    }
}
