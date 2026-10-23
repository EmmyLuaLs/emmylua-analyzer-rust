# Flow typing

Flow typing uses assignments and conditions to determine the type of a value at
a particular read. For example, a value declared as `string | nil` can be read as
`string` after a nil check.

EmmyLua uses
[TypeScript-like control-flow analysis](https://www.typescriptlang.org/docs/handbook/2/narrowing.html).
Assignments and guards refine types at reads; stable types govern assignments.

## Type model

The analyzer distinguishes two types for a variable or field:

- **Stable type**: The type that determines which values can be assigned.
  It comes from an annotation or, without one, from inference over the code.
- **Flow type**: The type used for a read at a specific point in the code.
  It reflects assignments and guards on the paths that can reach that read.

```lua
---@type {text?: string}
local value = {}
value.text = 'hello'

---@type string
local text = value.text -- Accepted: the field's flow type is 'hello'.

---@type {text: string}
local record = value    -- Error: value's stable type is {text?: string}.
```

The write narrows the flow type of `value.text`, but `value` keeps its
stable type `{text?: string}`. When checking `value` as a whole table,
`text` is therefore still optional.

For a fresh inferred table, [construction](#table-construction) establishes its
stable fields and index signatures. Sealing the table fixes that structure.

## Variables and control flow

### Assignments

Assigning a new value to a variable updates its flow type. Its stable type
stays the same and determines which assignments are allowed:

```lua
---@type string | integer
local value = 1 -- Stable type: string | integer.
value = 'hello' -- Allowed; flow type is now 'hello'.
value = 2       -- Allowed; flow type is now 2.
value = true    -- Error: boolean is outside the stable type.
```

Without an annotation, the stable type widens primitive literals to their base
types. This lets a mutable variable or field accept other values of the same type:

```lua
local count = 1 -- Stable type: integer; flow type: 1.
count = 2       -- Stable type: integer; flow type: 2.
count = 'two'   -- Error: string is outside the stable type.
```

| Inferred literal | Stable type |
| --- | --- |
| Integer, such as `1` | `integer` |
| Float, such as `1.5` | `number` |
| String, such as `'hello'` | `string` |
| `true` or `false` | `boolean` |
| `nil` | `nil` (unchanged) |

An explicit annotation keeps its declared type; for example, `---@type 1` permits
only `1`.

Assignment flow keeps the assigned type, including literals and unions of
literals. Self-dependent arithmetic such as `count = count + 1` uses a base flow
type, so loop analysis does not keep producing new literal types.

### Guards

A guard narrows a type based on a condition:

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
        return value:upper() -- value is string here.
    end
    return tostring(value)  -- value is integer here.
end
```

A read after a branch or loop combines types from the paths that can reach it.
Paths that return early do not contribute.

A guard saved in a local boolean can still narrow a value. Copying that boolean
to another variable does not preserve the narrowing.

## Tables

### Constructors

A fresh constructor is checked from its supplied contents, keeping literal
values precise:

```lua
---@type {text: 'ok'}
local valid = {text = 'ok'}    -- Accepted: the supplied value is 'ok'.
---@type {text: 'ok'}
local invalid = {text = 'bad'} -- Error: 'bad' does not match 'ok'.
```

Later writes cannot make an invalid constructor pass its original check.

### Inferred tables

An inferred table keeps its field names and a separate type for each field.
This structure lets the analyzer check reads and writes of individual fields.
Inferred mutable fields widen primitive literals using the same rule as variables:

```lua
local value = {text = 'ok'} -- Stable field type: string.

---@type {text: 'ok'}
local exact = value -- Error: the stable field type is wider than 'ok'.
value.text = 'updated' -- Allowed: the stable field type accepts any string.
```

The stable field information is therefore `{text: string}`. An explicit
annotation such as `{text: 'ok'}` keeps its declared literal field type.

### Table construction

Lua code often builds records and modules in several statements, starting with
an empty table and then adding fields and functions:

```lua
local module = {}
module.name = 'greeter'
function module.greet()
    return 'hello'
end
return module
```

For a scalar such as `local count = 1`, the initializer establishes the stable
type `integer`, and later assignments must match it. Applying that rule to the
table above would fix its field set at the empty constructor. Adding `name` or
`greet` would then require an explicit table annotation.

Table construction lets these initial field writes establish the stable table
type. A fresh table literal assigned directly to a new local variable without a
declared type is **under construction**. Its constructor supplies the initial
fields; an empty constructor supplies none. Direct field assignments can add
fields until the table is **sealed**, which fixes its stable table type:

```lua
local value = {}
value.text = 'hello'

---@type {text: string}
local record = value -- Seals value with the stable type {text: string}.
value.text = 'updated' -- Allowed: the existing field accepts strings.
value.count = 1        -- Error: the sealed type has no count field.
```

Literal string or integer keys, such as `value.text`, `value['text']`, and
`value[1]`, add individual fields. A field's stable type comes from its
annotation or, without one, from the first assigned value with primitive
literal widening. Later writes must match the type established on that path.

A construction write checks its right-hand side before adding its field.
Reads of existing data fields use the fields established so far and keep
construction open:

```lua
local value = {}
value.a = 1
value.b = value.a + 1 -- Allowed: reading a does not seal value.
```

A write through a dynamic key adds an index signature using the key type and
the widened value type:

```lua
---@param key string
---@return table<string, string>
local function build(key)
    local value = {}
    value[key] = 'hello' -- Infers [string]: string; construction stays open.
    return value        -- Seals the inferred dictionary type.
end
```

Later writes must match the inferred index signature. Existing fields and new
fields in its key domain must also fit the signature, and dynamic writes must
respect any narrower field types their keys could select. An index signature
does not establish any required named fields.

### Conditional construction

Entering a branch, loop, or `do` block does not seal the table. Where paths
rejoin, a field is required if it is present on every path reaching that point.
If it is absent on any of those paths, its inferred stable type includes nil:

```lua
---@param include_text boolean
local function build(include_text)
    local value = {}
    if include_text then
        value.text = 'hello'
    end

    ---@type {text?: string}
    local record = value -- Accepted: stable type is {text?: string}.
    ---@type {text: string}
    local required = value -- Error: text is not present on every path.
    return value
end
```

Assigning the field on every branch makes it required. If those branches
introduce it with different types, its stable type at the join is their union:

```lua
---@param numeric boolean
---@return {result: integer | string}
local function build(numeric)
    local value = {}
    if numeric then
        value.result = 1
    else
        value.result = 'one'
    end
    return value -- Accepted: stable type is {result: integer | string}.
end
```

A field established before the branch keeps its stable type; assignments on
each path must match it.

Fields introduced only in a loop that may execute zero times are optional.
Paths that return or otherwise cannot reach a join do not contribute to it.

Sealing inside a branch uses the fields established on paths reaching that
operation. Construction can continue after a join only if it is still open on
every path reaching the join.

### Sealing boundaries

A table is sealed in the following cases. Sealing happens before the operation
is checked:

- **Declare or assert the table type.** A `@type` annotation on the local or an
  `@as` assertion on its initializer gives it a sealed type from initialization.
  Field assignments use its declared fields or index signature.

  ```lua
  ---@type {text?: string}
  local declared = {}
  declared.text = 'hello' -- Allowed: text is a declared optional field.
  declared.count = 1      -- Error: the declared type has no count field.

  local asserted = {} --[[@as {text?: string}]]
  asserted.text = 'hello' -- Allowed: text is in the asserted type.
  asserted.count = 1      -- Error: the asserted type has no count field.
  ```

  <!-- An explicit type supplies the field contract before writes begin.
  Construction must not add fields to that declared or asserted contract. -->

- **Use the table as a whole.** Aliasing, arguments, returns, storage elsewhere,
  and `#value` seal the table. Later writes cannot satisfy an earlier use:

  ```lua
  local value = {}
  ---@type {text: string}
  local record = value -- Error: value seals with no text field.
  value.text = 'hello' -- Error: the sealed type has no text field.
  ```

  <!-- Whole-table uses fix the contract before a type check or before another
  reference can observe the table. Later writes cannot supply fields for an
  earlier use. Including #value is a conservative choice that keeps construction
  separate from array-length inference. -->

- **Read a function field.** Retrieving a function, calling it, or making a
  method call seals its table. Defining the function field keeps construction
  open, even when its body captures the table.

  ```lua
  local value = {}
  function value.read()
      return value.text
  end                     -- Definition keeps construction open.
  value.text = 'hello'    -- Allowed: construction is still open.
  local read = value.read -- Seals value before read can be called.
  value.count = 1         -- Error: the sealed type has no count field.
  ```

  <!-- Direct function fields are exempt from sealing on closure capture so
  their bodies can refer to fields added later during construction. Sealing
  when the function is read fixes the field set before that function can run,
  including when it is extracted into another variable. -->

- **Reach unstructured control flow.** A `goto` or label seals the table.

  ```lua
  local value = {}
  value.text = 'hello'
  goto done       -- Seals value before the jump.
  ::done::        -- A label is also a sealing boundary.
  value.count = 1 -- Error: the sealed type has no count field.
  ```

  <!-- Jumps can skip initialization or revisit earlier construction steps.
  Sealing at goto and labels limits construction to structured control flow. -->

- **Write through a nested path.** The write seals the table, then uses its
  existing field types or index signatures.

  ```lua
  local value = {child = {text = 'hello'}}
  value.child.text = 'updated' -- Seals value; the existing field accepts strings.
  value.count = 1              -- Error: the sealed type has no count field.
  ```

  <!-- Construction tracks writes directly to one fresh table. Following a
  nested path requires tracking the ownership of other table values. -->

- **Capture the table in a closure stored elsewhere.** Direct function fields
  are exempt, as shown above.

  ```lua
  local value = {}
  value.text = 'hello'
  local function read() -- Capturing value seals it.
      return value.text
  end
  value.count = 1       -- Error: the sealed type has no count field.
  ```

  <!-- A closure stored elsewhere can run before later fields are initialized.
  Sealing at capture checks its body against fields available at that point. -->

- **Replace the local binding or apply a named `@cast`.** Sealing happens before
  the assignment or cast. Reassignment does not start a new construction phase.

  ```lua
  local replaced = {text = 'hello'}
  replaced = {text = 'updated'} -- Seals replaced; the new table matches its type.

  local casted = {}
  casted.text = 'hello'
  ---@cast casted {text: string}
  -- The cast seals casted before applying the explicit type.
  casted.text = 1               -- Error: the field's type is string.
  ```

  <!-- Rebinding can select a different table, whose fields must not be
  combined with those of the original constructor. A named @cast supplies an
  explicit type adjustment, ending construction before it is applied. -->

- **Leave the local's declaring scope.** Returning the table seals it as a
  whole-table use. If the scope ends with construction still open, the table
  seals there and pending function bodies are checked.

  ```lua
  local function build()
      local value = {}
      value.text = 'hello'
      return value -- Returns the sealed type {text: string}.
  end
  ```

  <!-- Deferred function bodies still need checking when the table is unused.
  Scope end supplies a final field set without requiring an escaping use. -->

### Function fields

A direct function field may have an unresolved return type during construction.
Sealing fixes the table's fields and index signatures before its function
signatures are resolved. Function bodies are checked against that fixed structure.

This applies to `value.read = function() ... end`,
`function value.read() ... end`, and `function value:read() ... end`. The body
can therefore refer to fields added between its definition and sealing:

```lua
local module = {}
function module.read()
    return module.text -- Checked against text: string in the sealed type.
end
module.text = 'hello'
return module -- Seals with both read and text fields.
```

### Field reads

Guards and direct assignments narrow a field's flow type while its stable type
stays the same:

```lua
---@type {text?: string}
local value
if value.text then
    ---@type string
    local text = value.text -- Accepted: the field's flow type is string.
end

value.text = 'hello'
---@type 'hello'
local text = value.text -- Accepted: the field's flow type is 'hello'.
```

A later write replaces the field's previous flow type. Reassigning the table
variable discards narrowing of its old fields.

### Whole-table checks

Whole-table checks use the table's flow type and the stable type of each field.
Narrowing a field read therefore leaves an optional field optional in the table
type. This applies to assignments, arguments, returns, and `table<K, V>` checks.

A new constructor can use the narrowed read to supply a required field:

```lua
---@type {text?: string}
local value = {}
value.text = 'hello'

---@type {text: string}
local record = value -- Error: text is still optional in the table type.

---@type {text: string}
local copy = {text = value.text} -- Accepted: the constructor supplies text.
```

### Unions of table types

A field guard can narrow the table's flow type to matching alternatives in a
union:

```lua
---@alias Text {kind: 'text', text: string}
---@type Text | {kind: 'count', count: integer}
local value
if value.kind == 'text' then
    ---@type Text
    local record = value -- Accepted: value's flow type is Text.
else
    ---@type integer
    local count = value.count -- Accepted: only the count alternative remains.
end
```

## Annotations

`@as` asserts an expression's type without changing the type of the variable it
reads. A named [`@cast`](../emmylua_doc/annotations_EN/cast.md) changes the type
used for later reads and assignments of the named variable or field:

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

A named `@cast` can also add types to a union with `+T` or remove them with `-T`.

Function annotations describe how testing a call's result narrows other values:

| Feature | Role |
| --- | --- |
| `TypeGuard<T>` | Narrows a call argument when the returned boolean is tested. |
| `@return_cast` | Narrows a specified argument or `self` according to the boolean result. |
| [`@return_overload`](../emmylua_doc/annotations_EN/return.md#return-overload-rows-return_overload) | A check on one return value narrows related values from the same call. |

## Array element reads

Flow typing tracks whether individual array elements are nil and whether an
array is empty. For a nonempty array, `values[#values]` uses the array's element
type:

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

## Scope limits

Flow typing follows assignments and guards for variables and field paths.
The following limits apply when those paths refer to mutable tables:

- **Field narrowing and table structure.** Narrowing `value.text` changes
  reads of that field. [Whole-table checks](#whole-table-checks) still use its
  stable field type: assigning a string to an optional field does not make
  that field required in the table type.

- **Construction through aliases or helpers.** Creating an alias or passing
  the table to a helper such as `fill(value)` seals it first. Writes through
  those references must fit the sealed stable type; they cannot add inferred
  fields to it.

- **Mutations through other references.** The analyzer does not trace every
  alias or determine every field that a call or callback might change.
  For example, `clear(value)` may change `value.text` at runtime while its
  earlier flow narrowing remains in place.

- **Metatable changes.** Static metatable information can supply field types.
  Flow typing does not follow successive `setmetatable` calls or writes to
  `__index` to infer a different field set at each point in execution.

## Related language documentation

- [TypeScript: Narrowing](https://www.typescriptlang.org/docs/handbook/2/narrowing.html)
- [Flow: Type Refinements](https://flow.org/en/docs/lang/refinements/)
- [Luau: Type Refinements](https://luau.org/types/type-refinements/)
- [Luau: Table Types](https://luau.org/types/tables/)
