use emmylua_code_analysis::{LuaSemanticDeclId, LuaType};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HoverDeclInfo {
    id: LuaSemanticDeclId,
    typ: LuaType,
}

impl HoverDeclInfo {
    pub(crate) fn new(id: LuaSemanticDeclId, typ: LuaType) -> Self {
        Self { id, typ }
    }

    pub(crate) fn id(&self) -> &LuaSemanticDeclId {
        &self.id
    }

    pub(crate) fn typ(&self) -> &LuaType {
        &self.typ
    }
}

#[derive(Debug, Clone)]
pub(crate) struct HoverDeclContext {
    current_decl: HoverDeclInfo,
    origin_decls: Vec<HoverDeclInfo>,
}

impl HoverDeclContext {
    pub(crate) fn new(current_decl: HoverDeclInfo, origin_decls: Vec<HoverDeclInfo>) -> Self {
        Self {
            current_decl,
            origin_decls,
        }
    }

    pub(crate) fn current_decl(&self) -> &HoverDeclInfo {
        &self.current_decl
    }

    pub(crate) fn origin_decls(&self) -> &[HoverDeclInfo] {
        &self.origin_decls
    }

    fn primary_decl(&self) -> &HoverDeclInfo {
        self.origin_decls
            .iter()
            .find(|decl| decl.typ().is_signature())
            .or_else(|| {
                self.origin_decls
                    .iter()
                    .find(|decl| decl.typ() == self.current_decl.typ())
            })
            .or_else(|| self.origin_decls.first())
            .unwrap_or(&self.current_decl)
    }

    pub(crate) fn ordered_decl_refs(&self) -> Vec<&HoverDeclInfo> {
        let mut decls = if self.origin_decls.is_empty() {
            vec![&self.current_decl]
        } else {
            self.origin_decls.iter().collect::<Vec<_>>()
        };

        if let Some(pos) = decls
            .iter()
            .position(|decl| decl.typ() == self.current_decl.typ())
        {
            if pos != 0 {
                let item = decls.remove(pos);
                decls.insert(0, item);
            }
        }

        let primary_decl = self.primary_decl();
        if let Some(pos) = decls.iter().position(|decl| *decl == primary_decl) {
            if pos != 0 {
                let item = decls.remove(pos);
                decls.insert(0, item);
            }
        }

        decls
    }
}
