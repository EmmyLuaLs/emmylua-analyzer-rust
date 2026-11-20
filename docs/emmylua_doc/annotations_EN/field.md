# @field - Field Definition

Define fields for classes, supporting access control, optionality, and index signatures.

## Syntax

```lua
-- Named field
---@field [<access_control>] <field_name>[?] <type> [description]

-- Index signature field
---@field [<access_control>] [<key_type>] <value_type> [description]
```

## Access Control Modifiers

- `public` - Public field (default)
- `private` - Private field (only accessible within the class)
- `protected` - Protected field (accessible by class and subclasses)
- `package` - Package field (accessible within the same package)

## Examples

```lua
-- Basic field definition
---@class User
---@field id number User ID
---@field name string Username
---@field email string Email address
---@field createdAt string Creation time

-- Optional fields (using ? marker)
---@class UserProfile
---@field avatar? string Avatar URL (optional)
---@field bio? string Personal bio (optional)
---@field phone? string Phone number (optional)

-- Access control examples
---@class BankAccount
---@field public accountNumber string Account number
---@field public balance number Account balance
---@field private pin string PIN code
---@field protected accountType string Account type
---@field package internalId number Internal ID

-- Index signature fields
---@class Configuration
---@field host string Host address
---@field port number Port number
---@field [string] any Other configuration items (arbitrary string keys)

---@class ScoreBoard
---@field [string] number Mapping from student names to scores

---@class GenericContainer<T>
---@field [number] T Array index access

-- Complex field types
---@class APIResponse
---@field success boolean Whether request succeeded
---@field data table | nil Response data
---@field error string | nil Error message
---@field meta {page: number, limit: number, total: number} Metadata

-- Function type fields
---@class EventEmitter
---@field listeners table<string, fun(...)> Event listener mapping
---@field emit fun(self: EventEmitter, event: string, ...): boolean Emit event
---@field on fun(self: EventEmitter, event: string, listener: fun(...)): nil Register listener

-- Nested class fields
---@class Address
---@field street string Street
---@field city string City
---@field zipCode string ZIP code

---@class Company
---@field name string Company name
---@field headquarters Address Headquarters address
---@field branches Address[] Branch addresses list

-- Usage examples
---@type User
local user = {
    id = 1001,
    name = "John",
    email = "john@example.com",
    createdAt = "2024-01-01"
}

---@type Configuration
local config = {
    host = "localhost",
    port = 8080,
    database = "myapp",  -- Supported through index signature
    cache = true         -- Supported through index signature
}

---@type ScoreBoard
local scores = {
    ["John"] = 95,
    ["Jane"] = 87,
    ["Bob"] = 92
}
```

## Features

1. **Optional field support**
2. **Access control**
3. **Index signatures**
4. **Complex type support**
5. **Nested structures**

## Different read and write types

A property whose setter accepts a different type from its getter can declare a
complementary pair of fields:

```lua
---@class Burner
---@field (read) fuel? Fuel
---@field (write) fuel? string|Fuel

---@type Burner
local burner
burner.fuel = "coal" -- checked against string|Fuel|nil
local fuel = burner.fuel -- Fuel|nil, even after the string assignment
```

Unqualified fields continue to use one type for both operations. Each direction
may use its own optional marker and generic types. Named fields, string-literal
keys, and integer-literal keys are supported; directional index signatures and
`__newindex` modeling are not supported. Both modifiers on a single field are an
annotation error. Repeating a declaration for the same direction remains a
duplicate-field error (existing function overload rules still apply). Repeated
`(write)` function declarations accept an assigned callback matching any of the
declared setter signatures. Contextual callback inference retains the existing
overload order; ordinary, unqualified function fields are unchanged.

For a union receiver such as `A|B`, reads combine the getter types and assignments
accept any setter type contributed by either variant, matching ordinary union-field
assignment behavior. Ordinary fields in the union contribute their shared read/write
type. Narrowing the receiver to one variant also narrows the available setters.

Assignments to a paired property invalidate its previous flow narrowing and
restore its declared getter type. Later checks can narrow that type normally.
The receiver of a nested assignment is always read: `object.property.child = x`
uses the getter type of `property` and the setter type of `child`.

These annotations describe types, not access permissions. If only one direction
is supplied it is used for both operations; no read-only or write-only diagnostic
is introduced. Hover and completion documentation show both types for a pair.

A child declaration shadows the inherited property as a unit. A single-direction
override therefore supplies both the read type and the write type:

```lua
---@class Parent
---@field (read) value number
---@field (write) value string

---@class Child: Parent
---@field (read) value boolean

---@type Child
local child
child.value = true -- Child accepts boolean; Parent accepts string
local value = child.value -- boolean
```
