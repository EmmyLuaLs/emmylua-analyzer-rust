# EmmyLua Configuration Guide

[中文版](./emmyrc_json_CN.md)

EmmyLua Analyzer Rust recommends keeping project configuration in `.emmyrc.json` at the workspace root.

Supported config files:

- `.emmyrc.json`: recommended, full feature support
- `.luarc.json`: useful when migrating from existing LuaLS setups
- `.emmyrc.lua`: useful for dynamically generated config

## Quick Start

Put this minimal config in your project root:

```json
{
  "$schema": "https://raw.githubusercontent.com/EmmyLuaLs/emmylua-analyzer-rust/refs/heads/main/crates/emmylua_code_analysis/resources/schema.json",
  "runtime": {
    "version": "LuaLatest"
  },
  "workspace": {
    "workspaceRoots": ["./src"]
  }
}
```

## Schema Support

Adding `$schema` gives you:

- config completion
- field validation
- enum suggestions
- hover descriptions

```json
{
  "$schema": "https://raw.githubusercontent.com/EmmyLuaLs/emmylua-analyzer-rust/refs/heads/main/crates/emmylua_code_analysis/resources/schema.json"
}
```

## Path Rules

Paths in `workspace` and `resource` are expanded automatically when config is loaded.

| Syntax | Meaning |
| --- | --- |
| `./libs` | Relative to the workspace root |
| `libs/runtime` | Also treated as workspace-relative |
| `~/lua` | Relative to the user home directory |
| `${workspaceFolder}` or `{workspaceFolder}` | Workspace root |
| `{env:NAME}` | Environment variable `NAME` |
| `$NAME` | Environment variable `NAME` |
| `{luarocks}` | LuaRocks deploy lua directory |

Example:

```json
{
  "workspace": {
    "library": [
      "${workspaceFolder}/types",
      "{env:HOME}/.lua",
      "{luarocks}"
    ],
    "workspaceRoots": [
      "./src",
      "./test"
    ]
  }
}
```

## Recommended Template

This template is a good starting point for most Lua projects:

```json
{
  "$schema": "https://raw.githubusercontent.com/EmmyLuaLs/emmylua-analyzer-rust/refs/heads/main/crates/emmylua_code_analysis/resources/schema.json",
  "completion": {
    "autoRequire": true,
    "callSnippet": false,
    "postfix": "@"
  },
  "diagnostics": {
    "globals": [],
    "disable": ["undefined-global"]
  },
  "doc": {
    "syntax": "md"
  },
  "runtime": {
    "version": "LuaLatest",
    "requirePattern": ["?.lua", "?/init.lua"]
  }
}
```

## Full Configuration Example

> Note: the current top-level formatting key is `format`, not `reformat`.

<details>
<summary>Click to expand the full example</summary>

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
    "privateName": [],
    "knownTags": [],
    "syntax": "md"
  },
  "documentColor": {
    "enable": true
  },
  "format": {
    "externalTool": null,
    "externalToolRangeFormat": null,
    "useDiff": false
  },
  "hint": {
    "enable": true,
    "paramHint": true,
    "indexHint": true,
    "localHint": true,
    "overrideHint": true,
    "metaCallHint": true,
    "enumParamHint": false
  },
  "hover": {
    "enable": true,
    "customDetail": null
  },
  "inlineValues": {
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
    "version": "LuaLatest",
    "requireLikeFunction": [],
    "frameworkVersions": [],
    "extensions": [],
    "requirePattern": [],
    "nonstandardSymbol": [],
    "special": {}
  },
  "semanticTokens": {
    "enable": true,
    "renderDocumentationMarkup": true
  },
  "signature": {
    "detailSignatureHelper": true
  },
  "strict": {
    "requirePath": false,
    "typeCall": false,
    "arrayIndex": true,
    "metaOverrideFileDefine": true,
    "docBaseConstMatchBaseType": true,
    "requireExportGlobal": false
  },
  "workspace": {
    "ignoreDir": [],
    "ignoreGlobs": [],
    "library": [],
    "packageDirs": [],
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

## Top-Level Overview

| Section | Purpose | Common fields |
| --- | --- | --- |
| `completion` | Completion and auto-require | `autoRequire`, `callSnippet`, `postfix` |
| `diagnostics` | Diagnostic toggles, allowlists, severity overrides | `disable`, `globals`, `severity` |
| `doc` | Documentation parsing and rendering | `syntax`, `knownTags`, `privateName` |
| `runtime` | Lua version, extra syntax, require behavior | `version`, `extensions`, `requirePattern` |
| `workspace` | Roots, libraries, ignore rules | `library`, `workspaceRoots`, `ignoreGlobs` |
| `strict` | Stricter typing and visibility rules | `arrayIndex`, `requireExportGlobal` |
| `format` | External formatter integration | `externalTool`, `externalToolRangeFormat` |
| `hint` | Inlay hints | `paramHint`, `localHint`, `enumParamHint` |
| `hover` | Hover information | `enable`, `customDetail` |
| `references` | Reference search behavior | `fuzzySearch`, `shortStringSearch` |

## Configuration Reference

### completion

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `enable` | `boolean` | `true` | Enable completion |
| `autoRequire` | `boolean` | `true` | Insert require automatically for symbols from other modules |
| `autoRequireFunction` | `string` | `"require"` | Function name used for auto-require |
| `autoRequireNamingConvention` | `string` | `"keep"` | Filename conversion mode: `keep`, `snake-case`, `pascal-case`, `camel-case`, `keep-class` |
| `autoRequireSeparator` | `string` | `"."` | Separator used in auto-require paths |
| `callSnippet` | `boolean` | `false` | Include call snippets in function completion |
| `postfix` | `string` | `"@"` | Postfix completion trigger |
| `baseFunctionIncludesName` | `boolean` | `true` | Include the function name when generating base function snippets |

### diagnostics

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `enable` | `boolean` | `true` | Enable diagnostics |
| `disable` | `string[]` | `[]` | Disabled diagnostic rules |
| `enables` | `string[]` | `[]` | Additional rules to enable |
| `globals` | `string[]` | `[]` | Global variable allowlist |
| `globalsRegex` | `string[]` | `[]` | Regex allowlist for global variables |
| `severity` | `object` | `{}` | Per-rule severity overrides |
| `diagnosticInterval` | `number | null` | `500` | Delay after file changes before diagnostics run, in milliseconds |

Supported severities: `error`, `warning`, `information`, `hint`.

Example:

```json
{
  "diagnostics": {
    "disable": ["undefined-global"],
    "severity": {
      "undefined-global": "warning",
      "unused": "hint"
    },
    "enables": ["unknown-doc-tag"]
  }
}
```

<details>
<summary>Show diagnostic rules</summary>

Default `error` rules:

- `syntax-error`
- `doc-syntax-error`
- `undefined-global`
- `local-const-reassign`
- `annotation-usage-error`
- `iter-variable-reassign` (enabled by default on Lua 5.5+)

Default `hint` rules:

- `unreachable-code`
- `unused`
- `deprecated`
- `redefined-local`
- `duplicate-require`
- `preferred-local-alias`

Disabled by default:

- `code-style-check`
- `incomplete-signature-doc`
- `missing-global-doc`
- `unknown-doc-tag`
- `non-literal-expressions-in-assert`

All remaining built-in rules default to `warning`:

- `type-not-found`
- `missing-return`
- `param-type-mismatch`
- `missing-parameter`
- `redundant-parameter`
- `access-invisible`
- `discard-returns`
- `undefined-field`
- `duplicate-type`
- `redefined-label`
- `need-check-nil`
- `await-in-sync`
- `return-type-mismatch`
- `missing-return-value`
- `redundant-return-value`
- `undefined-doc-param`
- `duplicate-doc-field`
- `missing-fields`
- `inject-field`
- `circle-doc-class`
- `assign-type-mismatch`
- `unbalanced-assignments`
- `unnecessary-assert`
- `unnecessary-if`
- `duplicate-set-field`
- `duplicate-index`
- `generic-constraint-mismatch`
- `cast-type-mismatch`
- `unresolved-require`
- `require-module-not-visible`
- `enum-value-mismatch`
- `read-only`
- `global-in-non-module`
- `attribute-param-type-mismatch`
- `attribute-missing-parameter`
- `attribute-redundant-parameter`
- `invert-if`
- `call-non-callable`

</details>

### doc

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `privateName` | `string[]` | `[]` | Treat matching field names as private members, for example `m_*` |
| `knownTags` | `string[]` | `[]` | Additional documentation tags to recognize |
| `syntax` | `string` | `"md"` | Documentation syntax: `none`, `md`, `myst`, `rst` |
| `rstPrimaryDomain` | `string | null` | `null` | Primary domain for `myst` or `rst` |
| `rstDefaultRole` | `string | null` | `null` | Default role for `myst` or `rst` |

### runtime

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `version` | `string` | `"LuaLatest"` | Lua version: `Lua5.1`, `LuaJIT`, `Lua5.2`, `Lua5.3`, `Lua5.4`, `Lua5.5`, `LuaLatest` |
| `requireLikeFunction` | `string[]` | `[]` | Function names treated like require |
| `frameworkVersions` | `string[]` | `[]` | Framework version identifiers |
| `extensions` | `string[]` | `[]` | Additional file extensions treated as Lua |
| `requirePattern` | `string[]` | `[]` | Module search patterns such as `?.lua` and `?/init.lua` |
| `nonstandardSymbol` | `string[]` | `[]` | Allowed non-standard syntax symbols |
| `special` | `object` | `{}` | Special function mappings |

Supported `nonstandardSymbol` values:

- `//`
- `/**/`
- `` ` ``
- `+=`
- `-=`
- `*=`
- `/=`
- `%=`
- `^=`
- `//=`
- `|=`
- `&=`
- `<<=`
- `>>=`
- `||`
- `&&`
- `!`
- `!=`
- `continue`

Supported `special` values: `none`, `require`, `error`, `assert`, `type`, `setmetatable`.

### workspace

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `ignoreDir` | `string[]` | `[]` | Ignored directories |
| `ignoreGlobs` | `string[]` | `[]` | Glob patterns to ignore |
| `library` | `string[]` | `object[]` | `[]` | Library roots, either plain strings or objects with ignore rules |
| `packages` | `string[]` | `object[]` | `[]` | Package directories; the parent is treated as a library, but only the selected subdirectory is imported |
| `workspaceRoots` | `string[]` | `[]` | Workspace source roots |
| `preloadFileSize` | `number` | `0` | Reserved field, currently unused |
| `encoding` | `string` | `"utf-8"` | File encoding |
| `moduleMap` | `object[]` | `[]` | Module name rewrite rules |
| `reindexDuration` | `number` | `5000` | Delay before full reindex, in milliseconds |
| `enableReindex` | `boolean` | `false` | Enable full reindex after file changes |

`library` and `packages` can be either a string path or an object:

```json
{
  "workspace": {
    "library": [
      "./types",
      {
        "path": "./vendor",
        "ignoreDir": ["test"],
        "ignoreGlobs": ["**/*.spec.lua"]
      }
    ]
  }
}
```

`moduleMap` example:

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

### strict

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `requirePath` | `boolean` | `false` | Require import paths must match configured module paths |
| `typeCall` | `boolean` | `false` | Apply stricter checking to type calls |
| `arrayIndex` | `boolean` | `true` | Apply stricter array index checks |
| `metaOverrideFileDefine` | `boolean` | `true` | Meta definitions override file-local definitions |
| `docBaseConstMatchBaseType` | `boolean` | `true` | Allow base constant doc types to match base scalar types |
| `requireExportGlobal` | `boolean` | `false` | Third-party libraries must use `---@export global` before they become importable |

### format

For more details, see [External Formatter Options](../external_format/external_formatter_options_EN.md).

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `externalTool` | `object | null` | `null` | External formatter used for full-document formatting |
| `externalToolRangeFormat` | `object | null` | `null` | External formatter used for range formatting |
| `useDiff` | `boolean` | `false` | Merge formatter output through a diff step |

External tool object:

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `program` | `string` | `""` | Executable to run |
| `args` | `string[]` | `[]` | Argument list |
| `timeout` | `number` | `5000` | Timeout in milliseconds |

### Other sections

| Section | Field | Default | Description |
| --- | --- | --- | --- |
| `codeAction` | `insertSpace` | `false` | Add a space after `---` when inserting `@diagnostic disable-next-line` |
| `codeLens` | `enable` | `true` | Enable CodeLens |
| `documentColor` | `enable` | `true` | Detect color-like strings and show color previews |
| `hint` | `enable` | `true` | Master switch |
| `hint` | `paramHint` | `true` | Parameter name and parameter type hints |
| `hint` | `indexHint` | `true` | Named array index hints |
| `hint` | `localHint` | `true` | Local variable type hints |
| `hint` | `overrideHint` | `true` | Override hints |
| `hint` | `metaCallHint` | `true` | Hints for metatable `__call` dispatch |
| `hint` | `enumParamHint` | `false` | Enum literal hints |
| `hover` | `enable` | `true` | Enable hover docs |
| `hover` | `customDetail` | `null` | Custom hover detail level, typically `1` to `255` |
| `inlineValues` | `enable` | `true` | Show inline values during debugging |
| `references` | `enable` | `true` | Enable reference search |
| `references` | `fuzzySearch` | `true` | Fall back to fuzzy matching when normal search finds nothing |
| `references` | `shortStringSearch` | `false` | Also search inside short strings |
| `resource` | `paths` | `[]` | Resource roots for path completion and navigation |
| `semanticTokens` | `enable` | `true` | Enable semantic highlighting |
| `semanticTokens` | `renderDocumentationMarkup` | `true` | Render documentation markup, used together with `doc.syntax` |
| `signature` | `detailSignatureHelper` | `true` | Show detailed signature help |

## Recommendations

- Prefer `.emmyrc.json` for new projects
- When migrating from LuaLS, keep `.luarc.json` first and move to `.emmyrc.json` gradually
- Configure `workspace.library` and `workspace.workspaceRoots` early, otherwise navigation, completion, and diagnostics will be less precise
- If your team uses custom doc tags, remember to add them to `doc.knownTags`
- If you want stricter third-party library visibility, consider enabling `strict.requireExportGlobal`

[Back to top](#emmylua-configuration-guide)
