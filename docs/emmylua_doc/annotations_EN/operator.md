# @operator - Operator Overloading

Define operator overloads for classes, enabling custom operator behavior.

## Syntax

```lua
---@operator <operator>(<operand_types>): <return_type>
```

## Valid Operator Names

- `add` (+) - Addition
- `sub` (-) - Subtraction
- `mul` (*) - Multiplication
- `div` (/) - Division
- `idiv` (//) - Floor division
- `mod` (%) - Modulo
- `pow` (^) - Exponentiation
- `unm` (-) - Unary minus
- `band` (&) - Bitwise AND
- `bor` (|) - Bitwise OR
- `bxor` (~) - Bitwise XOR
- `bnot` (~) - Bitwise NOT
- `shl` (<<) - Bitwise left shift
- `shr` (>>) - Bitwise right shift
- `concat` (..) - Concatenation
- `len` (#) - Length
- `eq` (==) - Equality
- `lt` (<) - Less than
- `le` (<=) - Less than or equal
- `call` (...) - Call

Only operator names are accepted. Metamethods such as `__tostring` are not
operators: document them with `@field`, for example
`---@field __tostring fun(self: T): string`.

## Examples

```lua
-- Vector class with operator overloads
---@class Vector
---@field x number
---@field y number
---@operator add(Vector): Vector
---@operator sub(Vector): Vector
---@operator mul(number): Vector
---@operator div(number): Vector
---@operator unm: Vector
---@operator len: number
---@operator eq(Vector): boolean
local Vector = {}

function Vector.new(x, y)
    return setmetatable({x = x or 0, y = y or 0}, {
        __index = Vector,
        __add = Vector.__add,
        __sub = Vector.__sub,
        __mul = Vector.__mul,
        __div = Vector.__div,
        __unm = Vector.__unm,
        __len = Vector.__len,
        __eq = Vector.__eq
    })
end

function Vector:__add(other)
    return Vector.new(self.x + other.x, self.y + other.y)
end

function Vector:__sub(other)
    return Vector.new(self.x - other.x, self.y - other.y)
end

function Vector:__mul(scalar)
    return Vector.new(self.x * scalar, self.y * scalar)
end

function Vector:__div(scalar)
    return Vector.new(self.x / scalar, self.y / scalar)
end

function Vector:__unm()
    return Vector.new(-self.x, -self.y)
end

function Vector:__len()
    return math.sqrt(self.x * self.x + self.y * self.y)
end

function Vector:__eq(other)
    return self.x == other.x and self.y == other.y
end

-- Matrix class with operator overloads
---@class Matrix
---@field data number[][]
---@operator add(Matrix): Matrix
---@operator sub(Matrix): Matrix
---@operator mul(Matrix): Matrix
---@operator mul(number): Matrix
local Matrix = {}

function Matrix.new(data)
    return setmetatable({data = data}, {
        __index = Matrix,
        __add = Matrix.__add,
        __sub = Matrix.__sub,
        __mul = Matrix.__mul
    })
end

function Matrix:__add(other)
    local result = {}
    for i = 1, #self.data do
        result[i] = {}
        for j = 1, #self.data[i] do
            result[i][j] = self.data[i][j] + other.data[i][j]
        end
    end
    return Matrix.new(result)
end

function Matrix:__mul(other)
    if type(other) == "number" then
        -- Scalar multiplication
        local result = {}
        for i = 1, #self.data do
            result[i] = {}
            for j = 1, #self.data[i] do
                result[i][j] = self.data[i][j] * other
            end
        end
        return Matrix.new(result)
    else
        -- Matrix multiplication
        local result = {}
        for i = 1, #self.data do
            result[i] = {}
            for j = 1, #other.data[1] do
                result[i][j] = 0
                for k = 1, #other.data do
                    result[i][j] = result[i][j] + self.data[i][k] * other.data[k][j]
                end
            end
        end
        return Matrix.new(result)
    end
end

-- Complex number class
---@class Complex
---@field real number
---@field imag number
---@operator add(Complex): Complex
---@operator sub(Complex): Complex
---@operator mul(Complex): Complex
---@operator div(Complex): Complex
---@operator unm: Complex
---@operator eq(Complex): boolean
local Complex = {}

function Complex.new(real, imag)
    return setmetatable({real = real or 0, imag = imag or 0}, {
        __index = Complex,
        __add = Complex.__add,
        __sub = Complex.__sub,
        __mul = Complex.__mul,
        __div = Complex.__div,
        __unm = Complex.__unm,
        __eq = Complex.__eq,
        __tostring = Complex.__tostring
    })
end

function Complex:__add(other)
    return Complex.new(self.real + other.real, self.imag + other.imag)
end

function Complex:__mul(other)
    return Complex.new(
        self.real * other.real - self.imag * other.imag,
        self.real * other.imag + self.imag * other.real
    )
end

function Complex:__eq(other)
    return self.real == other.real and self.imag == other.imag
end

function Complex:__tostring()
    if self.imag >= 0 then
        return string.format("%.2f + %.2fi", self.real, self.imag)
    else
        return string.format("%.2f - %.2fi", self.real, -self.imag)
    end
end

-- Usage examples
local v1 = Vector.new(1, 2)
local v2 = Vector.new(3, 4)

local v3 = v1 + v2        -- Vector addition: (4, 6)
local v4 = v2 - v1        -- Vector subtraction: (2, 2)
local v5 = v1 * 2         -- Scalar multiplication: (2, 4)
local v6 = -v1            -- Unary minus: (-1, -2)
local len = #v1           -- Vector length: ~2.24
local equal = v1 == v2    -- Equality check: false

local c1 = Complex.new(1, 2)
local c2 = Complex.new(3, 4)
local c3 = c1 + c2        -- Complex addition: 4 + 6i
local c4 = c1 * c2        -- Complex multiplication: -5 + 10i
```

## Features

1. **Custom operator behavior**
2. **Type-safe operations**
3. **Multiple operand types**
4. **Mathematical operations**
5. **Comparison operations**
