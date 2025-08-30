# Configuration

EmmyLua loads settings from `.emmyrc.json` located in project's root.
For settings that aren't listed in project's `.emmyrc.json`, EmmyLua will search
for user-specific defaults.

:::{seealso}
  Full list of configuration values
  [is available in the reference section](../reference/configuration.md#full-list-of-config-values).
:::

## Compatibility with LuaLs

If your project uses LuaLs, EmmyLua will be able to extract some settings
from `.luarc.json`. However, `.emmyrc.json` configuration format is more
feature-rich, and incompatible parts will be automatically ignored.

## Project settings

Create an `.emmyrc.json` in the root directory of your project. At the minimum,
you'll want to configure Lua runtime version:

```json
{
  "runtime": {
    "version": "Lua5.4"
  }
}
```

To kick-start configuration process you can use this template. It contains
all project-specific settings along with their default values:

:::{spoiler} A template for project-specific `.emmyrc.json`
  :collapsible:

  ```{emmyrc:auto-example} EmmyRc
  :kind: project
  ```
:::

:::{tip}
  Only add project-specific setting to your project's `.emmyrc.json`. For example,
  it's a good idea to set up [Lua version](#EmmyRc.runtime.version)
  or [naming convention for auto-required paths](#EmmyRc.completion.autoRequireNamingConvention),
  but overriding [settings for inlay hints](#EmmyRc.hint) would serve
  no purpose.
:::

## User settings

You can override default settings by creating an `.emmyrc.json` in your home
or config directory.

:::{seealso}
  Full list of places where EmmyLua searches for configs
  [is available in the reference section](../reference/configuration.md#loading-order).
:::

Additionally, some editors allow overriding project's config values.
See [installation instructions](installation) for details.

To kick-start configuration process you can use this template. It contains
all user-specific settings along with their default values:

:::{spoiler} A template for user-specific `.emmyrc.json`
  :collapsible:

  ```{emmyrc:auto-example} EmmyRc
  :kind: user
  ```
:::

## Schema

To enable intelligent completion and validation for configuration files,
you can add a schema reference to your configuration file:

```json
{
  "$schema": "https://raw.githubusercontent.com/EmmyLuaLs/emmylua-analyzer-rust/refs/heads/main/crates/emmylua_code_analysis/resources/schema.json"
}
```
