# Flow typing

EmmyLua uses
[TypeScript-like control-flow analysis](https://www.typescriptlang.org/docs/handbook/2/narrowing.html).
Assignments and guards refine types at reads; stable types govern assignments.

## Type model

A variable or field has two types:

- **Stable type**: The type that determines which values can be assigned,
  supplied by an annotation or inferred from the code.
- **Flow type**: The type used at a read, based on assignments and guards on
  paths that reach it.

```lua
---@type string | integer
local value = 1 -- Stable type: string | integer; flow type: 1.
value = 'hello' -- Stable type unchanged; flow type: 'hello'.
value = true    -- Error: boolean is outside the stable type.
```

## Variables and control flow

### Stable type inference

Without an annotation, the stable type is inferred by widening the defining
value's flow type. Widening replaces primitive literal types with their base
types:

| Literal type | Widened type |
| --- | --- |
| Integer, such as `1` | `integer` |
| Float, such as `1.5` | `number` |
| String, such as `'hello'` | `string` |
| `true` or `false` | `boolean` |
| `nil` | `nil` (unchanged) |

```lua
local count = 1 -- Stable type: integer; flow type: 1.
count = 2       -- Stable type: integer; flow type: 2.
count = 'two'   -- Error: string is outside the stable type.
```

Explicit annotations keep their declared types: `---@type 1` permits only `1`.

### Nil placeholders

An unannotated local initialized to nil, or without an initializer, defers
stable type inference. On each path, the first assignment with a type other
than nil establishes its widened type plus nil. Until then, reads have flow
type nil:

```lua
local value -- Equivalent to local value = nil.
value = 'hello' -- Stable type: string | nil; flow type: 'hello'.
value = nil     -- Flow type: nil; stable type unchanged.
value = 1       -- Error: integer is outside the stable type.
```

Independent first assignments on separate branches join their stable types,
as for [inferred fields](#branch-joins-and-field-presence). Within each branch,
later assignments must fit the stable type established on that path:

```lua
local value
if flag then
    value = 1
else
    value = 'hello'
end
-- Stable type: integer | string | nil; flow type: 1 | 'hello'.
```

Nil placeholders also defer inference for fields, whether initialized in a
constructor or by a direct assignment such as `value.text = nil`:

```lua
local value = {text = nil}
value.text = 'hello' -- Stable type: string | nil; flow type: 'hello'.
```

An explicit `@type nil` establishes a nil-only stable type immediately.

### Assignments and guards

Assignments replace the flow type with the assigned type, preserving literals
and unions of literals. Self-dependent arithmetic such as `count = count + 1`
uses a base flow type.

<!-- Base flow types keep self-dependent arithmetic from producing an
unbounded sequence of literal types during loop analysis. -->

Guards narrow flow types on paths where their conditions hold:

| Guard | Examples |
| --- | --- |
| Truthiness | `if value then`, `if not value then`, `assert(value)` |
| Nil and literal comparisons | `value ~= nil`, `value == 'ok'` |
| Runtime type checks | `type(value) == 'string'` |
| Boolean combinations | Conditions joined with `and`, `or`, and `not` |

```lua
---@param value string | integer | nil
local function describe(value)
    if value == nil then
        return 'missing'
    end
    if type(value) == 'string' then
        return value:upper() -- value has flow type string.
    end
    return tostring(value) -- value has flow type integer.
end
```

A read after a branch or loop joins types from paths that reach it. Paths that
return early do not contribute.

A guard saved in a local boolean can still narrow the tested value. Copying
that boolean does not preserve the narrowing.

## Tables

### Constructors

Fresh constructors are checked from their supplied contents, with precise
literal values. An inferred table keeps its structure, with primitive field
literals widened for its stable type:

```lua
---@type {text: 'ok'}
local valid = {text = 'ok'}
---@type {text: 'ok'}
local invalid = {text = 'bad'} -- Error: 'bad' does not match 'ok'.

local inferred = {text = 'ok'} -- Stable type: {text: string}.
inferred.text = 'updated'     -- Allowed: the field accepts strings.
---@type {text: 'ok'}
local exact = inferred        -- Error: the stable field type is string.
```

Later writes cannot make an invalid constructor pass its original check.

### Declared tables

A local's `@type` or an initializer's `@as` supplies field types and index
signatures. Writes through that reference must fit them and cannot infer new
members:

```lua
---@type {text?: string}
local declared = {}
declared.text = 'hello'
declared.count = 1 -- Error: the declared type has no count field.

local asserted = {} --[[@as {text?: string}]]
asserted.count = 1 -- Error: the asserted type has no count field.
```

Parameters and imported module interfaces follow the same rule. Giving another
reference a declared type does not stop the original inferred binding from
acquiring fields.

### Field definitions

Lua records and modules are often built over several statements. Unlike a
scalar's stable type, an inferred table's field set is not fixed by its
initializer:

```lua
local module = {}
module.name = 'greeter'
function module.greet()
    return 'hello'
end
return module
```

A local initialized with a table constructor and no declared table type infers
members from the constructor and direct writes in its declaring function or
chunk, including branches and loops. An empty constructor supplies no fields.

Writes through aliases, helper parameters, or nested paths do not add members
to that binding. Captured tables follow the
[function rules](#functions-and-captured-tables).

Literal string or integer keys name fields: `value.text` and `value['text']`
name the same field; `value[1]` names an integer field. Reads do not define
fields. Reading, aliasing, passing, returning, or capturing the table does not
prevent later definitions. Use a declared type to restrict the field set and
catch misspelled definitions.

### Stable field types

The first definition on a path establishes the field's stable type, using
`@type` or [stable type inference](#stable-type-inference). Constructor fields
count as definitions; [nil placeholders](#nil-placeholders) defer inference.
Each definition checks its right-hand side before supplying the field.

```lua
local value = {}
value.result = 1     -- Stable type: integer; flow type: 1.
value.result = 2     -- Stable type unchanged; flow type: 2.
value.result = 'one' -- Error: string is outside the stable type.
value.result = nil   -- Error: nil is outside the stable type.
```

An `@type` on the first definition can supply a wider stable type. A later
annotation must denote the same type, though its spelling may differ:

```lua
local value = {}
---@type integer | string
value.result = 1
value.result = 'one' -- Allowed by the declared type.

---@type integer | string
value.result = 2 -- Allowed: repeats the type.
---@type integer
value.result = 3 -- Error: changes the stable type.
```

### Branch joins and field presence

Independent first definitions on separate branches join their stable types.
A field is required only when non-nil on every reaching path; otherwise it is
optional and its read type includes nil:

```lua
local value = {}
if numeric then
    value.result = 1
else
    value.result = 'one'
end
-- Stable type: integer | string; result is required.
value.result = 2    -- Allowed by the joined type.
value.result = true -- Error: boolean is outside the joined type.
```

A stable type established before the branch constrains writes on every branch.
Successive writes do not infer unions.

An annotation on a branch's first definition applies only to that path. The
join includes its full type, even alternatives not assigned on that branch:

```lua
local value = {}
if flag then
    ---@type integer | boolean
    value.result = 1
else
    value.result = 'one'
end
-- Stable type: integer | boolean | string.
value.result = false -- Allowed by the joined type.
```

A path with no definition contributes nil to the stable type:

```lua
local value = {}
if flag then
    value.result = 1
end
-- Stable type: integer | nil; result is optional.
value.result = 2     -- Flow type: 2; result is now required.
value.result = nil   -- Flow type: nil; stable type unchanged.
value.result = 'one' -- Error: assigning nil did not restart inference.
```

Assignments update presence without changing the stable type.

### Loops and labels

Loop bodies use the same first-definition and branch-join rules. Repeated
execution does not widen an established type:

```lua
local value = {}
for i = 1, 3 do
    if i == 1 then
        value.result = 1
    else
        value.result = 'done'
    end
    -- Stable type: integer | string.
end
```

A field defined only in a loop that may not execute is optional. Named labels
join their incoming paths; backward `goto` jumps follow the loop rules:

```lua
local value = {}
if flag then
    value.result = 1
    goto done
end
value.result = 'done'

::done::
-- Stable type: integer | string.
```

Unresolved first definitions or joins require an explicit table type or `@type`
on the first field definition. Uncertain presence stays optional and does not
widen an established type.

### Computed keys

A dynamic-key definition can infer an index signature from the key type and
the widened value type:

```lua
---@param key string
---@return table<string, string>
local function build(key)
    local value = {}
    value[key] = 'hello' -- Infers [string]: string.
    return value
end
```

Index signatures follow the first-definition and branch-join rules. Later
writes must fit the signature and any narrower field types their keys could
select. All fields in its key domain must fit it. A signature does not establish
required named fields.

### Completed interfaces

An inferred table's completed stable interface joins its field contracts at
all normal exits reachable after table creation. Explicit returns and falling
off the end count, regardless of the values returned. A field absent at any
exit is optional.

Whole-table checks and aliases use this interface, including fields defined
later. They do not check initialization order. Direct field reads use the flow
type at the read:

```lua
local value = {}
---@type {text: string}
local alias = value -- Accepted: the completed interface has text: string.
value.text:upper()  -- Error: text has not been initialized on this path.
value.text = 'hello'
```

### Metatables

Static metatable information can supply field types. Flow typing does not
follow successive `setmetatable` calls or writes to `__index` to infer a changing
field set.

## Field flow

### Reads and writes

Guards and assignments refine field reads without changing stable types:

```lua
---@type {text?: string}
local value = {}
if value.text then
    value.text:upper() -- value.text has flow type string.
end
value.text = 'hello' -- Flow type: 'hello'.
```

An assignment replaces the flow type at its target and discards earlier facts
for descendant paths of that reference. Sibling paths retain their facts.
Reads then use the assigned value and the stable field types:

```lua
---@type {child: {text?: string}}
local value = {child = {text = 'hello'}}
if value.child.text then
    value.child = {}
    value.child.text:upper() -- Error: the earlier guard no longer applies.
end
```

### Whole-table checks

Assignments, arguments, returns, and `table<K, V>` checks use the table's flow
type to select union alternatives and stable interfaces to check members.
Inferred tables use their [completed interfaces](#completed-interfaces).
Field guards and assignments do not refine a whole-table check.

Optional fields remain optional, including nil placeholders after a non-nil
assignment. A constructor can supply the refined field read instead:

```lua
local value = {text = nil}
value.text = 'hello' -- Stable field type: string | nil; flow type: 'hello'.

---@type {text: string}
local record = value -- Error: the stable text field is still optional.
---@type {text: string}
local copy = {text = value.text} -- Accepted: supplies the field directly.
```

### Unions of table types

A field guard can select matching table alternatives:

```lua
---@alias Text {kind: 'text', text: string}
---@type Text | {kind: 'count', count: integer}
local value
if value.kind == 'text' then
    ---@type Text
    local record = value -- value has flow type Text.
else
    ---@type integer
    local count = value.count -- Only the count alternative remains.
end
```

### Array elements

Flow typing tracks whether individual elements are nil and whether an array
is empty. For a nonempty array, `values[#values]` uses its element type:

```lua
---@type string[]
local values = {}
if values[1] then
    ---@type string
    local first = values[1] -- Accepted: this element was checked.
end
if #values > 0 then
    ---@type string
    local last = values[#values] -- Accepted: the array is nonempty.
end
```

## Aliases

An unannotated local alias shares the inferred member set and stable field
types, including members defined later through the original binding. Writes
must fit an existing field or index signature. Other keys report `inject-field`:

```lua
local value = {}
local alias = value
value.text = 'hello' -- Adds the field through the original binding.
alias.text:upper()   -- Accepted: the shared stable type is string.
alias.text = 'world' -- Allowed by the shared string type.
alias.count = 1      -- inject-field: aliases cannot introduce fields.
```

### Flow facts

Each reference keeps its own presence and narrowing facts. Alias creation does
not copy them. Without a refinement, a field read uses its stable type. Writes
and guards through one reference do not refine or invalidate facts through
another. Branch joins combine each reference's facts independently:

```lua
---@type {text?: string}
local value = {}
value.text = 'hello'
local alias = value -- alias.text is string | nil; value.text is 'hello'.
if alias.text then
    alias.text:upper() -- Accepted: this reference was checked.
end
```

Calls and callbacks also leave refinements in place. Mutations through other
references can therefore make a narrowed read fail at runtime:

```lua
alias.text = nil
value.text:upper() -- Accepted using the earlier refinement; fails at runtime.
```

### Rebinding

Rebinding follows the [assignment rules](#reads-and-writes). Other references
retain their facts and do not acquire members inferred for the replacement
table:

```lua
---@type {text?: string}
local value = {}
value.text = 'hello'
local alias = value
alias.text = 'world'
value = {}
-- value.text is string | nil; alias.text still has flow type 'world'.
```

<!-- Future direction: associate field flow facts with table values so aliases
can share refinements. This requires handling uncertain targets at joins and
separating rebinding from table mutation. It is outside the current contract. -->

## Functions and captured tables

Function bodies use the [completed stable interfaces](#completed-interfaces)
of captured tables. Their own assignments and guards refine field reads.

This applies to function fields, methods, local helpers, and callbacks. Calls
use ordinary signatures. They do not pass the caller's field facts into the
body, establish field initialization, or check initialization order:

```lua
local module = {}
function module.read()
    return module.text:upper() -- text is string in the completed interface.
end
module.read() -- Accepted; fails at runtime before text is initialized.
module.text = 'hello'
return module
```

An early return contributes even when it does not return the table:

```lua
---@param skip boolean
local function build(skip)
    local value = {}
    function value.read()
        return value.text:upper() -- Error: text is optional.
    end
    if skip then
        return nil
    end
    value.text = 'hello'
    return value
end
```

A function body cannot add fields to a captured table. A field initialized only
by a function such as `setup()` needs a declared type or a definition in the
table's declaring function or chunk.

<!-- Discover member names and definition sources independently of function
body inference. Recursive values and returns must not make the interface depend
on semantic query order. -->

## Annotations

`@as` asserts an expression's type without changing the variable it reads. A
named [`@cast`](../emmylua_doc/annotations_EN/cast.md) changes the type used for
later reads and assignments of that variable or field:

```lua
---@type string | nil
local value
local asserted = value --[[@as string]] -- asserted is string.

---@type string
local before = value -- Error: @as did not narrow value.
---@cast value string
---@type string
local after = value -- Accepted: @cast applies to later reads.
```

`@cast` can also add union alternatives with `+T` or remove them with `-T`.
Function annotations describe how testing a call's result narrows other values:

| Feature | Role |
| --- | --- |
| `TypeGuard<T>` | Narrows an argument when the returned boolean is tested. |
| `@return_cast` | Narrows an argument or `self` according to the boolean result. |
| [`@return_overload`](../emmylua_doc/annotations_EN/return.md#return-overload-rows-return_overload) | A check on one return value narrows related values from the same call. |

## Related language documentation

- [TypeScript: Narrowing](https://www.typescriptlang.org/docs/handbook/2/narrowing.html)
- [Flow: Type Refinements](https://flow.org/en/docs/lang/refinements/)
- [Luau: Type Refinements](https://luau.org/types/type-refinements/)
- [Luau: Table Types](https://luau.org/types/tables/)
