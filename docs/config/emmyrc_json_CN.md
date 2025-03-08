# 配置说明

语言服务器会读取项目根目录下的 `.emmyrc.json` 文件，另外为了兼容性, 也会读取 `.luarc.json` 文件。
`.emmyrc.json` 格式和`.luarc.json`配置格式近似，但是`.emmyrc.json`配置格式更加丰富，`.luarc.json`配置格式会被转换为`.emmyrc.json`配置格式。所有`.emmyrc.json`配置会覆盖掉`.luarc.json`配置。
`.emmyrc.json` 配置的内容和`.luarc.json`的配置内容并不完全兼容, 但不兼容的部分会被忽略.

它主要的配置格式是:
```json
{
  "completion": {
    "enable": true,
    "autoRequire": true,
    "autoRequireFunction": "require",
    "autoRequireNamingConvention": "keep",
    "callSnippet": false,
    "postfix": "@"
  },
  "signature": {
    "detailSignatureHelper": false
  },
  "diagnostics": {
    "disable": [
    ],
    "globals": [],
    "globalsRegex": [],
    "severity": {
    },
    "enables": [
    ]
  },
  "hint": {
    "enable": true,
    "paramHint": true,
    "indexHint": true,
    "localHint": true,
    "overrideHint": true
  },
  "runtime": {
    "version": "Lua5.4",
    "requireLikeFunction": [],
    "frameworkVersions": [],
    "extensions": [],
    "requirePattern": []
  },
  "workspace": {
    "ignoreDir": [
    ],
    "ignoreGlobs": [
    ],
    "library": [],
    "workspaceRoots": [],
    "encoding": "",
    "moduleMap": [],
    "reindexDuration": 5000
  },
  "resource": {
    "paths": [
    ]
  },
  "codeLens": {
    "enable": true
  },
  "strict": {
    "requirePath": false,
    "typeCall": true
  },
  "hover": {
    "enable": true
  },
  "references": {
    "enable": true,
    "fuzzy_search": true
  }
}

```

为了补全和提示配置文件, 可以通过添加`"$schema"`项来指定schema文件, schema文件的uri是:
https://github.com/CppCXY/emmylua-analyzer-rust/blob/main/crates/emmylua_code_analysis/resources/schema.json

## completion

- `enable`: 是否启用补全，默认为 `true`。
- `autoRequire`: 是否自动补全 require 语句，默认为 `true`。
- `autoRequireFunction`: 自动补全 require 语句时使用的函数名，默认为 `require`。
- `autoRequireNamingConvention`: 自动补全 require 语句时使用的命名规范，默认为 `camelCase`, 可选值为 `keep`, `camel-case`, `snake-case`, `pascal-case`。
- `callSnippet`: 是否使用代码片段补全函数调用，默认为 `false`。
- `postfix`: 补全时的后缀，默认为 `@`

## signature

- `detailSignatureHelper`: 是否显示详细的函数签名帮助，默认为 `false`。该选项当前无用

## diagnostics

- `enable`: 是否启用诊断，默认为 `true`。
- `disable`: 禁用的诊断信息列表, 如果需要工作区内禁用一些诊断消息, 需要填上对应诊断的id, 例如: `"undefined-global"`
- `globals`: 全局变量列表, 在该列表中的全局变量不会被诊断为未定义.
- `globalsRegex`: 全局变量正则表达式列表, 符合正则表达式的全局变量不会被诊断为未定义.
- `severity`: 诊断消息的严重程度, 例如: `"undefined-global": "warning"`, 可选值为 `"error"`, `"warning"`, `"information"`, `"hint"`.
- `enables`: 启用的诊断信息列表, 语言服务的诊断不是全部都启用的, 可以通过该选项启用一些诊断消息. 例如: `"undefined-field"`

## hint

- `enable`: 是否启用提示，默认为 `true`。
- `paramHint`: 是否显示参数提示，默认为 `true`。
- `indexHint`: 在索引表达式跨行时, 是否显示hint，默认为 `true`。
- `localHint`: 是否显示局部变量提示，默认为 `true`。
- `overrideHint`: 是否显示重载提示，默认为 `true`。

## runtime

- `version`: 运行时版本, 默认为 `Lua5.4`, 可选值为 `Lua5.1`, `Lua5.2`, `Lua5.3`, `Lua5.4`, `LuaJIT`.
- `requireLikeFunction`: 类似 require 的函数列表, 用于识别类似 require 的函数, 例如: `["import"]`.
- `frameworkVersions`: 框架版本列表, 用于识别框架版本, 例如: `["love2d"]`. 可以和emmylua doc 的version标签配合使用.
- `extensions`: 文件扩展名列表, 用于识别文件扩展名, 例如: `[".lua", ".lua.txt"]`.
- `requirePattern`: require 模式列表, 该参数和lua中的package.path和package.cpath有关, 例如: `["?.lua", "?.lua.txt"]`. 默认不需要填写, 将自动拥有,
`["?.lua", "?/init.lua"]`.

## workspace

工作区配置, 大部分工作区配置本身既支持相对路径也支持绝对路径

- `ignoreDir`: 忽略的目录列表, 用于忽略一些目录, 例如: `["build", "dist"]`.
- `ignoreGlobs`: 忽略的文件列表, 基于正则表达式的忽略一些文件, 例如: `["*.log", "*.tmp"]`.
- `library`: 库文件目录列表, 用于指定一些库文件, 例如: `["/usr/local/lib"]`. 
- `workspaceRoots`: 工作区根目录列表, 用于指定工作区的根目录, 例如: `["Assets/script/Lua"]`. 该功能主要是为了让require正常工作, 如果必须要打开lua主目录的上级目录, 需要在这里添加当前打开的目录相对于lua主目录的相对路径.
- `preloadFileSize`: 预加载文件大小, 默认为 `1048576` 字节, 用于控制预加载文件的大小.
- `encoding`: 文件编码, 默认为 `utf-8`, 用于读取文件时的编码.
- `moduleMap`: 模块映射列表, 用于指定模块映射, 例如: 
```json
{ 
  "pattern" : "^lib(.*)$", 
  "replace" : "script$1"
}
```
- `reindexDuration`: 重新索引的时间间隔, 默认为 `5000` 毫秒, 用于控制重新索引的时间间隔.

该功能主要是为了让require正常工作, 如果需要将以lib为起始的模块, 映射到以script为起始, 需要在这里添加映射关系.

## resource

- `paths`: 资源路径列表, 用于指定需要加载的资源的根目录, 例如: `["Assets/settings"]`. 其默认值为当前打开的工作区目录, emmylua支持在任意字符串中的文件路径补全, 以及任意字符串中的文件路径跳转. 通过配置这个目录, 可以让emmylua知道哪些目录是资源目录, 从而正确的提供补全和跳转.

## codeLens

- `enable`: 是否启用CodeLens功能, 默认为 `true`.

## strict

- `requirePath`: 是否启用require严格模式, 默认为 `false`. 严格模式时, require必须从指定的根目录开始, 否则无法跳转
- `typeCall`: 是否启用类型调用时严格模式, 默认为 `true`. 严格模式时, 类型调用必须手动写好重载, 否则返回unknown, 非严格模式时, 类型调用会返回自身

## hover

- `enable`: 是否启用hover功能, 默认为 `true`.

## references

- `enable`: 是否启用references功能, 默认为 `true`.
- `fuzzy_search`: 是否启用模糊搜索, 默认为 `true`.