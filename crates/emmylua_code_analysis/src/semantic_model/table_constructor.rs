//! Keys and values supplied by a table constructor, before later assignments.

use emmylua_parser::{LuaAstNode, LuaExpr, LuaIndexKey, LuaTableExpr, NumberResult};
use hashbrown::HashMap;

use super::SemanticModel;
use super::type_check::TypeCheckFailReason;
use crate::{LuaMemberKey, LuaType, VariadicType};

pub(crate) struct TableConstructor {
    fields: Vec<ConstructorField>,
    members: Result<HashMap<LuaMemberKey, usize>, TypeCheckFailReason>,
}

struct ConstructorField {
    key: Result<LuaType, TypeCheckFailReason>,
    value: Result<LuaType, TypeCheckFailReason>,
    expr: Option<LuaExpr>,
    expands: bool,
}

/// A supplied value and the expression that produced it.
pub(crate) struct ConstructorValue<'a> {
    pub typ: LuaType,
    pub expr: Option<&'a LuaExpr>,
}

impl TableConstructor {
    pub(crate) fn new(model: &SemanticModel, table: &LuaTableExpr) -> Self {
        let last_field = table.get_fields().last();
        let fields: Vec<_> = table
            .get_fields_with_keys()
            .into_iter()
            .map(|(field, key)| ConstructorField {
                expands: matches!(key, LuaIndexKey::Idx(_)) && last_field.as_ref() == Some(&field),
                key: key_type(model, key),
                value: model.require_complete(|| {
                    field.get_value_expr().map_or(LuaType::Nil, |value| {
                        model.type_of_expr_at(value.get_syntax_id(), value.get_position())
                    })
                }),
                expr: field.get_value_expr(),
            })
            .collect();
        let members =
            fields
                .iter()
                .enumerate()
                .try_fold(HashMap::new(), |mut members, (index, field)| {
                    let key = field.key.as_ref().map_err(Clone::clone)?;
                    if !field.expands {
                        if let Some(key) = member_key(key) {
                            members.insert(key, index);
                        }
                    }
                    Ok(members)
                });
        Self { fields, members }
    }

    /// None means no field supplies this key. An explicit nil stays a value so
    /// diagnostics can distinguish an absent field from an invalid field value.
    /// An unfinished key or matching value remains an error, not proof of absence.
    pub(crate) fn member_value(
        &self,
        key: &LuaMemberKey,
    ) -> Result<Option<ConstructorValue<'_>>, TypeCheckFailReason> {
        let members = self.members.as_ref().map_err(Clone::clone)?;
        if let Some(field) = self.fields.last().filter(|field| field.expands)
            && let (Ok(LuaType::IntegerConst(start)), LuaMemberKey::Integer(index)) =
                (&field.key, key)
            && let Some(offset) = index
                .checked_sub(*start)
                .and_then(|offset| usize::try_from(offset).ok())
        {
            let value = field.value.as_ref().map_err(Clone::clone)?;
            if let Some(value) = expanded_slot_type(value, offset) {
                return Ok(Some(ConstructorValue {
                    typ: value,
                    expr: field.expr.as_ref(),
                }));
            }
        }
        let Some(&index) = members.get(key) else {
            return Ok(None);
        };
        let value = self.fields[index].value.as_ref().map_err(Clone::clone)?;
        Ok(Some(ConstructorValue {
            typ: value.get_result_slot_type(0).unwrap_or(LuaType::Nil),
            expr: self.fields[index].expr.as_ref(),
        }))
    }

    pub(crate) fn field_entries(
        &self,
    ) -> impl Iterator<Item = Result<Vec<(LuaType, ConstructorValue<'_>)>, TypeCheckFailReason>> + '_
    {
        self.fields.iter().map(ConstructorField::entries)
    }
}

/// Keep absent slots out of the member set, and include nil when only some
/// return alternatives supply the requested slot.
fn expanded_slot_type(ty: &LuaType, offset: usize) -> Option<LuaType> {
    if let LuaType::Union(union) = ty {
        let mut types = Vec::new();
        let mut has_slot = false;
        for ty in union.iter() {
            let slot = expanded_slot_type(ty, offset);
            has_slot |= slot.is_some();
            types.push(slot.unwrap_or(LuaType::Nil));
        }
        return has_slot.then(|| LuaType::from_vec(types));
    }
    if let LuaType::Variadic(variadic) = ty
        && variadic.get_max_len().is_some_and(|len| offset >= len)
    {
        return None;
    }
    ty.get_result_slot_type(offset)
}

/// Parentheses do not change the fields supplied by a constructor.
pub(crate) fn table_expr(expr: &LuaExpr) -> Option<LuaTableExpr> {
    let mut expr = expr.clone();
    while let LuaExpr::ParenExpr(paren) = expr {
        expr = paren.get_expr()?;
    }
    if let LuaExpr::TableExpr(table) = expr {
        Some(table)
    } else {
        None
    }
}

pub(crate) fn static_member_key(model: &SemanticModel, key: &LuaIndexKey) -> Option<LuaMemberKey> {
    member_key(&key_type(model, key.clone()).ok()?)
}

pub(crate) fn member_key(key: &LuaType) -> Option<LuaMemberKey> {
    match key {
        LuaType::StringConst(name) | LuaType::DocStringConst(name) => {
            Some(LuaMemberKey::Name(name.as_ref().clone()))
        }
        LuaType::IntegerConst(index) | LuaType::DocIntegerConst(index) => {
            Some(LuaMemberKey::Integer(*index))
        }
        _ => None,
    }
}

pub(crate) fn key_type(
    model: &SemanticModel,
    key: LuaIndexKey,
) -> Result<LuaType, TypeCheckFailReason> {
    let key = match key {
        LuaIndexKey::Name(name) => {
            LuaType::StringConst(smol_str::SmolStr::new(name.get_name_text()).into())
        }
        LuaIndexKey::String(string) => {
            LuaType::StringConst(smol_str::SmolStr::new(string.get_value()).into())
        }
        LuaIndexKey::Idx(index) => LuaType::IntegerConst(index as i64),
        LuaIndexKey::Integer(number) => match number.get_number_value() {
            NumberResult::Int(value) => LuaType::IntegerConst(value),
            NumberResult::Float(value) => LuaType::FloatConst(value),
            _ => LuaType::Number,
        },
        LuaIndexKey::Expr(expr) => model.require_complete(|| {
            model.type_of_expr_at(expr.get_syntax_id(), expr.get_position())
        })?,
    };
    Ok(normalize_key_type(key))
}

/// A key takes one result. Lua stores integral floating-point keys as integers.
pub(crate) fn normalize_key_type(key: LuaType) -> LuaType {
    let key = key.get_result_slot_type(0).unwrap_or(LuaType::Nil);
    match key {
        LuaType::FloatConst(value) => {
            crate::semantic_db::types::exact_integer(value).map_or(key, LuaType::IntegerConst)
        }
        key => key,
    }
}

impl ConstructorField {
    /// A final unkeyed field expands its result sequence. Other fields take one value.
    fn entries(&self) -> Result<Vec<(LuaType, ConstructorValue<'_>)>, TypeCheckFailReason> {
        let actual = self.value.as_ref().map_err(Clone::clone)?;
        if *actual == LuaType::Nil {
            return Ok(Vec::new());
        }
        let key = self.key.as_ref().map_err(Clone::clone)?;
        Ok(expanded_results(actual, self.expands)
            .filter(|(_, typ, _)| !typ.is_nil())
            .map(|(offset, typ, repeated)| {
                let key = if repeated {
                    LuaType::Integer
                } else if let LuaType::IntegerConst(start) = key {
                    LuaType::IntegerConst(start + offset as i64)
                } else {
                    key.clone()
                };
                (
                    key,
                    ConstructorValue {
                        typ,
                        expr: self.expr.as_ref(),
                    },
                )
            })
            .collect())
    }
}

/// Result offsets, value types, and whether the value repeats from that offset.
/// Non-final expressions take one result; a final call or vararg expands.
/// Return alternatives may supply different values at the same offset.
pub(crate) fn expanded_results(
    typ: &LuaType,
    expands: bool,
) -> impl Iterator<Item = (usize, LuaType, bool)> {
    let mut pending = vec![(0, typ, expands)];
    std::iter::from_fn(move || {
        while let Some((offset, actual, expands)) = pending.pop() {
            if expands {
                match actual {
                    LuaType::Union(union) => {
                        pending.extend(union.iter().map(|ty| (offset, ty, true)));
                        continue;
                    }
                    LuaType::Variadic(variadic) => {
                        match variadic.as_ref() {
                            VariadicType::Multi(types) => {
                                pending.extend(types.iter().enumerate().map(|(index, ty)| {
                                    (offset + index, ty, index + 1 == types.len())
                                }));
                            }
                            VariadicType::Base(LuaType::Tuple(tuple)) => {
                                pending.extend(
                                    tuple
                                        .get_types()
                                        .iter()
                                        .enumerate()
                                        .map(|(index, ty)| (offset + index, ty, false)),
                                );
                            }
                            VariadicType::Base(ty) | VariadicType::Repeat(ty) => {
                                return Some((offset, ty.clone(), true));
                            }
                        }
                        continue;
                    }
                    _ => {}
                }
            }
            return Some((
                offset,
                actual.get_result_slot_type(0).unwrap_or(LuaType::Nil),
                false,
            ));
        }
        None
    })
}
