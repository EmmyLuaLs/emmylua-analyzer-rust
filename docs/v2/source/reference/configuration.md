# Configuration

:::{seealso}
  See our guide on [how to configure EmmyLua](../guide/configuration.md).
:::

## Loading order

EmmyLua will search for configs in the following order. Values from configs
higher in the list override values from configs lower in the list:

1. local `.emmyrc.json`: a JSON file located in your project's root,
2. local `.luarc.json`: a JSON file located in your project's root,
3. a JSON file specified by environment variable `$EMMYLUALS_CONFIG`,
4. global `<config-dir>/emmylua_ls/.emmyrc.json`,
5. global `<config-dir>/emmylua_ls/.luarc.json`,
6. global `<home-dir>/.emmyrc.json`,
7. global `<home-dir>/.luarc.json`.

Depending on your platform, `<config-dir>` will be different:

| Platform | Value                                 | Example                                    |
|----------|---------------------------------------|--------------------------------------------|
| Linux    | `$XDG_CONFIG_HOME` or `$HOME/.config` | `/home/alice/.config`                      |
| macOS    | `$HOME/Library/Application Support`   | `/Users/Alice/Library/Application Support` |
| Windows  | `{FOLDERID_RoamingAppData}`           | `C:\Users\Alice\AppData\Roaming`           |

## Full list of config values

```{emmyrc:auto} EmmyRc
:recursive:
:unwrap:
:hide-prefix:
:exclude: EmmyRc $schema, EmmyRc#/$defs/DiagnosticCode
```
