# EmmyLua 注解文档索引

本目录包含了所有 EmmyLua 注解的详细文档和示例。

## 语法标记符号说明

在注解语法描述中使用以下标记符号：

- `<name>` - 必需的占位符，需要用实际值替换
- `[value]` - 可选项，方括号内的内容是可选的
- `[value...]` - 可选且可重复的项
- `value1 | value2` - 选择项，使用左侧或右侧的值
- `<type[|type...]>` - 类型表达式，支持联合类型
- `[(modifier)]` - 可选修饰符，如 `(exact)`, `(key)` 等
- `#` - 注释标记，后跟描述文本

## 核心注解

### 类型系统
- [`@alias`](./alias.md) - 类型别名定义
- [`@class`](./class.md) - 类定义
- [`@field`](./field.md) - 字段定义
- [`@type`](./type.md) - 类型声明
- [`@enum`](./enum.md) - 枚举定义
- [`@generic`](./generic.md) - 泛型定义

### 函数注解
- [`@param`](./param.md) - 参数定义
- [`@return`](./return.md) - 返回值定义
- `@return_overload`（见 [`@return`](./return.md)）- 关联返回元组
- [`@overload`](./overload.md) - 函数重载
- [`@async`](./async.md) - 异步函数标记
- [`@nodiscard`](./nodiscard.md) - 不可忽略返回值

### 类型操作
- [`@cast`](./cast.md) - 类型转换

### 代码质量
- [`@deprecated`](./deprecated.md) - 弃用标记
- [`@diagnostic`](./diagnostic.md) - 诊断控制

### 元数据
- [`@meta`](./meta.md) - 元数据文件
- [`@module`](./module.md) - 模块声明

### 其他注解
- [`@operator`](./operator.md) - 操作符重载
- [`@see`](./see.md) - 引用其他符号
- [`@source`](./source.md) - 源代码引用
- [`@version`](./version.md) - 版本要求

## 使用指南

### 基础用法
大部分注解都使用 `---@` 前缀，并遵循以下格式：
```lua
---@注解名 参数 描述
```

### 常用组合
```lua
-- 类定义组合
---@class User
---@field id number 用户ID
---@field name string 用户名
---@field email string 邮箱

-- 函数定义组合
---@param name string 用户名
---@param age number 年龄
---@return User 用户对象
function createUser(name, age)
    return {id = generateId(), name = name, age = age}
end

-- 泛型函数组合
---@generic T
---@param items T[] 项目列表
---@param predicate fun(item: T): boolean 过滤条件
---@return T[] 过滤后的列表
function filter(items, predicate)
    -- 实现代码
end
```

### 最佳实践

1. **类型优先**：优先定义类型，再使用类型
2. **渐进增强**：从基础注解开始，逐步添加更复杂的注解
3. **一致性**：在项目中保持注解风格的一致性
4. **文档化**：为复杂的类型和函数提供详细描述
5. **测试验证**：使用类型检查工具验证注解的正确性

### 注解分类

#### 类型定义类
- `@alias` - 简化复杂类型
- `@class` - 定义对象结构
- `@enum` - 定义枚举值
- `@generic` - 定义泛型参数

#### 函数相关类
- `@param` - 参数类型和描述
- `@return` - 返回值类型和描述
- `@overload` - 多种调用方式
- `@async` - 异步函数标记

#### 代码质量类
- `@deprecated` - 标记过时代码
- `@diagnostic` - 控制警告显示
- `@nodiscard` - 强制检查返回值

#### 工具支持类
- `@meta` - 类型定义文件
- `@cast` - 运行时类型转换

## 快速参考

| 注解 | 用途 | 示例 |
|------|------|------|
| `@alias` | 类型别名 | `---@alias StringOrNumber string \| number` |
| `@class` | 类定义 | `---@class User` |
| `@field` | 字段定义 | `---@field name string` |
| `@param` | 参数定义 | `---@param name string` |
| `@return` | 返回值定义 | `---@return boolean` |
| `@return_overload` | 关联返回元组 | `---@return_overload true, T` |
| `@type` | 类型声明 | `---@type string` |
| `@generic` | 泛型定义 | `---@generic T` |
| `@overload` | 函数重载 | `---@overload fun(x: number): number` |
| `@deprecated` | 弃用标记 | `---@deprecated 请使用新方法` |
| `@cast` | 类型转换 | `---@cast value string` |

更多详细信息请查看各个注解的专门文档。
