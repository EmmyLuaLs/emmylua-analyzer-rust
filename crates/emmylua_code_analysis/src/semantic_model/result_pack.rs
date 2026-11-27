//! Ordered Lua results. Each alternative keeps its fixed values and repeating tail.

use emmylua_parser::{LuaAstNode, LuaExpr, LuaLiteralToken};

use crate::{LuaType, VariadicType};

use super::{
    SemanticModel,
    type_eval::{expand_alias_head, with_expanded_alias},
};

/// In an expression list, only a final call or `...` can supply multiple values.
pub(crate) fn can_expand_results(expr: &LuaExpr) -> bool {
    matches!(expr, LuaExpr::CallExpr(_))
        || matches!(expr, LuaExpr::LiteralExpr(literal)
            if matches!(literal.get_literal(), Some(LuaLiteralToken::Dots(_))))
}

pub(crate) struct ResultPack {
    rows: Vec<ResultRow>,
}

struct ResultRow {
    fixed: Vec<LuaType>,
    repeated: Option<LuaType>,
}

impl ResultRow {
    fn get(&self, index: usize) -> Option<&LuaType> {
        self.fixed.get(index).or(self.repeated.as_ref())
    }
}

impl ResultPack {
    pub(crate) fn from_type(ty: &LuaType, alias_model: Option<&SemanticModel<'_>>) -> Self {
        let mut rows = Vec::new();
        collect_rows(ty, alias_model, &mut Vec::new(), &mut rows);
        Self { rows }
    }

    pub(crate) fn from_expr(model: &SemanticModel<'_>, expr: &LuaExpr) -> Self {
        let ty = model.type_of_expr_at(expr.get_syntax_id(), expr.get_range().start());
        let resolve_aliases = matches!(expr, LuaExpr::CallExpr(_));
        Self::from_type(&ty, resolve_aliases.then_some(model))
    }

    /// The longest fixed prefix and the maximum result count, which may be unbounded.
    pub(crate) fn bounds(&self) -> (usize, Option<usize>) {
        let mut fixed = 0;
        let mut repeats = false;
        for row in &self.rows {
            fixed = fixed.max(row.fixed.len());
            repeats |= row.repeated.is_some();
        }
        (fixed, (!repeats).then_some(fixed))
    }

    pub(crate) fn min_len(&self) -> usize {
        self.rows
            .iter()
            .map(|row| row.fixed.len())
            .min()
            .unwrap_or(0)
    }

    /// Trailing optional annotations and repeating tails can be left out.
    pub(crate) fn required_len(&self) -> usize {
        self.rows
            .iter()
            .map(|row| {
                row.fixed
                    .iter()
                    .rposition(|ty| !ty.is_optional())
                    .map_or(0, |index| index + 1)
            })
            .min()
            .unwrap_or(0)
    }

    pub(crate) fn is_single(&self, ty: &LuaType) -> bool {
        matches!(self.rows.as_slice(), [row]
            if row.repeated.is_none() && row.fixed.as_slice() == std::slice::from_ref(ty))
    }

    /// Combine the supplied values at this position. A caller may pad omitted
    /// alternatives with nil when its context requires a value.
    pub(crate) fn slot(&self, index: usize, pad_missing: bool) -> Option<LuaType> {
        if let [row] = self.rows.as_slice() {
            return row.get(index).cloned();
        }

        let mut types: Vec<_> = self
            .rows
            .iter()
            .filter_map(|row| row.get(index).cloned())
            .collect();
        if types.is_empty() {
            return None;
        }
        if pad_missing && types.len() < self.rows.len() {
            types.push(LuaType::Nil);
        }
        Some(LuaType::from_vec(types))
    }

    pub(crate) fn values_mut(&mut self) -> impl Iterator<Item = &mut LuaType> {
        self.rows
            .iter_mut()
            .flat_map(|row| row.fixed.iter_mut().chain(row.repeated.iter_mut()))
    }
}

fn collect_rows(
    ty: &LuaType,
    alias_model: Option<&SemanticModel<'_>>,
    prefix: &mut Vec<LuaType>,
    rows: &mut Vec<ResultRow>,
) {
    match ty {
        LuaType::Union(union) => {
            for alternative in union.iter() {
                collect_rows(alternative, alias_model, prefix, rows);
            }
        }
        LuaType::Variadic(pack) => match pack.as_ref() {
            VariadicType::Multi(types) => {
                let Some((tail, fixed)) = types.split_last() else {
                    rows.push(ResultRow {
                        fixed: prefix.clone(),
                        repeated: None,
                    });
                    return;
                };
                let len = prefix.len();
                // Non-final expressions contribute one slot, even if the call returns nothing.
                prefix.extend(fixed.iter().map(|ty| {
                    if ty.contain_multi_return() {
                        ResultPack::from_type(ty, alias_model)
                            .slot(0, true)
                            .unwrap_or(LuaType::Nil)
                    } else {
                        ty.clone()
                    }
                }));
                collect_rows(tail, alias_model, prefix, rows);
                prefix.truncate(len);
            }
            VariadicType::Base(LuaType::Tuple(tuple)) => {
                rows.push(ResultRow {
                    fixed: prefix.iter().chain(tuple.get_types()).cloned().collect(),
                    repeated: None,
                });
            }
            // A generic T... can be bound to another pack, including alternative rows.
            VariadicType::Base(base) if base.contain_multi_return() => {
                collect_rows(base, alias_model, prefix, rows);
            }
            VariadicType::Base(base) | VariadicType::Repeat(base) => {
                rows.push(ResultRow {
                    fixed: prefix.clone(),
                    repeated: Some(base.clone()),
                });
            }
        },
        _ => {
            // Keep the alias active while consuming its results. A pack can refer back to it.
            if let Some(model) = alias_model
                && expand_alias_head(model, ty).is_some_and(|head| head.contain_multi_return())
                && with_expanded_alias(model, ty, |head| {
                    collect_rows(head, alias_model, prefix, rows);
                })
                .is_some()
            {
                return;
            }
            rows.push(ResultRow {
                fixed: prefix.iter().chain(std::iter::once(ty)).cloned().collect(),
                repeated: None,
            });
        }
    }
}
