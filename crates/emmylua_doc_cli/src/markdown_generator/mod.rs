mod global_gen;
mod index_gen;
mod init_tl;
mod mixin_copy;
mod mod_gen;
mod render;
mod typ_gen;

use std::path::PathBuf;

use emmylua_code_analysis::EmmyLuaAnalysis;
use serde::{Deserialize, Serialize};

#[allow(unused)]
pub fn generate_markdown(
    analysis: &mut EmmyLuaAnalysis,
    input: PathBuf,
    output: PathBuf,
    override_template: Option<PathBuf>,
    mixin: Option<PathBuf>,
) -> Option<()> {
    let docs_dir = output.join("docs");
    let types_out = docs_dir.join("types");
    if !types_out.exists() {
        println!("Creating types directory: {:?}", types_out);
        std::fs::create_dir_all(&types_out).ok()?;
    } else {
        println!("Clearing types directory: {:?}", types_out);
        std::fs::remove_dir_all(&types_out).ok()?;
        std::fs::create_dir_all(&types_out).ok()?;
    }

    let module_out = docs_dir.join("modules");
    if !module_out.exists() {
        println!("Creating modules directory: {:?}", module_out);
        std::fs::create_dir_all(&module_out).ok()?;
    } else {
        println!("Clearing modules directory: {:?}", module_out);
        std::fs::remove_dir_all(&module_out).ok()?;
        std::fs::create_dir_all(&module_out).ok()?;
    }

    let global_out = docs_dir.join("globals");
    if !global_out.exists() {
        println!("Creating globals directory: {:?}", global_out);
        std::fs::create_dir_all(&global_out).ok()?;
    } else {
        println!("Clearing globals directory: {:?}", global_out);
        std::fs::remove_dir_all(&global_out).ok()?;
        std::fs::create_dir_all(&global_out).ok()?;
    }

    let tl = init_tl::init_tl(override_template)?;
    let mut mkdocs_index = MkdocsIndex::default();
    let db = analysis.compilation.get_db();
    let type_index = db.get_type_index();
    let types = type_index.get_all_types();
    for type_decl in types {
        typ_gen::generate_type_markdown(db, &tl, type_decl, &types_out, &mut mkdocs_index);
    }

    let module_index = db.get_module_index();
    let modules = module_index.get_module_infos();
    for module in modules {
        mod_gen::generate_module_markdown(db, &tl, module, &module_out, &mut mkdocs_index);
    }

    let decl_index = db.get_decl_index();
    let globals = decl_index.get_global_decls();
    for global_decl_id in globals {
        global_gen::generate_global_markdown(
            db,
            &tl,
            &global_decl_id,
            &global_out,
            &mut mkdocs_index,
        );
    }

    index_gen::generate_index(&tl, &mut mkdocs_index, &output);

    if let Some(mixin) = mixin {
        mixin_copy::mixin_copy(&output, mixin);
    }

    Some(())
}

#[derive(Debug, Serialize, Deserialize)]
struct MemberDisplay {
    pub name: String,
    pub display: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct MkdocsIndex {
    pub types: Vec<IndexStruct>,
    pub modules: Vec<IndexStruct>,
    pub globals: Vec<IndexStruct>,
}

#[derive(Debug, Serialize, Deserialize)]
struct IndexStruct {
    pub name: String,
    pub file: String,
}

fn escape_type_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            // Windows Invalid Characters
            if "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect()
}
