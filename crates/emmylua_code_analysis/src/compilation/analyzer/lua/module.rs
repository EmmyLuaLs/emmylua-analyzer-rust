use emmylua_parser::LuaChunk;

use crate::{
    compilation::analyzer::unresolve::UnResolveModule, 
    db_index::{LuaMember, LuaMemberKey, LuaMemberOwner, LuaType},
    LuaDeclExtra
};

use super::{func_body::analyze_func_body_returns, LuaAnalyzer, LuaReturnPoint};

pub fn analyze_chunk_return(analyzer: &mut LuaAnalyzer, chunk: LuaChunk) -> Option<()> {
    let block = chunk.get_block()?;
    let return_exprs = analyze_func_body_returns(block);
    for point in return_exprs {
        match point {
            LuaReturnPoint::Expr(expr) => {
                let expr_type = analyzer.infer_expr(&expr);
                let expr_type = match expr_type {
                    Some(expr_type) => expr_type,
                    None => {
                        let unresolve = UnResolveModule {
                            file_id: analyzer.file_id,
                            expr,
                        };
                        analyzer.add_unresolved(unresolve.into());
                        return None;
                    }
                };

                let module_info = analyzer
                    .db
                    .get_module_index_mut()
                    .get_module_mut(analyzer.file_id)?;
                match expr_type {
                    LuaType::MuliReturn(multi) => {
                        let ty = multi.get_type(0)?;
                        module_info.export_type = Some(ty.clone());
                    }
                    _ => {
                        module_info.export_type = Some(expr_type);
                    }
                }

                break;
            }
            // Other cases are stupid code
            _ => {}
        }
    }

    Some(())
}

pub fn analyze_chunk_env(analyzer: &mut LuaAnalyzer, name: String) -> Option<()> {
    let file_id = analyzer.file_id;

    let env_decl_id = {
        let env_decl = analyzer
            .db
            .get_type_index()
            .find_type_decl(analyzer.file_id, &name)?;

        if !env_decl.is_env() {
            return None;
        }
        env_decl.get_id()
    };

    // 修正文件返回值类型为申明的 env
    let module_info = analyzer
        .db
        .get_module_index_mut()
        .get_module_mut(analyzer.file_id)?;

    module_info.export_type = Some(LuaType::Def(env_decl_id.clone()));

    // 将文件内所有全局变量转换为env的变量
    if let Some(decl_tree) = analyzer.db.get_decl_index_mut().get_decl_tree(&file_id){
        let owner =  LuaMemberOwner::Type(env_decl_id);

        let mut decl_list = Vec::new();

        for (decl_id, decl) in decl_tree.get_decls().clone() {
            if decl.is_global() {
                decl_list.push(decl_id);

                let name = decl.get_name();

                // 删除全局标记
                analyzer.db.get_reference_index_mut().remove_global_reference(name, file_id);
                analyzer.db.get_decl_index_mut().remove_global_decl(name);

                let decl_type = match decl.extra.clone() {
                    LuaDeclExtra::Global { kind:_, decl_type } => {
                        decl_type
                    }
                    _ => None
                };

                let member = LuaMember::new(
                    LuaMemberOwner::None, 
                    LuaMemberKey::Name(name.into()), 
                    file_id, 
                    decl.get_syntax_id(), 
                    decl_type);
                let member_id = member.get_id();

                // 添加到env的成员列表
                analyzer.db.get_member_index_mut().add_member(member);
                analyzer.db.get_member_index_mut().add_member_owner(owner.clone(), member_id);
            }
        }

        for decl_id in decl_list {
            // 标记该decl不再是全局变量
            if let Some(decl) = analyzer.db.get_decl_index_mut().get_decl_mut(&decl_id) {
                decl.set_local();
            }
        }
        
    }

    Some(())
}
