use std::sync::Arc;

use crate::{
    DbIndex, LuaDeclId, LuaFunctionType, LuaType, TypeOps,
};

use super::{
    CompilationDeclInfo, build_compilation_signature_doc_function, decl_initializer_type,
    decl_value_shell_type, find_compilation_decl_by_position, infer_compilation_decl_type,
};

#[derive(Debug, Clone)]
pub struct CompilationGlobalDecl {
    pub decl: CompilationDeclInfo,
    pub typ: Option<LuaType>,
}

#[derive(Debug, Clone)]
pub struct CompilationGlobalFunction {
    pub decl_id: Option<LuaDeclId>,
    pub typ: Option<Arc<LuaFunctionType>>,
}

#[derive(Debug, Clone, Default)]
pub struct CompilationGlobals {
    pub decls: Vec<CompilationGlobalDecl>,
    pub functions: Vec<CompilationGlobalFunction>,
}

pub fn globals(db: &DbIndex, name: &str) -> CompilationGlobals {
    let mut globals = CompilationGlobals::default();

    for file_id in db.get_vfs().get_all_file_ids() {
        let Some(summary) = db.get_summary_db().file().globals(file_id) else {
            continue;
        };

        for entry in &summary.entries {
            if entry.name.as_str() != name {
                continue;
            }

            for decl_id in &entry.decl_ids {
                let Some(decl) = find_compilation_decl_by_position(db, file_id, decl_id.as_position())
                else {
                    continue;
                };
                let typ = global_decl_type(db, &decl);
                globals.decls.push(CompilationGlobalDecl { decl, typ });
            }
        }

        for function in &summary.functions {
            if function.name.as_str() != name {
                continue;
            }

            globals.functions.push(CompilationGlobalFunction {
                decl_id: function
                    .decl_id
                    .map(|decl_id| LuaDeclId::new(file_id, decl_id.as_position())),
                typ: build_compilation_signature_doc_function(db, file_id, function.signature_offset),
            });
        }
    }

    globals
}

pub fn global_type(db: &DbIndex, name: &str) -> Option<LuaType> {
    let globals = globals(db, name);
    if !globals.functions.is_empty() {
        let mut function_types = globals
            .functions
            .into_iter()
            .map(|function| {
                function
                    .typ
                    .map(LuaType::DocFunction)
                    .unwrap_or(LuaType::Function)
            });
        let first = function_types.next()?;
        return Some(function_types.fold(first, |acc, ty| TypeOps::Union.apply(db, &acc, &ty)));
    }

    let mut collected = Vec::new();

    for candidate in globals.decls {
        let Some(typ) = candidate.typ else {
            continue;
        };

        if typ.is_def() || typ.is_ref() {
            return Some(typ);
        }

        collected.push(typ);
    }

    let mut types = collected.into_iter();
    let first = types.next()?;
    Some(types.fold(first, |acc, ty| TypeOps::Union.apply(db, &acc, &ty)))
}

fn global_decl_type(db: &DbIndex, decl: &CompilationDeclInfo) -> Option<LuaType> {
    let initializer_type = decl_initializer_type(db, decl).filter(|ty| !ty.is_unknown());
    let typ = infer_compilation_decl_type(db, decl).or_else(|| initializer_type.clone())?;
    let has_declared_type = decl.decl_type.as_ref().is_some_and(|decl_type| {
        !decl_type.explicit_type_offsets.is_empty() || !decl_type.named_type_names.is_empty()
    });

    if decl
        .decl_type
        .as_ref()
        .and_then(|decl_type| decl_type.source_call_syntax_id)
        .is_some()
    {
        return decl_initializer_type(db, decl)
            .filter(|initializer_ty| !initializer_ty.is_unknown())
            .or(Some(typ));
    }

    if !has_declared_type && (typ.is_unknown() || typ.contain_tpl()) {
        return initializer_type
            .filter(|initializer_ty| {
                !initializer_ty.is_unknown()
                    && (!initializer_ty.is_generic() || !initializer_ty.contain_tpl())
            })
            .or(Some(typ));
    }

    if !has_declared_type
        && let Some(initializer_ty) = initializer_type.as_ref()
        && !initializer_ty.contain_tpl()
        && (!initializer_ty.is_generic()
            || matches!(initializer_ty, LuaType::Generic(generic) if !generic.get_params().is_empty()))
    {
        if typ.is_generic() {
            return Some(initializer_ty.clone());
        }

        if let Some(value_shell_type) = decl_value_shell_type(db, decl)
            && !value_shell_type.is_unknown()
            && (value_shell_type.is_generic() || value_shell_type.contain_tpl())
        {
            return Some(initializer_ty.clone());
        }
    }

    if let Some(value_shell_type) = decl_value_shell_type(db, decl)
        && !value_shell_type.is_unknown()
    {
        return Some(value_shell_type);
    }

    Some(typ)
}
