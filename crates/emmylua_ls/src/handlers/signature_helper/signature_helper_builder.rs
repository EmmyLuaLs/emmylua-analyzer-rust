use emmylua_code_analysis::{
    LuaCompilation, LuaMemberOwner, LuaSemanticDeclId, LuaType, SemanticDeclLevel, SemanticModel,
};
use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaExpr};
use lsp_types::{Documentation, MarkupContent, MarkupKind, ParameterInformation, ParameterLabel};
use rowan::NodeOrToken;

use crate::handlers::hover::{find_member_origin_owner, infer_prefix_global_name};

use super::build_signature_helper::{build_function_label, generate_param_label};

#[derive(Debug)]
pub struct SignatureHelperBuilder<'a> {
    pub semantic_model: &'a SemanticModel<'a>,
    pub compilation: &'a LuaCompilation,

    pub call_expr: LuaCallExpr,
    pub prefix_name: Option<String>,
    pub function_name: String,
    self_type: Option<LuaType>,
    params_info: Vec<ParameterInformation>,
    pub best_call_function_label: String,
    pub description: Option<Documentation>,
}

impl<'a> SignatureHelperBuilder<'a> {
    pub fn new(
        compilation: &'a LuaCompilation,
        semantic_model: &'a SemanticModel<'a>,
        call_expr: LuaCallExpr,
    ) -> Self {
        let mut builder = Self {
            compilation,
            semantic_model,
            call_expr,
            prefix_name: None,
            function_name: String::new(),
            self_type: None,
            params_info: Vec::new(),
            best_call_function_label: String::new(),
            description: None,
        };
        builder.self_type = builder.infer_self_type();
        builder.build_full_name();
        builder.generate_best_call_params_info();
        builder
    }

    fn infer_self_type(&self) -> Option<LuaType> {
        let prefix_expr = self.call_expr.get_prefix_expr();
        if let Some(prefix_expr) = prefix_expr
            && let LuaExpr::IndexExpr(index) = prefix_expr
            && let Some(self_expr) = index.get_prefix_expr()
        {
            return self.semantic_model.infer_expr(self_expr).ok();
        }
        None
    }

    pub fn get_self_type(&self) -> Option<LuaType> {
        self.self_type.clone()
    }

    fn build_full_name(&mut self) -> Option<()> {
        let semantic_model = self.semantic_model;
        let db = semantic_model.get_db();
        let prefix_expr = self.call_expr.get_prefix_expr()?;
        let mut semantic_decl = semantic_model.find_decl(
            NodeOrToken::Node(prefix_expr.syntax().clone()),
            SemanticDeclLevel::Trace(50),
        );
        // 推断为来源
        semantic_decl = match semantic_decl {
            Some(LuaSemanticDeclId::Member(member_id)) => {
                find_member_origin_owner(self.compilation, semantic_model, member_id)
                    .or(semantic_decl)
            }
            Some(LuaSemanticDeclId::LuaDecl(_)) => semantic_decl,
            _ => None,
        };
        let semantic_decl = semantic_decl?;

        // 先设置原始描述
        let property = self
            .semantic_model
            .get_db()
            .get_property_index()
            .get_property(&semantic_decl);
        if let Some(property) = property
            && let Some(description) = property.description()
        {
            self.set_description(description.to_string());
        }

        match &semantic_decl {
            LuaSemanticDeclId::Member(member_id) => {
                let member = db.get_member_index().get_member(member_id)?;
                let global_name = infer_prefix_global_name(self.semantic_model, member);
                // 处理前缀
                let parent_owner = db.get_member_index().get_current_owner(&member.get_id());
                if let Some(LuaMemberOwner::Type(ty)) = parent_owner {
                    let mut name = String::new();
                    // 如果是全局定义, 则使用定义时的名称
                    if let Some(global_name) = global_name {
                        name.push_str(global_name);
                    } else {
                        name.push_str(ty.get_simple_name());
                    }
                    self.prefix_name = Some(name);
                }
                self.function_name = member.get_key().to_path().to_string();
            }
            LuaSemanticDeclId::LuaDecl(decl_id) => {
                let decl = db.get_decl_index().get_decl(decl_id)?;
                self.function_name = decl.get_name().to_string();
                // self.set_std_function_description(decl.get_file_id(), decl.get_name(), None);
            }
            _ => {}
        }
        Some(())
    }

    fn set_description(&mut self, description: String) {
        self.description = Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: description,
        }));
    }

    fn generate_best_call_params_info(&mut self) -> Option<()> {
        if !self.params_info.is_empty() {
            return Some(());
        }
        let func = self
            .semantic_model
            .infer_call_expr_func(self.call_expr.clone(), None)?;
        for param in func.get_params() {
            let param_label = generate_param_label(self.semantic_model.get_db(), param.clone());
            self.params_info.push(ParameterInformation {
                label: ParameterLabel::Simple(param_label),
                documentation: None,
            });
        }
        match (func.is_colon_define(), self.call_expr.is_colon_call()) {
            (true, false) => {
                let param_label = generate_param_label(
                    self.semantic_model.get_db(),
                    (String::from("self"), Some(LuaType::SelfInfer)),
                );
                self.params_info.insert(
                    0,
                    ParameterInformation {
                        label: ParameterLabel::Simple(param_label),
                        documentation: None,
                    },
                );
            }
            (false, true) => {
                if !self.params_info.is_empty() {
                    self.params_info.remove(0);
                }
            }
            _ => {}
        }
        self.best_call_function_label = build_function_label(
            self,
            &self.params_info,
            func.is_method(self.semantic_model, None),
            func.get_ret(),
        );

        Some(())
    }

    pub fn get_best_call_params_info(&self) -> &[ParameterInformation] {
        &self.params_info
    }
}
