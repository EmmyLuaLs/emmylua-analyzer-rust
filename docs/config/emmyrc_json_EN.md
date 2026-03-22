# EmmyLua Configuration Guide

[中文版](./emmyrc_json_CN.md)

EmmyLua Analyzer Rust is configured via a `.emmyrc.json` file in your project root. It also supports `.luarc.json` for compatibility, but `.emmyrc.json` is recommended for full feature support.

## Schema Support

Add the following field to your config file for editor autocompletion and validation:

```json
{
  "$schema": "https://raw.githubusercontent.com/EmmyLuaLs/emmylua-analyzer-rust/refs/heads/main/crates/emmylua_code_analysis/resources/schema.json"
}
```

---

## Complete Configuration Example

<details>
<summary>Click to expand</summary>

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

## Configuration Reference

### completion — Code Completion

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enable` | `boolean` | `true` | Enable code completion |
| `autoRequire` | `boolean` | `true` | Auto-complete require statements |
| `autoRequireFunction` | `string` | `"require"` | Function name used for auto-require |
| `autoRequireNamingConvention` | `string` | `"keep"` | Naming style conversion (`keep` / `camel-case` / `snake-case` / `pascal-case`) |
| `autoRequireSeparator` | `string` | `"."` | Path separator for auto-require |
| `callSnippet` | `boolean` | `false` | Enable function call snippets |
| `postfix` | `string` | `"@"` | Postfix completion trigger |
| `baseFunctionIncludesName` | `boolean` | `true` | Include function name in base function completion |

---

### codeAction — Code Actions

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `insertSpace` | `boolean` | `false` | Add space after `---` when inserting `@diagnostic disable-next-line` |

---

### codeLens — Code Lens

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enable` | `boolean` | `true` | Enable CodeLens |

---

### diagnostics — Diagnostics

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enable` | `boolean` | `true` | Enable diagnostics |
| `disable` | `string[]` | `[]` | Disabled diagnostic rules |
| `enables` | `string[]` | `[]` | Additionally enabled diagnostic rules |
| `globals` | `string[]` | `[]` | Global variable whitelist |
| `globalsRegex` | `string[]` | `[]` | Global variable regex patterns |
| `severity` | `object` | `{}` | Custom diagnostic severity overrides |
| `diagnosticInterval` | `number` | `500` | Diagnostic refresh interval (ms) |

Severity values: `error` / `warning` / `information` / `hint`

Example:

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

#### Available Diagnostic Rules

| Rule | Description | Default Level |
|------|-------------|---------------|
| `syntax-error` | Syntax errors | error |
| `doc-syntax-error` | Documentation syntax errors | error |
| `undefined-global` | Undefined global variable | error |
| `local-const-reassign` | Local constant reassignment | error |
| `annotation-usage-error` | Annotation usage error | error |
| `type-not-found` | Type not found | warning |
| `missing-return` | Missing return statement | warning |
| `param-type-not-match` | Parameter type mismatch | warning |
| `missing-parameter` | Missing parameter | warning |
| `redundant-parameter` | Redundant parameter | warning |
| `access-invisible` | Access to invisible member | warning |
| `discard-returns` | Discarded return values | warning |
| `undefined-field` | Undefined field | warning |
| `iter-variable-reassign` | Iterator variable reassignment | warning |
| `duplicate-type` | Duplicate type definition | warning |
| `redefined-label` | Redefined label | warning |
| `code-style-check` | Code style check | warning |
| `need-check-nil` | Need nil check | warning |
| `await-in-sync` | Using await in synchronous code | warning |
| `return-type-mismatch` | Return type mismatch | warning |
| `missing-return-value` | Missing return value | warning |
| `redundant-return-value` | Redundant return value | warning |
| `undefined-doc-param` | Undefined parameter in documentation | warning |
| `duplicate-doc-field` | Duplicate documentation field | warning |
| `missing-fields` | Missing fields | warning |
| `inject-field` | Inject field | warning |
| `circle-doc-class` | Circular class inheritance | warning |
| `incomplete-signature-doc` | Incomplete signature documentation | warning |
| `missing-global-doc` | Missing global variable documentation | warning |
| `assign-type-mismatch` | Assignment type mismatch | warning |
| `non-literal-expressions-in-assert` | Non-literal expressions in assert | warning |
| `unbalanced-assignments` | Unbalanced assignments | warning |
| `unnecessary-assert` | Unnecessary assert | warning |
| `unnecessary-if` | Unnecessary if statement | warning |
| `duplicate-set-field` | Duplicate field assignment | warning |
| `duplicate-index` | Duplicate index | warning |
| `generic-constraint-mismatch` | Generic constraint mismatch | warning |
| `unreachable-code` | Unreachable code | hint |
| `unused` | Unused variable/function | hint |
| `deprecated` | Deprecated feature | hint |
| `redefined-local` | Redefined local variable | hint |
| `duplicate-require` | Duplicate require | hint |

---

### doc — Documentation Syntax

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `syntax` | `string` | `"md"` | Comment syntax (`md` = Markdown, `myst` = MyST) |

---

### documentColor — Document Color

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enable` | `boolean` | `true` | Enable color preview in documents |

---

### hint — Inline Hints

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enable` | `boolean` | `true` | Enable inline hints |
| `paramHint` | `boolean` | `true` | Show function parameter name hints |
| `indexHint` | `boolean` | `true` | Show cross-line index expression hints |
| `localHint` | `boolean` | `true` | Show local variable type hints |
| `overrideHint` | `boolean` | `true` | Show method override hints |
| `metaCallHint` | `boolean` | `true` | Show metatable `__call` invocation hints |

---

### hover — Hover Information

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enable` | `boolean` | `true` | Enable mouse hover information |

---

### inlineValues — Inline Values

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enable` | `boolean` | `true` | Enable inline value display during debugging |

---

### references — Reference Finding

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enable` | `boolean` | `true` | Enable reference finding |
| `fuzzySearch` | `boolean` | `true` | Enable fuzzy search |
| `shortStringSearch` | `boolean` | `false` | Enable short string search |

---

### reformat — Code Formatting

See [External Formatter Options](../external_format/external_formatter_options_EN.md)

---

### resource — Resource Paths

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `paths` | `string[]` | `[]` | Resource root directories for file path completion and navigation |

---

### runtime — Runtime Environment

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `version` | `string` | `"LuaLatest"` | Lua version (`Lua5.1` / `Lua5.2` / `Lua5.3` / `Lua5.4` / `LuaJIT` / `LuaLatest`) |
| `requireLikeFunction` | `string[]` | `[]` | Require-like function names |
| `frameworkVersions` | `string[]` | `[]` | Framework version identifiers |
| `extensions` | `string[]` | `[]` | Additional file extensions |
| `requirePattern` | `string[]` | `[]` | Require path patterns |
| `classDefaultCall` | `object` | `{}` | Class default call configuration |
| `nonstandardSymbol` | `string[]` | `[]` | Non-standard symbols (e.g. `"continue"`) |
| `special` | `object` | `{}` | Special function mapping |

Example:

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

### semanticTokens — Semantic Tokens

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enable` | `boolean` | `true` | Enable semantic tokens |

---

### signature — Function Signature

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `detailSignatureHelper` | `boolean` | `false` | Show detailed function signature help (currently inactive) |

---

### strict — Strict Mode

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `requirePath` | `boolean` | `false` | Require paths must start from specified root directories |
| `typeCall` | `boolean` | `false` | Type calls require manual overload definitions |
| `arrayIndex` | `boolean` | `false` | Strict array index checking |
| `metaOverrideFileDefine` | `boolean` | `true` | Meta definitions override file definitions |
| `docBaseConstMatchBaseType` | `boolean` | `true` | Doc base const matches base type |

> **Tip**: When strict mode is disabled, behavior is closer to `luals` lenient mode.

---

### workspace — Workspace

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `ignoreDir` | `string[]` | `[]` | Directories to ignore |
| `ignoreGlobs` | `string[]` | `[]` | Glob patterns for ignoring files |
| `library` | `string[]` | `[]` | Library directory paths |
| `workspaceRoots` | `string[]` | `[]` | Workspace root directories |
| `encoding` | `string` | `"utf-8"` | File encoding |
| `moduleMap` | `object[]` | `[]` | Module path mapping (supports regex) |
| `reindexDuration` | `number` | `5000` | Reindex interval (ms) |
| `enableReindex` | `boolean` | `false` | Enable automatic reindexing |

Module mapping example:

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

## Getting Started

1. Create `.emmyrc.json` in your project root
2. Add the `$schema` field for editor autocompletion
3. Add configuration options as needed
4. The language server will automatically reload on save

[Back to top](#emmylua-configuration-guide)
