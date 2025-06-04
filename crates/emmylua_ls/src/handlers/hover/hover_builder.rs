use emmylua_code_analysis::{
    LuaFunctionType, LuaMember, LuaMemberOwner, LuaSemanticDeclId, LuaType, SemanticModel,
};
use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaSyntaxToken};
use lsp_types::{Hover, HoverContents, MarkedString, MarkupContent};

use crate::handlers::hover::hover_humanize::{
    extract_description_from_property_owner, DescriptionInfo,
};

use super::build_hover::{add_signature_param_description, add_signature_ret_description};

#[derive(Debug)]
pub struct HoverBuilder<'a> {
    /// Type description, does not include overload
    pub type_description: MarkedString,
    /// Full path of the class
    pub location_path: Option<MarkedString>,
    /// Function overload signatures, with the first being the primary overload
    pub signature_overload: Option<Vec<MarkedString>>,
    /// Annotation descriptions, including function parameters and return values
    pub annotation_description: Vec<MarkedString>,
    /// Type expansion, often used for alias types
    pub type_expansion: Option<Vec<String>>,
    /// see
    pub see_content: Option<String>,
    /// other
    pub other_content: Option<String>,

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
            see_content: None,
            other_content: None,
        }
    }

    pub fn set_type_description(&mut self, type_description: String) {
        self.type_description =
            MarkedString::from_language_code("lua".to_string(), type_description);
    }

    pub fn set_location_path(&mut self, owner_member: Option<&LuaMember>) {
        if let Some(owner_member) = owner_member {
            let owner_id = self
                .semantic_model
                .get_db()
                .get_member_index()
                .get_current_owner(&owner_member.get_id());
            if let Some(LuaMemberOwner::Type(ty)) = owner_id {
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
        if signature_overload.is_empty() {
            return;
        }
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
        if type_expansion.is_empty() {
            return;
        }
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
        if annotation_description.is_empty() {
            return;
        }
        self.annotation_description
            .push(MarkedString::from_markdown(annotation_description));
    }

    pub fn add_description(&mut self, property_owner: &LuaSemanticDeclId) -> Option<()> {
        self.add_description_from_info(extract_description_from_property_owner(
            self.semantic_model,
            property_owner,
        ))
    }

    pub fn add_description_from_info(&mut self, type_desc: Option<DescriptionInfo>) -> Option<()> {
        if let Some(desc_info) = type_desc {
            if let Some(description) = desc_info.description {
                self.add_annotation_description(description);
            }

            if let Some(see) = desc_info.see_content {
                self.see_content = Some(see);
            }
            if let Some(other) = desc_info.other_content {
                self.other_content = Some(other);
            }

            Some(())
        } else {
            None
        }
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

    pub fn get_call_function(&mut self) -> Option<LuaFunctionType> {
        if self.is_completion {
            return None;
        }
        // 根据当前输入的参数, 匹配完全匹配的签名
        if let Some(token) = self.trigger_token.clone() {
            if let Some(call_expr) = token.parent()?.parent() {
                if LuaCallExpr::can_cast(call_expr.kind().into()) {
                    let call_expr = LuaCallExpr::cast(call_expr)?;
                    let func = self
                        .semantic_model
                        .infer_call_expr_func(call_expr.clone(), None);
                    if let Some(func) = func {
                        // TODO: 对比参数类型确定是否完全匹配
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
            }
        }

        None
    }

    pub fn build_hover_result(&self, range: Option<lsp_types::Range>) -> Option<Hover> {
        let header = {
            let mut header = String::new();
            match &self.type_description {
                MarkedString::String(s) => {
                    header.push_str(&format!("\n{}\n", s));
                }
                MarkedString::LanguageString(s) => {
                    header.push_str(&format!("\n```{}\n{}\n```\n", s.language, s.value));
                }
            }
            if let Some(location_path) = &self.location_path {
                match location_path {
                    MarkedString::String(s) => {
                        header.push_str(&format!("\n{}\n", s));
                    }
                    _ => {}
                }
            }
            header
        };

        let description_content = {
            let mut content = String::new();

            for marked_string in &self.annotation_description {
                match marked_string {
                    MarkedString::String(s) => {
                        content.push_str(&format!("\n{}\n", s));
                    }
                    MarkedString::LanguageString(s) => {
                        content.push_str(&format!("\n```{}\n{}\n```\n", s.language, s.value));
                    }
                }
            }

            if let Some(see_content) = &self.see_content {
                content.push_str(&format!("\n@*see* {}\n", see_content));
            }

            if let Some(other) = &self.other_content {
                content.push_str("\n\n");
                content.push_str(other);
            }
            content
        };

        let expansion = {
            let mut expansion = String::new();
            if let Some(signature_overload) = &self.signature_overload {
                expansion.push_str("\n---\n");
                for signature in signature_overload {
                    match signature {
                        MarkedString::String(s) => {
                            expansion.push_str(&format!("\n{}\n", s));
                        }
                        MarkedString::LanguageString(s) => {
                            expansion.push_str(&format!("\n```{}\n{}\n```\n", s.language, s.value));
                        }
                    }
                }
            }

            if let Some(type_expansion) = &self.type_expansion {
                for type_expansion in type_expansion {
                    expansion.push_str(&format!("\n```{}\n{}\n```\n", "lua", type_expansion));
                }
            }
            expansion
        };

        let mut result = String::new();

        result.push_str(&header);
        if !description_content.is_empty() || !expansion.is_empty() {
            result.push_str("\n---\n");
        }
        result.push_str(&description_content);
        result.push_str(&expansion);

        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value: result,
            }),
            range,
        })
    }

    pub fn get_trigger_token(&self) -> Option<LuaSyntaxToken> {
        self.trigger_token.clone()
    }
}
