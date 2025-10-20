use emmylua_code_analysis::{
    GenericTplId, LuaCompilation, LuaMember, LuaMemberOwner, LuaSemanticDeclId, LuaType,
    RenderLevel, SemanticModel, TypeSubstitutor,
};
use emmylua_parser::{
    LuaAstNode, LuaCallExpr, LuaExpr, LuaLocalName, LuaLocalStat, LuaSyntaxKind, LuaSyntaxToken,
};
use lsp_types::{Hover, HoverContents, MarkedString, MarkupContent};

use crate::handlers::hover::hover_humanize::{
    DescriptionInfo, extract_description_from_property_owner,
};

use super::build_hover::{add_signature_param_description, add_signature_ret_description};

#[derive(Debug)]
pub struct HoverBuilder<'a> {
    /// Type description, does not include overload
    pub primary: MarkedString,
    /// Full path of the class
    pub location_path: Option<MarkedString>,
    /// Function overload signatures, with the first being the primary overload
    pub signature_overload: Option<Vec<MarkedString>>,
    /// Annotation descriptions, including function parameters and return values
    pub annotation_description: Vec<MarkedString>,
    /// 一些类型的完整追加显示, 通常是 @alias
    pub type_expansion: Option<Vec<String>>,
    /// For `@see` and unknown tags tags
    tag_content: Option<Vec<(String, String)>>,

    trigger_token: Option<LuaSyntaxToken>,
    pub semantic_model: &'a SemanticModel<'a>,
    pub compilation: &'a LuaCompilation,
    pub detail_render_level: RenderLevel,

    pub is_completion: bool,
    // 默认的泛型替换器
    pub substitutor: Option<TypeSubstitutor>,
}

impl<'a> HoverBuilder<'a> {
    pub fn new(
        compilation: &'a LuaCompilation,
        semantic_model: &'a SemanticModel,
        token: Option<LuaSyntaxToken>,
        is_completion: bool,
    ) -> Self {
        let detail_render_level =
            if let Some(custom_detail) = semantic_model.get_emmyrc().hover.custom_detail {
                RenderLevel::CustomDetailed(custom_detail)
            } else {
                RenderLevel::Detailed
            };

        let substitutor = if let Some(token) = token.clone() {
            infer_substitutor_base_type(semantic_model, token)
        } else {
            None
        };

        Self {
            compilation,
            semantic_model,
            primary: MarkedString::String("".to_string()),
            location_path: None,
            signature_overload: None,
            annotation_description: Vec::new(),
            is_completion,
            trigger_token: token,
            type_expansion: None,
            tag_content: None,
            detail_render_level,
            substitutor,
        }
    }

    pub fn set_type_description(&mut self, type_description: String) {
        self.primary = MarkedString::from_language_code("lua".to_string(), type_description);
    }

    pub fn set_location_path(&mut self, owner_member: Option<&LuaMember>) {
        if let Some(owner_member) = owner_member {
            let owner_id = self
                .semantic_model
                .get_db()
                .get_member_index()
                .get_current_owner(&owner_member.get_id());
            if let Some(LuaMemberOwner::Type(ty)) = owner_id
                && ty.get_name() != ty.get_simple_name()
            {
                self.location_path = Some(MarkedString::from_markdown(format!(
                    "{}{} `{}`",
                    "&nbsp;&nbsp;",
                    "in class",
                    ty.get_name()
                )));
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

            if let Some(tag_content) = desc_info.tag_content {
                self.tag_content = Some(tag_content);
            }

            Some(())
        } else {
            None
        }
    }

    pub fn add_signature_params_rets_description(&mut self, typ: LuaType) {
        if let LuaType::Signature(signature_id) = typ {
            add_signature_param_description(
                self.semantic_model.get_db(),
                &mut self.annotation_description,
                signature_id,
            );
            add_signature_ret_description(
                self.semantic_model.get_db(),
                &mut self.annotation_description,
                signature_id,
            );
        }
    }

    pub fn build_hover_result(&self, range: Option<lsp_types::Range>) -> Option<Hover> {
        let header = {
            let mut header = String::new();
            match &self.primary {
                MarkedString::String(s) => {
                    header.push_str(&format!("\n{}\n", s));
                }
                MarkedString::LanguageString(s) => {
                    header.push_str(&format!("\n```{}\n{}\n```\n", s.language, s.value));
                }
            }
            if let Some(location_path) = &self.location_path
                && let MarkedString::String(s) = location_path
            {
                header.push_str(&format!("\n{}\n", s));
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

            if let Some(tag_content) = &self.tag_content {
                if !tag_content.is_empty() {
                    content.push_str("\n---\n");
                }
                for (tag_name, description) in tag_content {
                    content.push_str(&format!("\n@*{}* {}\n", tag_name, description));
                }
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

        // 清除空白字符
        result = result.trim().to_string();

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

    pub fn get_call_expr(&self) -> Option<LuaCallExpr> {
        if let Some(token) = self.trigger_token.clone()
            && let Some(call_expr) = token.parent()?.parent()
            && LuaCallExpr::can_cast(call_expr.kind().into())
        {
            return Some(LuaCallExpr::cast(call_expr)?);
        }
        None
    }
}

// 推断基础泛型替换器
fn infer_substitutor_base_type(
    semantic_model: &SemanticModel,
    trigger_token: LuaSyntaxToken,
) -> Option<TypeSubstitutor> {
    let parent = trigger_token.parent()?;
    match parent.kind().into() {
        LuaSyntaxKind::LocalName => {
            let target_local_name = LuaLocalName::cast(parent.clone())?;
            let parent = parent.parent()?;
            match parent.kind().into() {
                LuaSyntaxKind::LocalStat => {
                    let local_stat = LuaLocalStat::cast(parent.clone())?;
                    let local_name_list = local_stat.get_local_name_list().collect::<Vec<_>>();
                    let value_expr_list = local_stat.get_value_exprs().collect::<Vec<_>>();

                    for (index, name) in local_name_list.iter().enumerate() {
                        if target_local_name == *name {
                            let value_expr = value_expr_list.get(index)?;
                            return substitutor_form_expr(semantic_model, value_expr);
                        }
                    }
                }
                _ => return None,
            }
        }
        _ => return None,
    }

    None
}

pub fn substitutor_form_expr(
    semantic_model: &SemanticModel,
    expr: &LuaExpr,
) -> Option<TypeSubstitutor> {
    if let LuaExpr::IndexExpr(index_expr) = expr {
        let prefix_type = semantic_model
            .infer_expr(index_expr.get_prefix_expr()?)
            .ok()?;
        let mut substitutor = TypeSubstitutor::new();
        if let LuaType::Generic(generic) = prefix_type {
            for (i, param) in generic.get_params().iter().enumerate() {
                substitutor.insert_type(GenericTplId::Type(i as u32), param.clone());
            }
            return Some(substitutor);
        } else {
            return None;
        }
    }
    None
}
