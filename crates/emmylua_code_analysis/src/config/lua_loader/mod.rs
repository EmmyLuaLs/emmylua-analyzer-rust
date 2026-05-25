use std::time::Duration;

use luars::{
    Lua, LuaApi, LuaResult, LuaSandboxApi, LuaState, LuaValue, SafeOption, SandboxConfig, Table,
};
use serde_json::Value;

pub fn load_lua_config(content: &str) -> Result<Value, String> {
    let mut lua = Lua::new(SafeOption::default());
    let libs = [
        luars::Stdlib::Package,
        luars::Stdlib::Basic,
        luars::Stdlib::Table,
        luars::Stdlib::String,
        luars::Stdlib::Math,
        luars::Stdlib::Os,
        luars::Stdlib::Utf8,
    ];
    for lib in libs.iter() {
        let _ = lua.open_stdlib(*lib);
    }

    let _ = lua.set_global("print", LuaValue::cfunction(ls_println));
    let sandbox = SandboxConfig {
        basic: true,
        math: true,
        string: true,
        table: true,
        utf8: true,
        coroutine: false,
        os: true,
        io: false,
        package: true,
        debug: false,
        allow_require: true,
        allow_load: false,
        allow_loadfile: false,
        allow_dofile: false,
        timeout: Some(Duration::from_secs(1)),
        memory_limit_bytes: Some(10 * 1024 * 1024), // 10 MB
        ..Default::default()
    };

    let r = match lua.load_sandboxed(content, &sandbox).eval::<Table>() {
        Ok(v) => v,
        Err(e) => {
            let err_msg = lua.get_error_message(e);
            return Err(format!("Failed to execute lua config: {:?}", err_msg));
        }
    };

    serde_json::to_value(r).map_err(|e| format!("Failed to convert lua table to json: {:?}", e))
}

fn ls_println(l: &mut LuaState) -> LuaResult<usize> {
    let args = l.get_args();
    let mut output = String::new();
    for (index, arg) in args.iter().enumerate() {
        let s = l.to_string(arg)?;
        output.push_str(&s);
        if index < args.len() - 1 {
            output.push('\t');
        }
    }
    log::info!("{}", output);
    Ok(0)
}
