use luars::{
    LuaResult, LuaVM, LuaValue,
    lua_vm::{LuaState, SafeOption},
};
use serde_json::Value;

pub fn load_lua_config(content: &str) -> Result<Value, String> {
    let mut lua = LuaVM::new(SafeOption {
        max_call_depth: 64,
        max_stack_size: 256,
        max_memory_limit: 100 * 1024 * 1024, // 100 MB
    });

    let _ = lua.open_stdlib(luars::Stdlib::Package);
    let _ = lua.open_stdlib(luars::Stdlib::Basic);
    let _ = lua.open_stdlib(luars::Stdlib::Table);
    let _ = lua.open_stdlib(luars::Stdlib::String);
    let _ = lua.open_stdlib(luars::Stdlib::Math);
    let _ = lua.open_stdlib(luars::Stdlib::Os);
    let _ = lua.open_stdlib(luars::Stdlib::Utf8);
    let _ = lua.set_global("print", LuaValue::cfunction(ls_println));

    let values = match lua.execute_string(content) {
        Ok(v) => v,
        Err(e) => {
            let err_msg = lua.main_state().get_error_msg(e);
            return Err(format!("Failed to execute lua config: {:?}", err_msg));
        }
    };

    if values.is_empty() {
        return Err("Lua config did not return any value".to_string());
    }

    let value = values[0];
    lua.serialize_to_json(&value)
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
