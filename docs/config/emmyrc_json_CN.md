<div align="center">

# 🔧 EmmyLua 配置指南

*全面掌握 EmmyLua Analyzer Rust 的配置选项*

[![Back to Main](https://img.shields.io/badge/← 返回主页-blue?style=for-the-badge)](../../README.md)

</div>

---

## 📋 概述

EmmyLua 语言服务器支持灵活的配置系统，通过配置文件可以精细控制各种功能特性。

### 📁 配置文件

<table>
<tr>
<td width="50%">

#### 📄 **主配置文件**
- **`.emmyrc.json`**: 主要配置文件
- **位置**: 项目根目录
- **优先级**: 最高

</td>
<td width="50%">

#### 🔄 **兼容性配置**
- **`.luarc.json`**: 兼容配置文件
- **自动转换**: 转换为 `.emmyrc.json` 格式
- **覆盖规则**: 被 `.emmyrc.json` 覆盖

</td>
</tr>
</table>

> **💡 注意**: `.emmyrc.json` 配置格式更加丰富，不兼容的部分会被自动忽略。

### 🛠️ Schema 支持

为了获得配置文件的智能补全和验证，可以在配置文件中添加 schema 引用：

```json
{
  "$schema": "https://raw.githubusercontent.com/EmmyLuaLs/emmylua-analyzer-rust/refs/heads/main/crates/emmylua_code_analysis/resources/schema.json"
}
```

---

## 📝 完整配置示例

以下是包含所有配置选项的完整配置文件示例：

<details>
<summary><b>点击展开完整配置</b></summary>

```json
{
    "codeAction": {
        "insertSpace": false
    },
    "codeLens": {
        "enable": true
    },
    "completion": {
        "autoRequire": true,
        "autoRequireFunction": "require",
        "autoRequireNamingConvention": "keep",
        "autoRequireSeparator": ".",
        "callSnippet": false,
        "enable": true,
        "postfix": "@"
    },
    "diagnostics": {
        "diagnosticInterval": 500,
        "disable": [],
        "enable": true,
        "enables": [],
        "globals": [],
        "globalsRegex": [],
        "severity": {}
    },
    "documentColor": {
        "enable": true
    },
    "hint": {
        "enable": true,
        "indexHint": true,
        "localHint": true,
        "overrideHint": true,
        "paramHint": true
    },
    "hover": {
        "enable": true
    },
    "references": {
        "enable": true,
        "fuzzySearch": true,
        "shortStringSearch": false
    },
    "resource": {
        "paths": []
    },
    "runtime": {
        "classDefaultCall": {
            "forceNonColon": false,
            "forceReturnSelf": false,
            "functionName": ""
        },
        "extensions": [],
        "frameworkVersions": [],
        "requireLikeFunction": [],
        "requirePattern": [],
        "version": "LuaLatest"
    },
    "semanticTokens": {
        "enable": true
    },
    "signature": {
        "detailSignatureHelper": true
    },
    "strict": {
        "arrayIndex": true,
        "docBaseConstMatchBaseType": true,
        "metaOverrideFileDefine": true,
        "requirePath": false,
        "typeCall": false
    },
    "workspace": {
        "enableReindex": false,
        "encoding": "utf-8",
        "ignoreDir": [],
        "ignoreGlobs": [],
        "library": [],
        "moduleMap": [],
        "preloadFileSize": 0,
        "reindexDuration": 5000,
        "workspaceRoots": []
    }
}
```

</details>

---

## 🎯 配置分类详解

### 💡 completion - 代码补全

<div align="center">

#### 智能补全配置，提升编码效率

</div>

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| **`enable`** | `boolean` | `true` | 🔧 启用/禁用代码补全功能 |
| **`autoRequire`** | `boolean` | `true` | 📦 自动补全 require 语句 |
| **`autoRequireFunction`** | `string` | `"require"` | ⚡ 自动补全时使用的函数名 |
| **`autoRequireNamingConvention`** | `string` | `"keep"` | 🏷️ 命名规范转换方式 |
| **`callSnippet`** | `boolean` | `false` | 🎪 启用函数调用代码片段 |
| **`postfix`** | `string` | `"@"` | 🔧 后缀补全触发符号 |

#### 🏷️ 命名规范选项

<table>
<tr>
<td width="25%">

**`keep`**  
保持原样

</td>
<td width="25%">

**`camel-case`**  
驼峰命名

</td>
<td width="25%">

**`snake-case`**  
下划线命名

</td>
<td width="25%">

**`pascal-case`**  
帕斯卡命名

</td>
</tr>
</table>

---

### 📝 signature - 函数签名

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| **`detailSignatureHelper`** | `boolean` | `false` | 📊 显示详细函数签名帮助（当前无效） |

---

### 🔍 diagnostics - 代码诊断

<div align="center">

#### 强大的静态分析和错误检测系统

</div>

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| **`disable`** | `string[]` | `[]` | ❌ 禁用的诊断消息列表 |
| **`globals`** | `string[]` | `[]` | 🌐 全局变量白名单 |
| **`globalsRegex`** | `string[]` | `[]` | 🔤 全局变量正则表达式列表 |
| **`severity`** | `object` | `{}` | ⚠️ 诊断消息严重程度配置 |
| **`enables`** | `string[]` | `[]` | ✅ 启用的诊断消息列表 |

#### 🎯 严重程度级别

<table>
<tr>
<td width="25%">

**`error`**  
🔴 错误

</td>
<td width="25%">

**`warning`**  
🟡 警告

</td>
<td width="25%">

**`information`**  
🔵 信息

</td>
<td width="25%">

**`hint`**  
💡 提示

</td>
</tr>
</table>

#### 📋 常用诊断消息示例

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

### 可用的诊断列表

| 诊断消息 | 描述 | 默认分类 |
|-----------|------|------|
| **`syntax-error`** | 语法错误 | 🔴 错误 |
| **`doc-syntax-error`** | 文档语法错误 | 🔴 错误 |
| **`type-not-found`** | 类型未找到 | 🟡 警告 |
| **`missing-return`** | 缺少返回语句 | 🟡 警告 |
| **`param-type-not-match`** | 参数类型不匹配 | 🟡 警告 |
| **`missing-parameter`** | 缺少参数 | 🟡 警告 |
| **`redundant-parameter`** | 冗余参数 | 🟡 警告 |
| **`unreachable-code`** | 不可达代码 | 💡 提示 |
| **`unused`** | 未使用的变量/函数 | 💡 提示 |
| **`undefined-global`** | 未定义的全局变量 | 🔴 错误 |
| **`deprecated`** | 已弃用的功能 | 🔵 提示 |
| **`access-invisible`** | 访问不可见成员 | 🟡 警告 |
| **`discard-returns`** | 丢弃返回值 | 🟡 警告 |
| **`undefined-field`** | 未定义的字段 | 🟡 警告 |
| **`local-const-reassign`** | 局部常量重新赋值 | 🔴 错误 |
| **`iter-variable-reassign`** | 迭代变量重新赋值 | 🟡 警告 |
| **`duplicate-type`** | 重复类型定义 | 🟡 警告 |
| **`redefined-local`** | 重新定义局部变量 | 💡 提示 |
| **`redefined-label`** | 重新定义标签 | 🟡 警告 |
| **`code-style-check`** | 代码风格检查 | 🟡 警告 |
| **`need-check-nil`** | 需要检查 nil 值 | 🟡 警告 |
| **`await-in-sync`** | 在同步代码中使用 await | 🟡 警告 |
| **`annotation-usage-error`** | 注解使用错误 | 🔴 错误 |
| **`return-type-mismatch`** | 返回类型不匹配 | 🟡 警告 |
| **`missing-return-value`** | 缺少返回值 | 🟡 警告 |
| **`redundant-return-value`** | 冗余返回值 | 🟡 警告 |
| **`undefined-doc-param`** | 文档中未定义的参数 | 🟡 警告 |
| **`duplicate-doc-field`** | 重复的文档字段 | 🟡 警告 |
| **`missing-fields`** | 缺少字段 | 🟡 警告 |
| **`inject-field`** | 注入字段 | 🟡 警告 |
| **`circle-doc-class`** | 循环文档类继承 | 🟡 警告 |
| **`incomplete-signature-doc`** | 不完整的签名文档 | 🟡 警告 |
| **`missing-global-doc`** | 缺少全局变量文档 | 🟡 警告 |
| **`assign-type-mismatch`** | 赋值类型不匹配 | 🟡 警告 |
| **`duplicate-require`** | 重复 require | 💡 提示 |
| **`non-literal-expressions-in-assert`** | assert 中使用非字面量表达式 | 🟡 警告 |
| **`unbalanced-assignments`** | 不平衡的赋值 | 🟡 警告 |
| **`unnecessary-assert`** | 不必要的 assert | 🟡 警告 |
| **`unnecessary-if`** | 不必要的 if 判断 | 🟡 警告 |
| **`duplicate-set-field`** | 重复设置字段 | 🟡 警告 |
| **`duplicate-index`** | 重复索引 | 🟡 警告 |
| **`generic-constraint-mismatch`** | 泛型约束不匹配 | 🟡 警告 |

---

### 💡 hint - 内联提示

<div align="center">

#### 智能内联提示系统，无需鼠标悬浮即可查看类型信息

</div>

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| **`enable`** | `boolean` | `true` | 🔧 启用/禁用内联提示 |
| **`paramHint`** | `boolean` | `true` | 🏷️ 显示函数参数提示 |
| **`indexHint`** | `boolean` | `true` | 📊 显示跨行索引表达式提示 |
| **`localHint`** | `boolean` | `false` | 📍 显示局部变量类型提示 |
| **`overrideHint`** | `boolean` | `true` | 🔄 显示方法重载提示 |

---

### ⚙️ runtime - 运行时环境

<div align="center">

#### 配置 Lua 运行时环境和版本特性

</div>

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| **`version`** | `string` | `"Lua5.4"` | 🚀 Lua 版本选择 |
| **`requireLikeFunction`** | `string[]` | `[]` | 📦 类似 require 的函数列表 |
| **`frameworkVersions`** | `string[]` | `[]` | 🎯 框架版本标识 |
| **`extensions`** | `string[]` | `[]` | 📄 支持的文件扩展名 |
| **`requirePattern`** | `string[]` | `[]` | 🔍 require 模式匹配规则 |

#### 🚀 支持的 Lua 版本

<table>
<tr>
<td width="20%">

**`Lua5.1`**  
经典版本

</td>
<td width="20%">

**`Lua5.2`**  
增强功能

</td>
<td width="20%">

**`Lua5.3`**  
整数支持

</td>
<td width="20%">

**`Lua5.4`**  
最新特性

</td>
<td width="20%">

**`LuaJIT`**  
高性能版本

</td>
</tr>
</table>

#### 📋 运行时配置示例

```json
{
  "runtime": {
    "version": "Lua5.4",
    "requireLikeFunction": ["import", "load"],
    "frameworkVersions": ["love2d", "openresty"],
    "extensions": [".lua", ".lua.txt"],
    "requirePattern": ["?.lua", "?/init.lua"]
  }
}
```

---

### 🏗️ workspace - 工作区配置

<div align="center">

#### 工作区和项目结构配置，支持相对路径和绝对路径

</div>

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| **`ignoreDir`** | `string[]` | `[]` | 📁 忽略的目录列表 |
| **`ignoreGlobs`** | `string[]` | `[]` | 🔍 基于 glob 模式的忽略文件 |
| **`library`** | `string[]` | `[]` | 📚 库文件目录路径 |
| **`workspaceRoots`** | `string[]` | `[]` | 🏠 工作区根目录列表 |
| **`encoding`** | `string` | `"utf-8"` | 🔤 文件编码格式 |
| **`moduleMap`** | `object[]` | `[]` | 🗺️ 模块路径映射规则 |
| **`reindexDuration`** | `number` | `5000` | ⏱️ 重新索引时间间隔（毫秒） |

#### 🗺️ 模块映射配置

模块映射用于将一个模块路径转换为另一个路径，支持正则表达式：

```json
{
  "workspace": {
    "moduleMap": [
      {
        "pattern": "^lib(.*)$",
        "replace": "script$1"
      }
    ]
  }
}
```

#### 📋 工作区配置示例

```json
{
  "workspace": {
    "ignoreDir": ["build", "dist", "node_modules"],
    "ignoreGlobs": ["*.log", "*.tmp", "test_*"],
    "library": ["/usr/local/lib/lua", "./libs"],
    "workspaceRoots": ["Assets/Scripts/Lua"],
    "encoding": "utf-8",
    "reindexDuration": 3000
  }
}
```

---

### 📁 resource - 资源路径

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| **`paths`** | `string[]` | `[]` | 🎯 资源文件根目录列表 |

> **💡 用途**: 配置资源目录可以让 EmmyLua 正确提供文件路径补全和跳转功能。

---

### 👁️ codeLens - 代码透镜

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| **`enable`** | `boolean` | `true` | 🔍 启用/禁用 CodeLens 功能 |

---

### 🔒 strict - 严格模式

<div align="center">

#### 严格模式配置，控制类型检查和代码分析的严格程度

</div>

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| **`requirePath`** | `boolean` | `false` | 📍 require 路径严格模式 |
| **`typeCall`** | `boolean` | `false` | 🎯 类型调用严格模式 |
| **`arrayIndex`** | `boolean` | `false` | 📊 数组索引严格模式 |
| **`metaOverrideFileDefine`** | `boolean` | `true` | 🔄 元定义覆盖文件定义 |

#### 🎯 严格模式说明

<table>
<tr>
<td width="50%">

**🔒 启用严格模式时**
- **require 路径**: 必须从指定根目录开始
- **类型调用**: 必须手动定义重载
- **数组索引**: 严格遵循索引规则
- **元定义**: 覆盖文件中的定义

</td>
<td width="50%">

**🔓 禁用严格模式时**
- **require 路径**: 灵活的路径解析
- **类型调用**: 返回自身类型
- **数组索引**: 宽松的索引检查
- **元定义**: 行为类似 `luals`

</td>
</tr>
</table>

---

### 👁️ hover - 悬浮提示

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| **`enable`** | `boolean` | `true` | 🖱️ 启用/禁用鼠标悬浮提示 |

---

### 🔗 references - 引用查找

| 配置项 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| **`enable`** | `boolean` | `true` | 🔍 启用/禁用引用查找功能 |
| **`fuzzy_search`** | `boolean` | `true` | 🎯 启用模糊搜索 |

---

<div align="center">

## 🎯 总结

通过合理配置 EmmyLua，您可以：

- **🎯 提升开发效率**: 智能补全和提示
- **🔍 提高代码质量**: 严格的类型检查和诊断
- **🛠️ 定制开发环境**: 适应不同项目需求
- **⚡ 优化性能**: 合理的工作区和索引配置

[⬆ 返回顶部](#-emmylua-配置指南)

</div>
