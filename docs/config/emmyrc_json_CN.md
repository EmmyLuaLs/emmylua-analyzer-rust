# EmmyLua 配置指南

[English](./emmyrc_json_EN.md)

EmmyLua Analyzer Rust 通过项目根目录的 `.emmyrc.json` 文件进行配置。也兼容 `.luarc.json`，但功能支持有限，推荐使用 `.emmyrc.json`。

## Schema 支持

在配置文件中添加以下字段可获得编辑器内的智能补全与校验：

```json
{
  "$schema": "https://raw.githubusercontent.com/EmmyLuaLs/emmylua-analyzer-rust/refs/heads/main/crates/emmylua_code_analysis/resources/schema.json"
}
```

---

## 完整配置示例

<details>
<summary>点击展开</summary>

```json
{
    "$schema": "https://raw.githubusercontent.com/EmmyLuaLs/emmylua-analyzer-rust/refs/heads/main/crates/emmylua_code_analysis/resources/schema.json",
    "codeAction": {
        "insertSpace": false
    },
    "codeLens": {
        "enable": true
    },
    "completion": {
        "enable": true,
        "autoRequire": true,
        "autoRequireFunction": "require",
        "autoRequireNamingConvention": "keep",
        "autoRequireSeparator": ".",
        "callSnippet": false,
        "postfix": "@",
        "baseFunctionIncludesName": true
    },
    "diagnostics": {
        "enable": true,
        "disable": [],
        "enables": [],
        "globals": [],
        "globalsRegex": [],
        "severity": {},
        "diagnosticInterval": 500
    },
    "doc": {
        "syntax": "md"
    },
    "documentColor": {
        "enable": true
    },
    "hover": {
        "enable": true
    },
    "hint": {
        "enable": true,
        "paramHint": true,
        "indexHint": true,
        "localHint": true,
        "overrideHint": true,
        "metaCallHint": true
    },
    "inlineValues": {
        "enable": true
    },
    "references": {
        "enable": true,
        "fuzzySearch": true,
        "shortStringSearch": false
    },
    "reformat": {
        "externalTool": null,
        "externalToolRangeFormat": null,
        "useDiff": false
    },
    "resource": {
        "paths": []
    },
    "runtime": {
        "version": "LuaLatest",
        "requireLikeFunction": [],
        "frameworkVersions": [],
        "extensions": [],
        "requirePattern": [],
        "classDefaultCall": {
            "functionName": "",
            "forceNonColon": false,
            "forceReturnSelf": false
        },
        "nonstandardSymbol": [],
        "special": {}
    },
    "semanticTokens": {
        "enable": true
    },
    "signature": {
        "detailSignatureHelper": true
    },
    "strict": {
        "requirePath": false,
        "typeCall": false,
        "arrayIndex": true,
        "metaOverrideFileDefine": true,
        "docBaseConstMatchBaseType": true
    },
    "workspace": {
        "ignoreDir": [],
        "ignoreGlobs": [],
        "library": [],
        "workspaceRoots": [],
        "preloadFileSize": 0,
        "encoding": "utf-8",
        "moduleMap": [],
        "reindexDuration": 5000,
        "enableReindex": false
    }
}
```

</details>

---

## 配置详解

### completion — 代码补全

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `enable` | `boolean` | `true` | 启用代码补全 |
| `autoRequire` | `boolean` | `true` | 自动补全 require 语句 |
| `autoRequireFunction` | `string` | `"require"` | 自动补全时使用的函数名 |
| `autoRequireNamingConvention` | `string` | `"keep"` | 命名风格转换（`keep` / `camel-case` / `snake-case` / `pascal-case`） |
| `autoRequireSeparator` | `string` | `"."` | 自动引用路径分隔符 |
| `callSnippet` | `boolean` | `false` | 启用函数调用代码片段 |
| `postfix` | `string` | `"@"` | 后缀补全触发符号 |
| `baseFunctionIncludesName` | `boolean` | `true` | 基础函数补全时包含函数名 |

---

### codeAction — 代码操作

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `insertSpace` | `boolean` | `false` | 插入 `@diagnostic disable-next-line` 时在 `---` 后添加空格 |

---

### codeLens — 代码透镜

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `enable` | `boolean` | `true` | 启用 CodeLens |

---

### diagnostics — 代码诊断

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `enable` | `boolean` | `true` | 启用诊断 |
| `disable` | `string[]` | `[]` | 禁用的诊断规则列表 |
| `enables` | `string[]` | `[]` | 额外启用的诊断规则列表 |
| `globals` | `string[]` | `[]` | 全局变量白名单 |
| `globalsRegex` | `string[]` | `[]` | 全局变量正则匹配 |
| `severity` | `object` | `{}` | 自定义诊断严重程度 |
| `diagnosticInterval` | `number` | `500` | 诊断刷新间隔（毫秒） |

严重程度可选值：`error` / `warning` / `information` / `hint`

示例：

```json
{
  "diagnostics": {
    "disable": ["undefined-global"],
    "severity": {
      "undefined-global": "warning",
      "unused": "hint"
    },
    "enables": ["undefined-field"]
  }
}
```

#### 可用诊断规则

| 规则名 | 描述 | 默认级别 |
|--------|------|----------|
| `syntax-error` | 语法错误 | error |
| `doc-syntax-error` | 文档语法错误 | error |
| `undefined-global` | 未定义的全局变量 | error |
| `local-const-reassign` | 局部常量重新赋值 | error |
| `annotation-usage-error` | 注解使用错误 | error |
| `type-not-found` | 类型未找到 | warning |
| `missing-return` | 缺少返回语句 | warning |
| `param-type-not-match` | 参数类型不匹配 | warning |
| `missing-parameter` | 缺少参数 | warning |
| `redundant-parameter` | 冗余参数 | warning |
| `access-invisible` | 访问不可见成员 | warning |
| `discard-returns` | 丢弃返回值 | warning |
| `undefined-field` | 未定义的字段 | warning |
| `iter-variable-reassign` | 迭代变量重新赋值 | warning |
| `duplicate-type` | 重复类型定义 | warning |
| `redefined-label` | 重新定义标签 | warning |
| `code-style-check` | 代码风格检查 | warning |
| `need-check-nil` | 需要检查 nil | warning |
| `await-in-sync` | 同步代码中使用 await | warning |
| `return-type-mismatch` | 返回类型不匹配 | warning |
| `missing-return-value` | 缺少返回值 | warning |
| `redundant-return-value` | 冗余返回值 | warning |
| `undefined-doc-param` | 文档中未定义的参数 | warning |
| `duplicate-doc-field` | 重复的文档字段 | warning |
| `missing-fields` | 缺少字段 | warning |
| `inject-field` | 注入字段 | warning |
| `circle-doc-class` | 循环类继承 | warning |
| `incomplete-signature-doc` | 不完整的签名文档 | warning |
| `missing-global-doc` | 缺少全局变量文档 | warning |
| `assign-type-mismatch` | 赋值类型不匹配 | warning |
| `non-literal-expressions-in-assert` | assert 中使用非字面量表达式 | warning |
| `unbalanced-assignments` | 不平衡的赋值 | warning |
| `unnecessary-assert` | 不必要的 assert | warning |
| `unnecessary-if` | 不必要的 if 判断 | warning |
| `duplicate-set-field` | 重复设置字段 | warning |
| `duplicate-index` | 重复索引 | warning |
| `generic-constraint-mismatch` | 泛型约束不匹配 | warning |
| `unreachable-code` | 不可达代码 | hint |
| `unused` | 未使用的变量/函数 | hint |
| `deprecated` | 已弃用的功能 | hint |
| `redefined-local` | 重新定义局部变量 | hint |
| `duplicate-require` | 重复 require | hint |

---

### doc — 文档语法

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `syntax` | `string` | `"md"` | 文档注释语法（`md` = Markdown，`myst` = MyST） |

---

### documentColor — 文档颜色

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `enable` | `boolean` | `true` | 启用文档中的颜色预览 |

---

### hint — 内联提示

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `enable` | `boolean` | `true` | 启用内联提示 |
| `paramHint` | `boolean` | `true` | 显示函数参数名提示 |
| `indexHint` | `boolean` | `true` | 显示跨行索引表达式提示 |
| `localHint` | `boolean` | `true` | 显示局部变量类型提示 |
| `overrideHint` | `boolean` | `true` | 显示方法重载提示 |
| `metaCallHint` | `boolean` | `true` | 显示元表 `__call` 调用提示 |

---

### hover — 悬浮提示

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `enable` | `boolean` | `true` | 启用鼠标悬浮提示 |

---

### inlineValues — 内联值

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `enable` | `boolean` | `true` | 启用调试时的内联值显示 |

---

### references — 引用查找

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `enable` | `boolean` | `true` | 启用引用查找 |
| `fuzzySearch` | `boolean` | `true` | 启用模糊搜索 |
| `shortStringSearch` | `boolean` | `false` | 启用短字符串搜索 |

---

### reformat — 代码格式化

参见 [外部格式化工具选项](../external_format/external_formatter_options_CN.md)

---

### resource — 资源路径

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `paths` | `string[]` | `[]` | 资源文件根目录列表，用于文件路径补全和跳转 |

---

### runtime — 运行时环境

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `version` | `string` | `"LuaLatest"` | Lua 版本（`Lua5.1` / `Lua5.2` / `Lua5.3` / `Lua5.4` / `LuaJIT` / `LuaLatest`） |
| `requireLikeFunction` | `string[]` | `[]` | 类似 require 的函数列表 |
| `frameworkVersions` | `string[]` | `[]` | 框架版本标识 |
| `extensions` | `string[]` | `[]` | 额外的文件扩展名 |
| `requirePattern` | `string[]` | `[]` | require 路径匹配模式 |
| `classDefaultCall` | `object` | `{}` | 类默认调用配置 |
| `nonstandardSymbol` | `string[]` | `[]` | 非标准符号列表（如 `"continue"`） |
| `special` | `object` | `{}` | 特殊函数映射 |

示例：

```json
{
  "runtime": {
    "version": "LuaLatest",
    "requireLikeFunction": ["import", "load", "dofile"],
    "frameworkVersions": ["love2d", "openresty"],
    "extensions": [".lua", ".lua.txt", ".luau"],
    "requirePattern": ["?.lua", "?/init.lua", "lib/?.lua"],
    "classDefaultCall": {
      "functionName": "new",
      "forceNonColon": false,
      "forceReturnSelf": true
    },
    "nonstandardSymbol": ["continue"],
    "special": {
      "errorf": "error"
    }
  }
}
```

---

### semanticTokens — 语义标记

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `enable` | `boolean` | `true` | 启用语义标记 |

---

### signature — 函数签名

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `detailSignatureHelper` | `boolean` | `false` | 显示详细函数签名帮助（当前无效） |

---

### strict — 严格模式

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `requirePath` | `boolean` | `false` | require 路径必须从指定根目录开始 |
| `typeCall` | `boolean` | `false` | 类型调用需手动定义重载 |
| `arrayIndex` | `boolean` | `false` | 严格数组索引检查 |
| `metaOverrideFileDefine` | `boolean` | `true` | 元定义覆盖文件中的定义 |
| `docBaseConstMatchBaseType` | `boolean` | `true` | 文档基类常量匹配基础类型 |

> **提示**：禁用严格模式时，行为更接近 `luals` 的宽松模式。

---

### workspace — 工作区

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `ignoreDir` | `string[]` | `[]` | 忽略的目录列表 |
| `ignoreGlobs` | `string[]` | `[]` | 基于 glob 模式忽略文件 |
| `library` | `string[]` | `[]` | 库文件目录路径 |
| `workspaceRoots` | `string[]` | `[]` | 工作区根目录列表 |
| `encoding` | `string` | `"utf-8"` | 文件编码 |
| `moduleMap` | `object[]` | `[]` | 模块路径映射（支持正则） |
| `reindexDuration` | `number` | `5000` | 重新索引间隔（毫秒） |
| `enableReindex` | `boolean` | `false` | 启用自动重新索引 |

模块映射示例：

```json
{
  "workspace": {
    "moduleMap": [
      { "pattern": "^lib(.*)$", "replace": "script$1" }
    ],
    "ignoreDir": ["build", "dist", "node_modules"],
    "library": ["/usr/local/lib/lua", "./libs"],
    "workspaceRoots": ["Assets/Scripts/Lua"]
  }
}
```

---

## 快速上手

1. 在项目根目录创建 `.emmyrc.json`
2. 添加 `$schema` 字段以获得编辑器智能提示
3. 按需添加配置项
4. 保存后语言服务器会自动加载配置

[返回顶部](#emmylua-配置指南)
