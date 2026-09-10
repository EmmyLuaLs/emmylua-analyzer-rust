//! 成员缓存结构原型, 入口仅由本文件测试调用, 暂不接入实际语义流程.
//! 本文件所有内容均为对 `&db` 的引用解析, 即意味着所有类型都是确定的, 不存在未解析的情况.
//! - `SemanticLocalCache` 在真实环境内应为单线程全局缓存
//! - 测试尽量使用 `VirtualWorkspace` 的 `ty` `expr_ty` `def系列` 等根据文本构造待测试类型, 不能虚空构造类型编造在实际上不可能发生的错误
//!
//! Spec:
//! - 对于复杂泛型循环继承以安全终止为目标, 允许在极端条件下最终结果不完整, 因为引入完整的防护代价过于高昂.
#![allow(dead_code)]

use std::{
    borrow::Cow, cell::OnceCell, hash::BuildHasher, iter::once, num::NonZeroUsize, rc::Rc,
    slice::Iter,
};

use flagset::{FlagSet, flags};
use hashbrown::{HashMap, HashSet, hash_map::EntryRef};
use indexmap::{
    IndexMap,
    map::{RawEntryApiV1, raw_entry_v1::RawEntryMut},
};
use rustc_hash::FxBuildHasher;

use crate::{
    DbIndex, LuaGenericType, LuaIntersectionType, LuaMemberIndexItem, LuaMemberKey, LuaMemberOwner,
    LuaType, LuaTypeDeclId, LuaUnionType, MultiLineUnionIter, TypeOps, TypeSubstitutor, UnionIter,
    instantiate_type_generic,
    semantic::type_check::{RelationFailure, probe_assignable},
};

/// 成员来源.
#[derive(Debug, Clone)]
pub enum MemberOrigin {
    /// 类型已经包含在对象或元组中.
    Direct(LuaType),
    /// 声明类型已经存入数据库, 多声明仍需按现有规则合并.
    InDb(LuaMemberIndexItem),
    /// 在共享声明来源上固定本次实例化的替换上下文.
    Generic {
        source: Rc<MemberSymbol>,
        substitutor: Rc<TypeSubstitutor>,
    },
    Union(Vec<Rc<MemberSymbol>>),
    Intersection(Vec<Rc<MemberSymbol>>),
}

/// 成员来源和按需解析的类型.
#[derive(Debug)]
pub struct MemberSymbol {
    origin: MemberOrigin,
    typ: OnceCell<Option<LuaType>>,
    /// 部分联合分支缺少属性
    is_partial: bool,
}

impl MemberSymbol {
    pub fn new(origin: MemberOrigin) -> Self {
        Self {
            origin,
            is_partial: false,
            typ: OnceCell::new(),
        }
    }

    pub fn typ<'a>(&'a self, db: &'a DbIndex) -> Option<&'a LuaType> {
        // 已有类型直接借用
        match &self.origin {
            MemberOrigin::Direct(typ) => return Some(typ),
            MemberOrigin::InDb(LuaMemberIndexItem::One(id)) => {
                return db
                    .get_type_index()
                    .get_type_cache(&(*id).into())
                    .map(|cache| cache.as_type());
            }
            _ => {}
        }

        self.typ
            .get_or_init(|| match &self.origin {
                MemberOrigin::InDb(item) => item.resolve_type(db).ok(),
                MemberOrigin::Generic {
                    source,
                    substitutor,
                } => Some(instantiate_type_generic(db, source.typ(db)?, substitutor)),
                MemberOrigin::Union(members) => {
                    let types = members
                        .iter()
                        .map(|member| member.typ(db).cloned())
                        .collect::<Option<Vec<_>>>()?;
                    Some(TypeOps::union_all(db, types))
                }
                MemberOrigin::Intersection(members) => {
                    let mut types = members.iter().map(|member| member.typ(db).cloned());
                    let Some(first) = types.next() else {
                        return Some(LuaType::Unknown);
                    };
                    let first = first?;
                    types.try_fold(first, |left, right| {
                        Some(TypeOps::Intersect.apply(db, &left, &right?))
                    })
                }
                MemberOrigin::Direct(typ) => Some(typ.clone()),
            })
            .as_ref()
    }
}

pub type MemberMap = IndexMap<LuaMemberKey, Rc<MemberSymbol>, FxBuildHasher>;

pub type PropertyList = Rc<Vec<(LuaMemberKey, Rc<MemberSymbol>)>>;

type IndexInfoMap = IndexMap<LuaType, Rc<MemberSymbol>, FxBuildHasher>;

#[derive(Debug, Default)]
struct DeclaredTypeMembers {
    properties: MemberMap,
    index_infos: IndexInfoMap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MemberCacheEntryId(NonZeroUsize);

flags! {
    pub enum MemberCacheFlag: u8 {
        /// 成员已被解析
        MembersResolved,
    }
}

#[derive(Debug, Default)]
struct MemberCacheEntry {
    flags: FlagSet<MemberCacheFlag>,
    members: MemberMap,
    property_list: Option<PropertyList>,
    /// 索引签名
    index_infos: IndexInfoMap,
    call_signatures: Option<Option<Rc<[LuaType]>>>,
    reduced_entry_id: Option<MemberCacheEntryId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TypeSystemEntity {
    Type(LuaType),
    MemberEntry(MemberCacheEntryId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TypeSystemPropertyName {
    Property(LuaMemberKey),
    PropertyList,
    ReducedType,
}

#[derive(Debug)]
struct TypeResolution {
    target: TypeSystemEntity,
    property_name: TypeSystemPropertyName,
    /// 是否未发生循环依赖.
    cycle_free: bool,
}

#[derive(Debug, Default)]
pub struct SemanticLocalCache {
    declared_type_members: HashMap<LuaTypeDeclId, DeclaredTypeMembers, FxBuildHasher>,
    type_decl_base_types: HashMap<LuaTypeDeclId, Rc<[LuaType]>, FxBuildHasher>,
    type_member_entries: IndexMap<LuaType, MemberCacheEntry, FxBuildHasher>,
    type_resolutions: Vec<TypeResolution>,
    /// 继承展开的独立深度预算, 防止泛型实参无限增长.
    ///
    /// 精确检查复杂度过高且99.99%情况下没有必要, 因此使用简单的深度限制来处理是合理的.
    inheritance_depth: usize,
}

impl SemanticLocalCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.inheritance_depth = 0;
        self.type_resolutions.clear();
        self.type_member_entries.clear();
        self.type_decl_base_types.clear();
        self.declared_type_members.clear();
    }

    fn push_type_resolution(
        &mut self,
        target: TypeSystemEntity,
        property_name: TypeSystemPropertyName,
    ) -> bool {
        if self.type_resolutions.len() >= 128 {
            return false;
        }
        for index in (0..self.type_resolutions.len()).rev() {
            if self.type_resolutions[index].target == target
                && self.type_resolutions[index].property_name == property_name
            {
                for resolution in &mut self.type_resolutions[index..] {
                    resolution.cycle_free = false;
                }
                return false;
            }
        }
        self.type_resolutions.push(TypeResolution {
            target,
            property_name,
            cycle_free: true,
        });
        true
    }

    fn pop_type_resolution(&mut self) -> bool {
        self.type_resolutions
            .pop()
            .is_some_and(|resolution| resolution.cycle_free)
    }
}

#[inline]
fn get_or_create_member_entry_id(
    cache: &mut SemanticLocalCache,
    typ: &LuaType,
) -> MemberCacheEntryId {
    // 单类型查询只预留一个大条目, 后续增长交给容器处理.
    if cache.type_member_entries.capacity() == 0 {
        cache.type_member_entries.reserve_exact(1);
    }
    let hash = cache.type_member_entries.hasher().hash_one(typ);
    let entry = cache
        .type_member_entries
        .raw_entry_mut_v1()
        .from_key_hashed_nocheck(hash, typ);
    let id = MemberCacheEntryId(NonZeroUsize::new(entry.index() + 1).expect("缓存序号不能溢出"));
    if let RawEntryMut::Vacant(entry) = entry {
        entry.insert_hashed_nocheck(hash, typ.clone(), MemberCacheEntry::default());
    }
    id
}

fn get_member_entry(cache: &SemanticLocalCache, id: MemberCacheEntryId) -> &MemberCacheEntry {
    &cache.type_member_entries[id.0.get() - 1]
}

fn get_member_entry_mut(
    cache: &mut SemanticLocalCache,
    id: MemberCacheEntryId,
) -> &mut MemberCacheEntry {
    &mut cache.type_member_entries[id.0.get() - 1]
}

fn get_member_entry_type(cache: &SemanticLocalCache, id: MemberCacheEntryId) -> &LuaType {
    cache
        .type_member_entries
        .get_index(id.0.get() - 1)
        .expect("缓存条目必须存在")
        .0
}

/// 枚举当前类型的属性, 点查使用独立入口.
pub fn get_properties_of_type(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    typ: &LuaType,
) -> Option<PropertyList> {
    let id = get_reduced_member_entry_id(db, cache, typ)??;
    get_properties_by_entry_id(db, cache, id)
}

fn get_properties_by_entry_id(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    id: MemberCacheEntryId,
) -> Option<PropertyList> {
    if let Some(properties) = &get_member_entry(cache, id).property_list {
        return Some(properties.clone());
    }
    match get_member_entry_type(cache, id) {
        LuaType::Union(_) | LuaType::MultiLineUnion(_) | LuaType::Intersection(_) => {
            resolve_union_or_intersection_properties(db, cache, id)?;
        }
        _ => {
            resolve_structured_type_members(db, cache, id)?;
            let entry = get_member_entry_mut(cache, id);
            entry.property_list = Some(
                entry
                    .members
                    .iter()
                    .map(|(key, member)| (key.clone(), member.clone()))
                    .collect::<Vec<_>>()
                    .into(),
            );
        }
    }
    get_member_entry(cache, id).property_list.clone()
}

fn resolve_structured_type_members(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    id: MemberCacheEntryId,
) -> Option<()> {
    if get_member_entry(cache, id)
        .flags
        .contains(MemberCacheFlag::MembersResolved)
    {
        return Some(());
    }
    let typ = get_member_entry_type(cache, id).clone();
    match &typ {
        LuaType::Union(_) | LuaType::MultiLineUnion(_) => {
            resolve_union_type_members(db, cache, id, &typ)
        }
        LuaType::Intersection(_) => resolve_intersection_type_members(db, cache, id, &typ),
        _ => {
            if is_object_type(&typ) {
                resolve_object_type_members(db, cache, id, &typ)
            } else {
                None
            }
        }
    };
    get_member_entry(cache, id)
        .flags
        .contains(MemberCacheFlag::MembersResolved)
        .then_some(())
}

fn set_structured_type_members(
    entry: &mut MemberCacheEntry,
    members: Option<MemberMap>,
    indexes: Option<IndexInfoMap>,
) {
    entry.flags |= MemberCacheFlag::MembersResolved;
    if let Some(mut members) = members {
        if !entry.members.is_empty() {
            for (key, member) in &mut members {
                if let Some(existing) = entry.members.get(key) {
                    *member = existing.clone();
                }
            }
        }
        entry.members = members;
        entry.property_list = None;
    }
    if let Some(indexes) = indexes {
        entry.index_infos = indexes;
    }
}

fn resolve_union_or_intersection_properties(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    id: MemberCacheEntryId,
) -> Option<()> {
    if !cache.push_type_resolution(
        TypeSystemEntity::MemberEntry(id),
        TypeSystemPropertyName::PropertyList,
    ) {
        return None;
    }
    let typ = get_member_entry_type(cache, id).clone();
    let resolved = collect_union_or_intersection_properties(db, cache, id, &typ);
    let no_cycle = cache.pop_type_resolution();
    let properties = resolved?;
    if !no_cycle {
        return None;
    }
    get_member_entry_mut(cache, id).property_list = Some(properties);
    Some(())
}

fn collect_union_or_intersection_properties(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    id: MemberCacheEntryId,
    typ: &LuaType,
) -> Option<PropertyList> {
    let is_union = matches!(typ, LuaType::Union(_) | LuaType::MultiLineUnion(_));
    let mut checked = HashSet::new();
    let mut properties = Vec::new();
    for current in get_union_or_intersection_types(typ)? {
        let current_properties = match get_properties_of_type(db, cache, &current) {
            Some(properties) => properties,
            None if is_structured_type(&current) => return None,
            None => continue,
        };
        for (key, _) in current_properties.iter() {
            if checked.insert(key.clone()) {
                if let Some(member) =
                    get_member_by_entry_id(db, cache, id, key)?.filter(|member| !member.is_partial)
                {
                    properties.push((key.clone(), member));
                }
            }
        }
        // 组合索引可能范围重叠但键不相同, 仅对象的完整空索引表允许提前停止.
        if is_union
            && is_object_type(&current)
            && get_index_infos_of_type(db, cache, &current)?.is_empty()
        {
            break;
        }
    }
    Some(properties.into())
}

/// 按固定属性键查询成员, 索引签名由索引接口解析.
/// 缺失或查询尚未完成时返回 None, 仅确定的结果写入缓存.
pub fn get_property_of_type(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    typ: &LuaType,
    key: &LuaMemberKey,
) -> Option<Rc<MemberSymbol>> {
    resolve_property_of_type(db, cache, typ, key)?
}

fn resolve_property_of_type(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    typ: &LuaType,
    key: &LuaMemberKey,
) -> Option<Option<Rc<MemberSymbol>>> {
    Some(get_member_of_type(db, cache, typ, key)?.filter(|member| !member.is_partial))
}

/// 保留部分联合成员, 外层 None 表示查询尚未完成.
fn get_member_of_type(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    typ: &LuaType,
    key: &LuaMemberKey,
) -> Option<Option<Rc<MemberSymbol>>> {
    if !matches!(key, LuaMemberKey::Name(_) | LuaMemberKey::Integer(_)) {
        return Some(None);
    }
    let Some(id) = get_reduced_member_entry_id(db, cache, typ)? else {
        return Some(None);
    };
    get_member_by_entry_id(db, cache, id, key)
}

#[inline]
fn get_member_by_entry_id(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    id: MemberCacheEntryId,
    key: &LuaMemberKey,
) -> Option<Option<Rc<MemberSymbol>>> {
    if let Some(member) = get_member_entry(cache, id).members.get(key).cloned() {
        return Some(Some(member));
    }

    // 仅支持部分类型点查, 对大部分类型来说仍然应直接构建完整的属性表
    let member = match get_member_entry_type(cache, id) {
        LuaType::Union(_) | LuaType::MultiLineUnion(_) | LuaType::Intersection(_) => {
            resolve_union_or_intersection_property(db, cache, id, key)?
        }
        LuaType::TableConst(range) => {
            if get_member_entry(cache, id)
                .flags
                .contains(MemberCacheFlag::MembersResolved)
            {
                return Some(None);
            }
            let owner = LuaMemberOwner::Element(range.clone());
            db.get_member_index()
                .get_member_item(&owner, key)
                .map(|item| Rc::new(MemberSymbol::new(MemberOrigin::InDb(item.clone()))))
        }
        _ => {
            resolve_structured_type_members(db, cache, id)?;
            return Some(get_member_entry(cache, id).members.get(key).cloned());
        }
    };
    Some(member.map(|member| {
        get_member_entry_mut(cache, id)
            .members
            .entry(key.clone())
            .or_insert(member)
            .clone()
    }))
}

fn resolve_union_or_intersection_property(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    id: MemberCacheEntryId,
    key: &LuaMemberKey,
) -> Option<Option<Rc<MemberSymbol>>> {
    if !cache.push_type_resolution(
        TypeSystemEntity::MemberEntry(id),
        TypeSystemPropertyName::Property(key.clone()),
    ) {
        return None;
    }
    let typ = get_member_entry_type(cache, id).clone();
    let member = create_union_or_intersection_property(db, cache, &typ, key);
    let no_cycle = cache.pop_type_resolution();
    let member = member?;
    if !no_cycle {
        return None;
    }
    Some(member)
}

fn create_union_or_intersection_property(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    typ: &LuaType,
    key: &LuaMemberKey,
) -> Option<Option<Rc<MemberSymbol>>> {
    let types = get_union_or_intersection_types(typ)?;
    let is_union = matches!(typ, LuaType::Union(_) | LuaType::MultiLineUnion(_));
    let mut has_property = false;
    let mut is_partial = false;
    let mut first = None;
    let mut members = IndexMap::<_, _, FxBuildHasher>::default();
    for current in types {
        if matches!(current.as_ref(), LuaType::Never) {
            continue;
        }
        let member = match resolve_property_of_type(db, cache, &current, key)? {
            Some(member) => {
                has_property = true;
                member
            }
            None if is_union => {
                match get_applicable_index_info_for_name(db, cache, &current, key)? {
                    Some(member) => member,
                    None => {
                        is_partial = true;
                        continue;
                    }
                }
            }
            None => continue,
        };
        if let Some(first) = &first {
            if Rc::ptr_eq(first, &member) {
                continue;
            }
            if members.is_empty() {
                members.insert(Rc::as_ptr(first), first.clone());
            }
            members.entry(Rc::as_ptr(&member)).or_insert(member);
        } else {
            first = Some(member);
        }
    }
    // 索引签名只补足已有属性, 不能凭空生成任意具名属性.
    if !has_property {
        return Some(None);
    }
    if members.is_empty() && !is_partial {
        return Some(first);
    }
    let members = if members.is_empty() {
        first.into_iter().collect()
    } else {
        members.into_values().collect()
    };
    let origin = if is_union {
        MemberOrigin::Union(members)
    } else {
        MemberOrigin::Intersection(members)
    };
    let mut member = MemberSymbol::new(origin);
    member.is_partial = is_partial;
    Some(Some(Rc::new(member)))
}

/// 仅返回完整解析的索引信息.
pub fn get_index_infos_of_type<'a>(
    db: &DbIndex,
    cache: &'a mut SemanticLocalCache,
    typ: &LuaType,
) -> Option<&'a IndexInfoMap> {
    let id = resolve_index_info_entry_id(db, cache, typ)?;
    Some(&get_member_entry(cache, id).index_infos)
}

fn resolve_index_info_entry_id(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    typ: &LuaType,
) -> Option<MemberCacheEntryId> {
    let id = get_reduced_member_entry_id(db, cache, typ)??;
    resolve_structured_type_members(db, cache, id)?;
    Some(id)
}

struct ApplicableIndexInfo {
    member: Rc<MemberSymbol>,
    string_index_only: bool,
}

fn get_applicable_index_info(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    typ: &LuaType,
    key_type: &LuaType,
) -> Option<Option<ApplicableIndexInfo>> {
    let Some(id) = get_reduced_member_entry_id(db, cache, typ)? else {
        return Some(None);
    };
    if matches!(
        get_member_entry_type(cache, id),
        LuaType::Union(_) | LuaType::MultiLineUnion(_) | LuaType::Intersection(_)
    ) {
        let typ = get_member_entry_type(cache, id).clone();
        let is_union = matches!(&typ, LuaType::Union(_) | LuaType::MultiLineUnion(_));
        let mut infos = Vec::new();
        let types = get_union_or_intersection_types(&typ)?;
        for current in types.filter(|current| !matches!(current.as_ref(), LuaType::Never)) {
            let Some(info) = get_applicable_index_info(db, cache, &current, key_type)? else {
                if is_union {
                    return Some(None);
                }
                continue;
            };
            infos.push(info);
        }
        let string_index_only = infos.iter().all(|info| info.string_index_only);
        let member = combine_member_symbols(
            is_union,
            infos
                .into_iter()
                .filter(|info| is_union || string_index_only || !info.string_index_only)
                .map(|info| Some(Some(info.member))),
        )?;
        return Some(member.map(|member| ApplicableIndexInfo {
            member,
            string_index_only,
        }));
    }
    resolve_structured_type_members(db, cache, id)?;
    find_applicable_index_info(db, &get_member_entry(cache, id).index_infos, key_type)
}

fn get_applicable_index_info_for_name(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    typ: &LuaType,
    key: &LuaMemberKey,
) -> Option<Option<Rc<MemberSymbol>>> {
    let Some(key_type) = key.to_index_type() else {
        return Some(None);
    };
    Some(get_applicable_index_info(db, cache, typ, &key_type)?.map(|info| info.member))
}

fn find_applicable_index_info(
    db: &DbIndex,
    indexes: &IndexInfoMap,
    key_type: &LuaType,
) -> Option<Option<ApplicableIndexInfo>> {
    let member = combine_member_symbols(
        false,
        indexes
            .iter()
            .filter(|(index_type, _)| !matches!(index_type, LuaType::String))
            .map(|(index_type, member)| {
                is_applicable_index_type(db, key_type, index_type)
                    .map(|applicable| applicable.then(|| member.clone()))
            }),
    )?;
    if let Some(member) = member {
        return Some(Some(ApplicableIndexInfo {
            member,
            string_index_only: false,
        }));
    }
    // 仅在没有其他适用索引时使用 string 索引.
    let string_index = indexes.get(&LuaType::String);
    match string_index {
        Some(member) if is_applicable_index_type(db, key_type, &LuaType::String)? => {
            Some(Some(ApplicableIndexInfo {
                member: member.clone(),
                string_index_only: true,
            }))
        }
        _ => Some(None),
    }
}

fn is_applicable_index_type(db: &DbIndex, source: &LuaType, target: &LuaType) -> Option<bool> {
    if let Some(types) = get_union_or_intersection_types(target) {
        let is_union = matches!(target, LuaType::Union(_) | LuaType::MultiLineUnion(_));
        let mut unresolved = false;
        for typ in types {
            match is_applicable_index_type(db, source, &typ) {
                Some(applicable) if applicable == is_union => return Some(is_union),
                Some(_) => {}
                None => unresolved = true,
            }
        }
        return (!unresolved).then_some(!is_union);
    }
    match (source, target) {
        (
            LuaType::IntegerConst(integer) | LuaType::DocIntegerConst(integer),
            LuaType::FloatConst(float),
        )
        | (
            LuaType::FloatConst(float),
            LuaType::IntegerConst(integer) | LuaType::DocIntegerConst(integer),
        ) => {
            return Some(float_index_to_integer(*float) == Some(*integer));
        }
        (LuaType::FloatConst(source), LuaType::FloatConst(target)) => {
            return Some(source == target);
        }
        (LuaType::FloatConst(value), LuaType::Integer) => {
            return Some(float_index_to_integer(*value).is_some());
        }
        (LuaType::Integer | LuaType::Number, LuaType::FloatConst(_)) => return Some(false),
        _ => {}
    }
    // 键中的字面量表示精确值, 不采用推断值类型的宽松字面量匹配.
    let target = match target {
        LuaType::StringConst(value) => Cow::Owned(LuaType::DocStringConst(value.clone())),
        LuaType::IntegerConst(value) => Cow::Owned(LuaType::DocIntegerConst(*value)),
        _ => Cow::Borrowed(target),
    };
    match probe_assignable(db, source, &target, None) {
        Ok(()) => Some(true),
        Err(RelationFailure::Unrelated) => Some(false),
        Err(RelationFailure::Indeterminate(_)) => None,
    }
}

fn float_index_to_integer(value: f64) -> Option<i64> {
    // 排除 2^63 上界, 避免饱和转换或整数转浮点时的舍入把不同键判为相同.
    (value.fract() == 0.0 && value >= i64::MIN as f64 && value < -(i64::MIN as f64))
        .then_some(value as i64)
}

enum ReducedType<'a> {
    Type(Cow<'a, LuaType>),
    MemberEntry(MemberCacheEntryId),
}

impl ReducedType<'_> {
    fn into_owned(self) -> ReducedType<'static> {
        match self {
            Self::Type(typ) => ReducedType::Type(Cow::Owned(typ.into_owned())),
            Self::MemberEntry(id) => ReducedType::MemberEntry(id),
        }
    }
}

/// 归约类型并定位成员缓存, 外层 None 表示未完成, 内层 None 表示非结构化类型.
#[inline]
fn get_reduced_member_entry_id(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    typ: &LuaType,
) -> Option<Option<MemberCacheEntryId>> {
    if matches!(
        typ,
        LuaType::TableConst(_)
            | LuaType::Object(_)
            | LuaType::Tuple(_)
            | LuaType::Array(_)
            | LuaType::TableGeneric(_)
    ) {
        return Some(Some(get_or_create_member_entry_id(cache, typ)));
    }
    match get_reduced_type_with_depth(db, cache, typ, 0)? {
        ReducedType::MemberEntry(id) => {
            Some(is_structured_type(get_member_entry_type(cache, id)).then_some(id))
        }
        ReducedType::Type(typ) => {
            Some(is_structured_type(&typ).then(|| get_or_create_member_entry_id(cache, &typ)))
        }
    }
}

fn get_reduced_type<'a>(
    db: &'a DbIndex,
    cache: &mut SemanticLocalCache,
    typ: &'a LuaType,
) -> Option<Cow<'a, LuaType>> {
    match get_reduced_type_with_depth(db, cache, typ, 0)? {
        ReducedType::Type(typ) => Some(typ),
        ReducedType::MemberEntry(id) => Some(Cow::Owned(get_member_entry_type(cache, id).clone())),
    }
}

fn get_reduced_type_with_depth<'a>(
    db: &'a DbIndex,
    cache: &mut SemanticLocalCache,
    typ: &'a LuaType,
    depth: u32,
) -> Option<ReducedType<'a>> {
    if depth >= 100 {
        return Some(ReducedType::Type(Cow::Borrowed(typ)));
    }
    match typ {
        LuaType::Ref(id) => {
            let type_decl = db.get_type_index().get_type_decl(id)?;
            if type_decl.is_alias() {
                return get_reduced_type_with_depth(
                    db,
                    cache,
                    type_decl.get_alias_ref()?,
                    depth + 1,
                );
            }
            Some(ReducedType::Type(Cow::Borrowed(typ)))
        }
        LuaType::Def(id) => {
            let def_as_ref = LuaType::Ref(id.clone());
            let reduced = get_reduced_type_with_depth(db, cache, &def_as_ref, depth + 1)?;
            Some(match reduced {
                ReducedType::Type(Cow::Borrowed(_)) => ReducedType::Type(Cow::Owned(def_as_ref)),
                reduced => reduced.into_owned(),
            })
        }
        LuaType::Generic(generic) => {
            let type_decl = db
                .get_type_index()
                .get_type_decl(generic.get_base_type_id_ref())?;
            if !type_decl.is_alias() {
                return Some(ReducedType::Type(Cow::Borrowed(typ)));
            }
            let substitutor = TypeSubstitutor::from_type_array(generic.get_params().clone());
            let origin = type_decl.get_alias_origin(db, Some(&substitutor))?;
            let expanded = get_reduced_type_with_depth(db, cache, &origin, depth + 1)?;
            Some(expanded.into_owned())
        }
        LuaType::Union(_) | LuaType::MultiLineUnion(_) | LuaType::Intersection(_) => {
            resolve_reduced_composite_entry_id(db, cache, typ).map(ReducedType::MemberEntry)
        }
        _ => Some(ReducedType::Type(Cow::Borrowed(typ))),
    }
}

fn combine_member_symbols(
    is_union: bool,
    branch_members: impl Iterator<Item = Option<Option<Rc<MemberSymbol>>>>,
) -> Option<Option<Rc<MemberSymbol>>> {
    let mut first = None;
    let mut members = IndexMap::<_, _, FxBuildHasher>::default();
    for member in branch_members {
        let Some(member) = member? else {
            if is_union {
                return Some(None);
            }
            continue;
        };
        if let Some(first) = &first {
            if Rc::ptr_eq(first, &member) {
                continue;
            }
            if members.is_empty() {
                members.insert(Rc::as_ptr(first), first.clone());
            }
            members.entry(Rc::as_ptr(&member)).or_insert(member);
        } else {
            first = Some(member);
        }
    }
    if members.is_empty() {
        return Some(first);
    }
    let members = members.into_values().collect();
    let origin = if is_union {
        MemberOrigin::Union(members)
    } else {
        MemberOrigin::Intersection(members)
    };
    Some(Some(Rc::new(MemberSymbol::new(origin))))
}

fn collect_declared_type_members(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    typ: &LuaType,
    properties: &mut MemberMap,
    indexes: &mut IndexInfoMap,
) -> Option<()> {
    match typ {
        LuaType::Ref(id) | LuaType::Def(id) => {
            let declared = resolve_declared_type_members(db, cache, id)?;
            properties.extend(
                declared
                    .properties
                    .iter()
                    .map(|(key, member)| (key.clone(), member.clone())),
            );
            indexes.extend(
                declared
                    .index_infos
                    .iter()
                    .map(|(key, member)| (key.clone(), member.clone())),
            );
        }
        LuaType::Generic(generic) => {
            collect_generic_declared_members(db, cache, generic, properties, indexes)?;
        }
        LuaType::TableConst(range) => {
            collect_owner_members(
                db,
                &LuaMemberOwner::Element(range.clone()),
                properties,
                indexes,
            );
        }
        LuaType::Object(object) => {
            for (key, typ) in object.get_fields() {
                if matches!(key, LuaMemberKey::Name(_) | LuaMemberKey::Integer(_)) {
                    properties.insert(
                        key.clone(),
                        Rc::new(MemberSymbol::new(MemberOrigin::Direct(typ.clone()))),
                    );
                }
            }
            for (key, typ) in object.get_index_access() {
                indexes.insert(
                    key.clone(),
                    Rc::new(MemberSymbol::new(MemberOrigin::Direct(typ.clone()))),
                );
            }
        }
        LuaType::Tuple(tuple) => {
            for (index, typ) in tuple.get_types().iter().enumerate() {
                properties.insert(
                    LuaMemberKey::Integer(index as i64 + 1),
                    Rc::new(MemberSymbol::new(MemberOrigin::Direct(typ.clone()))),
                );
            }
        }
        LuaType::Array(array) => {
            indexes.insert(
                LuaType::Integer,
                Rc::new(MemberSymbol::new(MemberOrigin::Direct(
                    array.get_base().clone(),
                ))),
            );
        }
        LuaType::TableGeneric(params) if params.len() == 2 => {
            indexes.insert(
                params[0].clone(),
                Rc::new(MemberSymbol::new(MemberOrigin::Direct(params[1].clone()))),
            );
        }
        _ => return None,
    }
    Some(())
}

fn resolve_declared_type_members<'a>(
    db: &DbIndex,
    cache: &'a mut SemanticLocalCache,
    id: &LuaTypeDeclId,
) -> Option<&'a DeclaredTypeMembers> {
    let entry = cache.declared_type_members.entry_ref(id);
    if let EntryRef::Occupied(entry) = entry {
        return Some(entry.into_mut());
    }
    if db.get_type_index().get_type_decl(id)?.is_alias() {
        return None;
    }
    // 声明成员独立于继承结果, 原始类型和各个实例共享同一符号来源.
    let mut declared = DeclaredTypeMembers::default();
    collect_owner_members(
        db,
        &LuaMemberOwner::Type(id.clone()),
        &mut declared.properties,
        &mut declared.index_infos,
    );
    Some(entry.or_insert(declared))
}

fn collect_generic_declared_members(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    generic: &LuaGenericType,
    properties: &mut MemberMap,
    indexes: &mut IndexInfoMap,
) -> Option<()> {
    let id = generic.get_base_type_id_ref();
    let source = resolve_declared_type_members(db, cache, id)?;
    let substitutor = Rc::new(TypeSubstitutor::from_type_array(
        generic.get_params().clone(),
    ));
    for (key, member) in &source.properties {
        properties.insert(
            key.clone(),
            Rc::new(MemberSymbol::new(MemberOrigin::Generic {
                source: member.clone(),
                substitutor: substitutor.clone(),
            })),
        );
    }
    for (key_type, member) in &source.index_infos {
        indexes.insert(
            instantiate_type_generic(db, key_type, &substitutor),
            Rc::new(MemberSymbol::new(MemberOrigin::Generic {
                source: member.clone(),
                substitutor: substitutor.clone(),
            })),
        );
    }
    Some(())
}

fn get_type_decl_base_types(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    typ: &LuaType,
) -> Option<Rc<[LuaType]>> {
    let id = get_type_decl_id(typ)?;
    if let Some(base_types) = cache.type_decl_base_types.get(id) {
        return Some(base_types.clone());
    }
    // 穿透 alias
    let mut seen = HashSet::new();
    let base_types: Rc<[LuaType]> = db
        .get_type_index()
        // 该迭代器已处理直接循环继承, 但对复杂情况没有处理, 因此我们必须在后面进行进一步的归约再过滤.
        .get_super_types_iter(id)
        .into_iter()
        .flatten()
        .filter_map(|super_type| get_reduced_type(db, cache, super_type).map(Cow::into_owned))
        // 禁止继承 union 类型, 因为无法确定具体分支
        .filter(|real_type| !matches!(real_type, LuaType::Union(_) | LuaType::MultiLineUnion(_)))
        // 别名展开后可能暴露自继承, 泛型实参不同也不能继承自身声明.
        .filter(|real_type| get_type_decl_id(real_type) != Some(id))
        .filter(|real_type| seen.insert(real_type.clone()))
        .collect();
    cache
        .type_decl_base_types
        .insert(id.clone(), base_types.clone());
    Some(base_types)
}

fn collect_owner_members(
    db: &DbIndex,
    owner: &LuaMemberOwner,
    properties: &mut MemberMap,
    indexes: &mut IndexInfoMap,
) {
    if let Some(items) = db.get_member_index().get_owner_members(owner) {
        let mut property_count = 0;
        let mut index_count = 0;
        for (key, _) in items.iter() {
            match key {
                LuaMemberKey::Name(_) | LuaMemberKey::Integer(_) => property_count += 1,
                LuaMemberKey::TypeKey(_) => index_count += 1,
                _ => {}
            }
        }
        properties.reserve(property_count);
        indexes.reserve(index_count);
        for (key, item) in items.iter() {
            match key {
                LuaMemberKey::Name(_) | LuaMemberKey::Integer(_) => {
                    properties.insert(
                        key.clone(),
                        Rc::new(MemberSymbol::new(MemberOrigin::InDb(item.clone()))),
                    );
                }
                LuaMemberKey::TypeKey(key_type) => {
                    indexes.insert(
                        key_type.clone(),
                        Rc::new(MemberSymbol::new(MemberOrigin::InDb(item.clone()))),
                    );
                }
                _ => {}
            }
        }
    }
}

#[derive(Clone)]
enum CompositeBranches<'a> {
    Union(UnionIter<'a>),
    MultiLineUnion(MultiLineUnionIter<'a>),
    Intersection(Iter<'a, LuaType>),
}

impl<'a> Iterator for CompositeBranches<'a> {
    type Item = Cow<'a, LuaType>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Union(branches) => branches.next(),
            Self::MultiLineUnion(branches) => branches.next().map(Cow::Borrowed),
            Self::Intersection(branches) => branches.next().map(Cow::Borrowed),
        }
    }
}

fn get_union_or_intersection_types(typ: &LuaType) -> Option<CompositeBranches<'_>> {
    match typ {
        LuaType::Union(union) => Some(CompositeBranches::Union(union.iter())),
        LuaType::MultiLineUnion(union) => Some(CompositeBranches::MultiLineUnion(union.iter())),
        LuaType::Intersection(intersection) => Some(CompositeBranches::Intersection(
            intersection.get_types().iter(),
        )),
        _ => None,
    }
}

fn resolve_reduced_composite_entry_id(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    typ: &LuaType,
) -> Option<MemberCacheEntryId> {
    let id = get_or_create_member_entry_id(cache, typ);
    if let Some(reduced) = get_member_entry(cache, id).reduced_entry_id {
        return Some(reduced);
    }
    if !cache.push_type_resolution(
        TypeSystemEntity::MemberEntry(id),
        TypeSystemPropertyName::ReducedType,
    ) {
        return None;
    }
    let resolved = (|| {
        let is_union = matches!(typ, LuaType::Union(_) | LuaType::MultiLineUnion(_));
        let mut seen = HashSet::new();
        let mut types = Vec::new();
        let mut changed = false;
        for source in get_union_or_intersection_types(typ)? {
            let current = get_reduced_type(db, cache, &source)?;
            changed |= current.as_ref() != source.as_ref();
            // 联合分支中的别名先展开, 交叉类型仍作为完整分支参与成员合成.
            if is_union
                && matches!(
                    current.as_ref(),
                    LuaType::Union(_) | LuaType::MultiLineUnion(_)
                )
            {
                changed = true;
                for nested in get_union_or_intersection_types(&current)? {
                    if seen.insert(nested.as_ref().clone()) {
                        types.push(nested.into_owned());
                    }
                }
            } else if seen.insert(current.as_ref().clone()) {
                types.push(current.into_owned());
            } else {
                changed = true;
            }
        }
        let reduced = if !changed {
            typ.clone()
        } else if is_union {
            LuaUnionType::from_vec(types).into()
        } else {
            LuaType::Intersection(LuaIntersectionType::new(types).into())
        };
        Some((reduced, changed))
    })();
    let no_cycle = cache.pop_type_resolution();
    let (reduced, changed) = resolved?;
    if !no_cycle {
        return None;
    }
    let reduced_id = if changed {
        // 新归约结果可直接用于成员查询, 无需重复整理分支.
        let reduced_id = get_or_create_member_entry_id(cache, &reduced);
        get_member_entry_mut(cache, reduced_id)
            .reduced_entry_id
            .get_or_insert(reduced_id);
        reduced_id
    } else {
        id
    };
    get_member_entry_mut(cache, id).reduced_entry_id = Some(reduced_id);
    Some(reduced_id)
}

fn get_type_decl_id(typ: &LuaType) -> Option<&LuaTypeDeclId> {
    match typ {
        LuaType::Ref(id) | LuaType::Def(id) => Some(id),
        LuaType::Generic(generic) => Some(generic.get_base_type_id_ref()),
        _ => None,
    }
}

fn is_object_type(typ: &LuaType) -> bool {
    matches!(
        typ,
        LuaType::Ref(_)
            | LuaType::Def(_)
            | LuaType::TableConst(_)
            | LuaType::Generic(_)
            | LuaType::Object(_)
            | LuaType::Tuple(_)
            | LuaType::Array(_)
            | LuaType::TableGeneric(_)
    )
}

fn is_structured_type(typ: &LuaType) -> bool {
    is_object_type(typ)
        || matches!(
            typ,
            LuaType::Union(_) | LuaType::MultiLineUnion(_) | LuaType::Intersection(_)
        )
}

fn resolve_object_type_members(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    id: MemberCacheEntryId,
    typ: &LuaType,
) -> Option<()> {
    let mut properties = MemberMap::default();
    let mut indexes = IndexInfoMap::default();
    collect_declared_type_members(db, cache, typ, &mut properties, &mut indexes)?;
    // 提前发布声明成员, 递归查询或父类解析失败时仍可读取.
    set_structured_type_members(
        get_member_entry_mut(cache, id),
        Some(properties),
        Some(indexes),
    );
    // 相同实例由 MembersResolved 截断, 不同实参的递归使用独立深度上限.
    if cache.inheritance_depth >= 128 {
        return Some(());
    }
    cache.inheritance_depth += 1;
    let resolved = resolve_inherited_type_members(db, cache, id, typ);
    cache.inheritance_depth -= 1;
    resolved
}

fn resolve_inherited_type_members(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    id: MemberCacheEntryId,
    typ: &LuaType,
) -> Option<()> {
    let base_types = get_type_decl_base_types(db, cache, typ);
    let Some(base_types) = base_types.filter(|bases| !bases.is_empty()) else {
        return Some(());
    };
    let substitutor = match typ {
        LuaType::Generic(generic) => Some(&TypeSubstitutor::from_type_array(
            generic.get_params().clone(),
        )),
        _ => None,
    };
    let mut properties = Vec::with_capacity(base_types.len());
    let mut indexes = Vec::new();
    for parent in base_types.iter() {
        let parent = match substitutor {
            Some(substitutor) => Cow::Owned(instantiate_type_generic(db, parent, &substitutor)),
            None => Cow::Borrowed(parent),
        };
        let Some(parent_id) = get_reduced_member_entry_id(db, cache, &parent)? else {
            continue;
        };

        let Some(parent_properties) = get_properties_by_entry_id(db, cache, parent_id) else {
            continue;
        };

        if resolve_structured_type_members(db, cache, parent_id).is_none() {
            continue;
        }
        // 保留父类快照, 全部解析结束后再写入, 避免提前暴露继承结果.
        properties.push(parent_properties);
        indexes.extend(
            get_member_entry(cache, parent_id)
                .index_infos
                .iter()
                .map(|(key, member)| (key.clone(), member.clone())),
        );
    }
    let entry = get_member_entry_mut(cache, id);
    let member_count = entry.members.len();
    for properties in properties {
        for (key, member) in properties.iter() {
            entry
                .members
                .entry(key.clone())
                .or_insert_with(|| member.clone());
        }
    }
    if entry.members.len() != member_count {
        entry.property_list = None;
    }
    for (key, member) in indexes {
        entry.index_infos.entry(key).or_insert(member);
    }
    Some(())
}

// 此处不设置 properties, 完整的属性应通过 [`get_properties_of_type`] 获取.
fn resolve_union_type_members(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    id: MemberCacheEntryId,
    typ: &LuaType,
) -> Option<()> {
    fn get_union_index_infos(
        db: &DbIndex,
        cache: &mut SemanticLocalCache,
        typ: &LuaType,
    ) -> Option<IndexInfoMap> {
        let types = get_union_or_intersection_types(typ)?;
        let mut types = types.filter(|current| !matches!(current.as_ref(), LuaType::Never));
        let mut indexes = IndexInfoMap::default();
        let Some(first) = types.next() else {
            return Some(indexes);
        };
        if !is_structured_type(&first) {
            return Some(indexes);
        }
        let first_id = resolve_index_info_entry_id(db, cache, &first)?;
        for index in 0..get_member_entry(cache, first_id).index_infos.len() {
            let (key, first_member) = get_member_entry(cache, first_id)
                .index_infos
                .get_index(index)?;
            let (key, first_member) = (key.clone(), first_member.clone());
            let members = once(Some(Some(first_member))).chain(types.clone().map(|current| {
                if !is_structured_type(&current) {
                    return Some(None);
                }
                let current_indexes = get_index_infos_of_type(db, cache, &current)?;
                let member = current_indexes.get(&key).cloned();
                Some(member)
            }));
            if let Some(member) = combine_member_symbols(true, members)? {
                indexes.insert(key, member);
            }
        }
        Some(indexes)
    }

    let indexes = get_union_index_infos(db, cache, typ)?;
    set_structured_type_members(get_member_entry_mut(cache, id), None, Some(indexes));
    Some(())
}

// 此处不设置 properties, 完整的属性应通过 [`get_properties_of_type`] 获取.
fn resolve_intersection_type_members(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    id: MemberCacheEntryId,
    typ: &LuaType,
) -> Option<()> {
    let mut grouped = IndexMap::<LuaType, Vec<Rc<MemberSymbol>>, FxBuildHasher>::default();
    for current in get_union_or_intersection_types(typ)? {
        if !is_structured_type(&current) {
            continue;
        }
        let current_indexes = get_index_infos_of_type(db, cache, &current)?;
        for (key, member) in current_indexes {
            grouped.entry(key.clone()).or_default().push(member.clone());
        }
    }
    let mut indexes = IndexInfoMap::default();
    for (key, members) in grouped {
        if let Some(member) =
            combine_member_symbols(false, members.into_iter().map(|member| Some(Some(member))))?
        {
            indexes.insert(key, member);
        }
    }
    set_structured_type_members(get_member_entry_mut(cache, id), None, Some(indexes));
    Some(())
}

#[cfg(test)]
mod tests {
    use std::{rc::Rc, sync::Arc};

    use crate::{LuaMemberIndexItem, LuaMemberKey, LuaType, VirtualWorkspace};

    use super::{
        MemberCacheFlag, MemberOrigin, SemanticLocalCache, TypeSystemEntity,
        TypeSystemPropertyName, get_applicable_index_info_for_name, get_index_infos_of_type,
        get_member_entry, get_member_entry_type, get_member_of_type, get_or_create_member_entry_id,
        get_properties_of_type, get_property_of_type, get_reduced_member_entry_id,
        get_reduced_type, get_type_decl_base_types, resolve_index_info_entry_id,
    };

    // TableConst 必须支持点查模式
    #[test]
    fn table_const_point_queries_only_cache_requested_keys() {
        let mut ws = VirtualWorkspace::new();
        let mut source = String::from("{");
        for index in 1..=256 {
            source.push_str(&format!("field{index} = {index}, [{index}] = {index},\n"));
        }
        source.push('}');
        let typ = ws.expr_ty(&source);
        assert!(matches!(typ, LuaType::TableConst(_)));
        let db = ws.analysis.compilation.get_db();
        let mut cache = SemanticLocalCache::new();
        for key in [LuaMemberKey::None, LuaMemberKey::TypeKey(LuaType::String)] {
            assert!(get_property_of_type(db, &mut cache, &typ, &key).is_none());
        }
        assert!(cache.type_member_entries.is_empty());

        for (key, expected) in [
            (
                LuaMemberKey::Name("field256".into()),
                LuaType::IntegerConst(256),
            ),
            (LuaMemberKey::Integer(1), LuaType::IntegerConst(1)),
        ] {
            let member = get_property_of_type(db, &mut cache, &typ, &key).unwrap();
            assert!(matches!(
                member.origin,
                MemberOrigin::InDb(LuaMemberIndexItem::One(_))
            ));
            assert!(member.typ.get().is_none());
            assert_eq!(member.typ(db), Some(&expected));
            let repeated = get_property_of_type(db, &mut cache, &typ, &key).unwrap();
            assert!(Rc::ptr_eq(&member, &repeated));
        }
        for key in [
            LuaMemberKey::Name("missing".into()),
            LuaMemberKey::Integer(4096),
        ] {
            for _ in 0..2 {
                assert!(get_property_of_type(db, &mut cache, &typ, &key).is_none());
            }
        }
        let entry = cache.type_member_entries.get(&typ).unwrap();
        assert_eq!(entry.members.len(), 2);
        assert!(entry.property_list.is_none());
        assert!(entry.call_signatures.is_none());
        let requested: Vec<_> = entry
            .members
            .iter()
            .map(|(key, member)| (key.clone(), member.clone()))
            .collect();
        let properties = get_properties_of_type(db, &mut cache, &typ).unwrap();
        assert_eq!(properties.len(), 512);
        for (key, member) in requested {
            let (_, listed) = properties.iter().find(|(name, _)| name == &key).unwrap();
            assert!(Rc::ptr_eq(&member, listed));
            let repeated = get_property_of_type(db, &mut cache, &typ, &key).unwrap();
            assert!(Rc::ptr_eq(&member, &repeated));
        }
        let repeated = get_properties_of_type(db, &mut cache, &typ).unwrap();
        assert!(Rc::ptr_eq(&properties, &repeated));
        let cold = get_properties_of_type(db, &mut SemanticLocalCache::new(), &typ).unwrap();
        assert!(
            properties
                .iter()
                .map(|(key, _)| key)
                .eq(cold.iter().map(|(key, _)| key))
        );
    }

    // 测试泛型继承时使用自身
    #[test]
    fn test_grow_forward_recursive_inheritance() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Forward<T>
            ---@field forward T

            ---@class Grow<T>: Forward<Grow<T[]>>
            "#,
        );
        let ty = ws.ty("Grow<number>");
        let expected = ws.ty("Grow<number[]>");

        let db = ws.analysis.compilation.get_db();
        let mut cache = SemanticLocalCache::new();
        let properties = get_properties_of_type(db, &mut cache, &ty);
        assert!(properties.is_some());

        let forward_key = LuaMemberKey::Name("forward".into());
        let properties = properties.unwrap();
        let (_, prop_from_all) = properties
            .iter()
            .find(|(key, _)| key == &forward_key)
            .unwrap();
        assert_eq!(prop_from_all.typ(db), Some(&expected));

        let prop_single = get_property_of_type(db, &mut cache, &ty, &forward_key).unwrap();
        assert_eq!(prop_single.typ(db), Some(&expected));
    }

    // 泛型支持混入模式
    #[test]
    fn test_generic_mixin_inheritance() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Forward<T>
            ---@field forward T

            ---@class Grow<T>: T
            ---@field x T
            "#,
        );
        let ty = ws.ty("Grow<Forward<number>>");
        let expected = ws.ty("number");

        let db = ws.analysis.compilation.get_db();
        let mut cache = SemanticLocalCache::new();
        let properties = get_properties_of_type(db, &mut cache, &ty);
        assert!(properties.is_some());

        let forward_key = LuaMemberKey::Name("forward".into());
        let properties = properties.unwrap();
        let (_, prop_from_all) = properties
            .iter()
            .find(|(key, _)| key == &forward_key)
            .unwrap();
        assert_eq!(prop_from_all.typ(db), Some(&expected));
    }

    // 相互引用的泛型混入环能够被截断并正确解析已有字段
    #[test]
    fn test_mutual_mixin_inheritance_cycle() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class MixinA<T>: T
            ---@field a string

            ---@class MixinB<T>: T
            ---@field b number
            "#,
        );
        let ty = ws.ty("MixinA<MixinB<MixinA<number>>>");
        let db = ws.analysis.compilation.get_db();
        let mut cache = SemanticLocalCache::new();
        let properties = get_properties_of_type(db, &mut cache, &ty).unwrap();

        assert!(
            properties
                .iter()
                .any(|(key, _)| key == &LuaMemberKey::Name("a".into()))
        );
        assert!(
            properties
                .iter()
                .any(|(key, _)| key == &LuaMemberKey::Name("b".into()))
        );
    }

    // 自指泛型约束(Grow<T>: T)实例化出的父类是更小的实例而非自身, 暂不做基类相同过滤以降低实现复杂度
    #[test]
    fn test_generic_mixin_inheritance_caches_parent_instance() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Grow<T>: T
            ---@field x T
            "#,
        );
        let ty = ws.ty("Grow<Grow<number>>");
        let parent_ty = ws.ty("Grow<number>");

        let db = ws.analysis.compilation.get_db();
        let mut cache = SemanticLocalCache::new();
        let properties = get_properties_of_type(db, &mut cache, &ty).unwrap();

        // 声明成员优先
        let x_key = LuaMemberKey::Name("x".into());
        let (_, member) = properties.iter().find(|(key, _)| key == &x_key).unwrap();
        assert_eq!(member.typ(db), Some(&parent_ty));

        // 顶层类型与解析中实例化出的父类各占一个条目
        assert_eq!(cache.type_member_entries.len(), 2);
        assert!(cache.type_member_entries.contains_key(&ty));
        assert!(cache.type_member_entries.contains_key(&parent_ty));

        // 后续直接查询命中已有条目, 不再重新解析
        let id = get_or_create_member_entry_id(&mut cache, &parent_ty);
        assert_eq!(
            resolve_index_info_entry_id(db, &mut cache, &parent_ty),
            Some(id)
        );
    }

    // 测试泛型别名与普通别名的展开
    #[test]
    fn test_alias_instance_members_resolved() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias StringMap<T> table<string, T>
            ---@alias NumberMap table<string, number>
            "#,
        );
        let generic_alias = ws.ty("StringMap<number>");
        let plain_alias = ws.ty("NumberMap");
        let expected = ws.ty("number");
        assert!(matches!(generic_alias, LuaType::Generic(_)));
        let db = ws.analysis.compilation.get_db();
        let mut cache = SemanticLocalCache::new();
        for ty in [generic_alias, plain_alias] {
            assert!(
                get_properties_of_type(db, &mut cache, &ty)
                    .unwrap()
                    .is_empty()
            );
            let entry = get_index_infos_of_type(db, &mut cache, &ty).unwrap();
            let member = entry.get(&LuaType::String).cloned();
            assert_eq!(
                member.map(|m| m.typ(db).cloned()),
                Some(Some(expected.clone()))
            );
        }
    }

    // 继承泛型别名的子类可以继承父类型展开后的索引成员.
    #[test]
    fn test_class_inheriting_generic_alias_inherits_index() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias StringMap<T> table<string, T>

            ---@class UsesAlias: StringMap<number>
            ---@field own_field integer
            "#,
        );
        let ty = ws.ty("UsesAlias");
        let expected = ws.ty("number");
        let db = ws.analysis.compilation.get_db();
        let mut cache = SemanticLocalCache::new();
        let properties = get_properties_of_type(db, &mut cache, &ty).unwrap();
        assert!(
            properties
                .iter()
                .any(|(key, _)| key == &LuaMemberKey::Name("own_field".into()))
        );
        let entry = get_index_infos_of_type(db, &mut cache, &ty).unwrap();
        let member = entry.get(&LuaType::String).cloned();
        assert_eq!(member.map(|m| m.typ(db).cloned()), Some(Some(expected)));
    }

    #[test]
    fn union_properties_use_index_fallback() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class HasValue
            ---@field value number
            ---@class MissingValue
            ---@field other boolean
            ---@alias Indexed HasValue | table<string, string>
            "#,
        );
        let readable = ws.ty("Indexed");
        let partial = ws.ty("Indexed | MissingValue");
        let indexed = ws.ty("table<string, number> | table<string, string>");
        let expected = ws.ty("number | string");
        let db = ws.analysis.compilation.get_db();
        let mut cache = SemanticLocalCache::new();
        let key = LuaMemberKey::Name("value".into());
        for (typ, is_partial) in [(readable, false), (partial, true)] {
            let member = get_member_of_type(db, &mut cache, &typ, &key)
                .unwrap()
                .unwrap();
            assert_eq!(member.typ(db), Some(&expected));
            assert_eq!(member.is_partial, is_partial);
            assert_eq!(
                get_property_of_type(db, &mut cache, &typ, &key).is_none(),
                is_partial
            );
            let properties = get_properties_of_type(db, &mut cache, &typ).unwrap();
            if is_partial {
                assert!(properties.is_empty());
            } else {
                assert_eq!(properties.len(), 1);
                assert_eq!(properties[0].0, key);
                assert!(Rc::ptr_eq(&member, &properties[0].1));
            }
        }
        assert!(get_property_of_type(db, &mut cache, &indexed, &key).is_none());
        assert!(
            get_properties_of_type(db, &mut cache, &indexed)
                .unwrap()
                .is_empty()
        );
        let index = get_applicable_index_info_for_name(db, &mut cache, &indexed, &key)
            .unwrap()
            .unwrap();
        assert_eq!(index.typ(db), Some(&expected));
    }

    #[test]
    fn union_property_order_does_not_depend_on_point_query_order() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class OrderedLeft
            ---@field first number
            ---@field second string
            ---@field left boolean

            ---@class OrderedRight
            ---@field first number
            ---@field second string
            ---@field right boolean
            "#,
        );
        let union = ws.ty("OrderedLeft | OrderedRight");
        let db = ws.analysis.compilation.get_db();
        let cold = get_properties_of_type(db, &mut SemanticLocalCache::new(), &union).unwrap();
        assert_eq!(cold.len(), 2);
        let mut cache = SemanticLocalCache::new();
        for name in ["left", "right"] {
            assert!(
                get_property_of_type(db, &mut cache, &union, &LuaMemberKey::Name(name.into()))
                    .is_none()
            );
        }
        for (key, _) in cold.iter().rev() {
            assert!(get_property_of_type(db, &mut cache, &union, key).is_some());
        }
        let warm = get_properties_of_type(db, &mut cache, &union).unwrap();
        assert!(
            cold.iter()
                .map(|(key, _)| key)
                .eq(warm.iter().map(|(key, _)| key))
        );
        for (key, listed) in warm.iter() {
            let member = get_property_of_type(db, &mut cache, &union, key).unwrap();
            assert!(Rc::ptr_eq(listed, &member));
        }
        let repeated = get_properties_of_type(db, &mut cache, &union).unwrap();
        assert!(Rc::ptr_eq(&warm, &repeated));
    }

    #[test]
    fn intersection_missing_branches_do_not_make_members_partial() {
        let mut ws = VirtualWorkspace::new();
        let intersection =
            ws.ty("{ value: string | number, left: boolean } & { value: number, right: string }");
        assert!(matches!(intersection, LuaType::Intersection(_)));
        let db = ws.analysis.compilation.get_db();
        let mut cache = SemanticLocalCache::new();
        let properties = get_properties_of_type(db, &mut cache, &intersection).unwrap();
        assert_eq!(properties.len(), 3);
        for (name, expected) in [
            ("value", LuaType::Number),
            ("left", LuaType::Boolean),
            ("right", LuaType::String),
        ] {
            let key = LuaMemberKey::Name(name.into());
            let member = get_property_of_type(db, &mut cache, &intersection, &key).unwrap();
            assert!(!member.is_partial);
            assert_eq!(member.typ(db), Some(&expected));
            let (_, listed) = properties.iter().find(|(name, _)| name == &key).unwrap();
            assert!(Rc::ptr_eq(&member, listed));
        }
    }

    #[test]
    fn recursive_union_does_not_cache_unfinished_queries_as_missing() {
        let mut ws = VirtualWorkspace::new();
        ws.def("---@alias RecursiveMembers RecursiveMembers | { value: number }");
        let typ = ws.ty("RecursiveMembers");
        let db = ws.analysis.compilation.get_db();
        let key = LuaMemberKey::Name("value".into());
        let mut cache = SemanticLocalCache::new();
        for _ in 0..2 {
            assert!(get_member_of_type(db, &mut cache, &typ, &key).is_none());
            assert!(get_properties_of_type(db, &mut cache, &typ).is_none());
            assert!(get_index_infos_of_type(db, &mut cache, &typ).is_none());
            assert!(get_applicable_index_info_for_name(db, &mut cache, &typ, &key).is_none());
            assert!(get_reduced_type(db, &mut cache, &typ).is_none());
            assert!(!cache.type_member_entries.is_empty());
            for entry in cache.type_member_entries.values() {
                assert!(entry.reduced_entry_id.is_none());
                assert!(!entry.members.contains_key(&key));
                assert!(entry.property_list.is_none());
                assert!(entry.call_signatures.is_none());
            }
            assert!(cache.type_resolutions.is_empty());
        }
    }

    #[test]
    fn nested_union_aliases_preserve_partial_member_sources() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class HasNumber
            ---@field value number
            ---@class Missing
            ---@field other boolean
            ---@class HasString
            ---@field value string
            ---@alias Inner HasNumber | Missing
            ---@alias Nested Inner | HasString
            ---@alias Maybe<T> T | Missing
            ---@alias MultiLine
            ---| HasNumber
            ---| Missing
            "#,
        );
        let source_type = ws.ty("HasNumber");
        let cases: Vec<_> = [
            ("HasNumber | Missing", "number"),
            ("Inner | HasString", "number | string"),
            ("Nested | boolean", "number | string"),
            ("Maybe<HasNumber> | HasString", "number | string"),
            ("MultiLine | HasString", "number | string"),
            ("Inner | boolean", "number"),
        ]
        .into_iter()
        .map(|(name, expected)| (name, ws.ty(name), ws.ty(expected)))
        .collect();
        let db = ws.analysis.compilation.get_db();
        let mut cache = SemanticLocalCache::new();
        let source = get_property_of_type(
            db,
            &mut cache,
            &source_type,
            &LuaMemberKey::Name("value".into()),
        )
        .unwrap();
        for (name, typ, expected) in cases {
            assert!(
                get_properties_of_type(db, &mut cache, &typ)
                    .unwrap()
                    .is_empty()
            );
            for (field, expected) in [("value", expected), ("other", LuaType::Boolean)] {
                let key = LuaMemberKey::Name(field.into());
                let member = get_member_of_type(db, &mut cache, &typ, &key)
                    .unwrap()
                    .unwrap();
                assert!(member.is_partial, "{name}.{field}");
                assert_eq!(member.typ(db), Some(&expected), "{name}.{field}");
                assert!(get_property_of_type(db, &mut cache, &typ, &key).is_none());
            }
            assert!(
                get_properties_of_type(db, &mut cache, &typ)
                    .unwrap()
                    .is_empty()
            );
        }
        assert!(!source.is_partial);
    }

    #[test]
    fn composite_reduction_is_shared_across_property_queries() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class ReductionLeft
            ---@field value number
            ---@field shared boolean
            ---@class ReductionRight
            ---@field value string
            ---@field shared boolean
            ---@alias ReductionInner ReductionLeft | ReductionRight
            ---@alias ReductionNested ReductionInner | ReductionLeft
            ---@alias ReductionLeftAlias ReductionLeft
            "#,
        );
        let union = ws.ty("ReductionNested");
        let expected_union = ws.ty("ReductionLeft | ReductionRight");
        let intersection = ws.ty("ReductionLeftAlias & ReductionRight");
        let expected_intersection = ws.ty("ReductionLeft & ReductionRight");
        let db = ws.analysis.compilation.get_db();
        let mut cache = SemanticLocalCache::new();
        for (typ, expected) in [
            (union, expected_union),
            (intersection, expected_intersection),
        ] {
            let reduced = get_reduced_type(db, &mut cache, &typ).unwrap().into_owned();
            assert_eq!(reduced, expected);
            for name in ["value", "shared"] {
                let key = LuaMemberKey::Name(name.into());
                let member = get_property_of_type(db, &mut cache, &typ, &key).unwrap();
                let direct = get_property_of_type(db, &mut cache, &reduced, &key).unwrap();
                assert!(Rc::ptr_eq(&member, &direct));
                let repeated = get_reduced_type(db, &mut cache, &typ).unwrap();
                match (&reduced, repeated.as_ref()) {
                    (LuaType::Union(first), LuaType::Union(second)) => {
                        assert!(Arc::ptr_eq(first, second));
                    }
                    (LuaType::Intersection(first), LuaType::Intersection(second)) => {
                        assert!(Arc::ptr_eq(first, second));
                    }
                    _ => panic!("unexpected reduced type"),
                }
            }
            let entry = cache.type_member_entries.get(&reduced).unwrap();
            assert_eq!(
                get_member_entry_type(&cache, entry.reduced_entry_id.unwrap()),
                &reduced
            );
        }
    }

    #[test]
    fn reduction_cache_only_contains_composite_types() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class ReductionObject
            ---@field value number
            ---@alias ReductionObjectAlias ReductionObject
            "#,
        );
        let object = ws.ty("ReductionObject");
        let alias = ws.ty("ReductionObjectAlias");
        let union = ws.ty("ReductionObject | nil");
        let db = ws.analysis.compilation.get_db();
        let mut cache = SemanticLocalCache::new();
        for typ in [&object, &alias, &LuaType::Number] {
            assert!(get_reduced_type(db, &mut cache, typ).is_some());
        }
        assert!(cache.type_member_entries.is_empty());
        let reduced = get_reduced_type(db, &mut cache, &union).unwrap();
        let (LuaType::Union(original), LuaType::Union(reduced)) = (&union, reduced.as_ref()) else {
            panic!("expected union");
        };
        assert!(Arc::ptr_eq(original, reduced));
        assert_eq!(cache.type_member_entries.len(), 1);
        let key = LuaMemberKey::Name("value".into());
        assert!(get_property_of_type(db, &mut cache, &object, &key).is_some());
        assert!(
            cache
                .type_member_entries
                .get(&object)
                .unwrap()
                .reduced_entry_id
                .is_none()
        );
        cache.clear();
        assert!(cache.type_member_entries.is_empty());
        assert!(get_reduced_type(db, &mut cache, &union).is_some());
        assert_eq!(cache.type_member_entries.len(), 1);
    }

    #[test]
    fn generic_recursive_base_finishes_published_members() {
        let mut ws = VirtualWorkspace::new();
        ws.def(r#"
            ---@class RecursiveMixin<T>: T
            ---@field own number
            ---@class RecursiveMixinBase
            ---@field inherited string
            ---@alias RecursiveMixinInstance RecursiveMixin<RecursiveMixinInstance & RecursiveMixinBase>
        "#);
        let typ = ws.ty("RecursiveMixinInstance");
        let db = ws.analysis.compilation.get_db();
        let mut cache = SemanticLocalCache::new();
        let properties = get_properties_of_type(db, &mut cache, &typ).unwrap();
        assert_eq!(
            properties
                .iter()
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>(),
            ["own", "inherited"].map(|name| LuaMemberKey::Name(name.into()))
        );
        let repeated = get_properties_of_type(db, &mut cache, &typ).unwrap();
        assert!(Rc::ptr_eq(&properties, &repeated));
        let id = get_reduced_member_entry_id(db, &mut cache, &typ)
            .unwrap()
            .unwrap();
        assert!(
            get_member_entry(&cache, id)
                .flags
                .contains(MemberCacheFlag::MembersResolved)
        );
        assert!(cache.type_resolutions.is_empty());
    }

    #[test]
    fn failed_base_reduction_keeps_published_declarations() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class FailedMixin<T>: T
            ---@field own number
            ---@alias UnfinishedBase UnfinishedBase | { fallback: string }
        "#,
        );
        let typ = ws.ty("FailedMixin<UnfinishedBase>");
        let db = ws.analysis.compilation.get_db();
        let mut cache = SemanticLocalCache::new();
        for _ in 0..2 {
            let properties = get_properties_of_type(db, &mut cache, &typ).unwrap();
            assert_eq!(properties.len(), 1);
            assert_eq!(properties[0].0, LuaMemberKey::Name("own".into()));
            assert_eq!(properties[0].1.typ(db), Some(&LuaType::Number));
            assert!(
                get_property_of_type(db, &mut cache, &typ, &LuaMemberKey::Name("fallback".into()))
                    .is_none()
            );
            let id = get_reduced_member_entry_id(db, &mut cache, &typ)
                .unwrap()
                .unwrap();
            assert!(
                get_member_entry(&cache, id)
                    .flags
                    .contains(MemberCacheFlag::MembersResolved)
            );
            assert!(cache.type_resolutions.is_empty());
            assert_eq!(cache.inheritance_depth, 0);
        }
    }

    #[test]
    fn enumeration_is_independent_of_point_and_index_queries() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class ListLeft
            ---@field value number
            ---@field left string
            ---@field [integer] boolean
            ---@class ListRight
            ---@field value string
            ---@field right boolean
            ---@field [integer] number
        "#,
        );
        let types = [
            ws.ty("ListLeft"),
            ws.ty("ListLeft | ListRight"),
            ws.ty("ListLeft & ListRight"),
        ];
        let db = ws.analysis.compilation.get_db();
        let key = LuaMemberKey::Name("value".into());
        for typ in types {
            let mut cache = SemanticLocalCache::new();
            let member = get_property_of_type(db, &mut cache, &typ, &key).unwrap();
            let id = get_reduced_member_entry_id(db, &mut cache, &typ)
                .unwrap()
                .unwrap();
            assert!(get_member_entry(&cache, id).property_list.is_none());
            assert!(
                get_index_infos_of_type(db, &mut cache, &typ)
                    .unwrap()
                    .contains_key(&LuaType::Integer)
            );
            assert!(get_member_entry(&cache, id).property_list.is_none());
            assert!(get_member_entry(&cache, id).call_signatures.is_none());
            let properties = get_properties_of_type(db, &mut cache, &typ).unwrap();
            let listed = &properties.iter().find(|(name, _)| name == &key).unwrap().1;
            assert!(Rc::ptr_eq(&member, listed));
            let repeated = get_properties_of_type(db, &mut cache, &typ).unwrap();
            assert!(Rc::ptr_eq(&properties, &repeated));
            let cold = get_properties_of_type(db, &mut SemanticLocalCache::new(), &typ).unwrap();
            assert!(
                properties
                    .iter()
                    .map(|(key, _)| key)
                    .eq(cold.iter().map(|(key, _)| key))
            );
        }
    }

    #[test]
    fn generic_alias_chains_preserve_inherited_members() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Plain { value: number }
            ---@alias Alias<T> Plain
            ---@class Mixin<T>: T
            ---@field own string
            "#,
        );
        let types = [
            (ws.ty("Alias<number>"), 1),
            (ws.ty("Mixin<Alias<number>>"), 2),
        ];
        let db = ws.analysis.compilation.get_db();
        let key = LuaMemberKey::Name("value".into());
        for (typ, expected_count) in types {
            let mut cache = SemanticLocalCache::new();
            let properties = get_properties_of_type(db, &mut cache, &typ).unwrap();
            assert_eq!(properties.len(), expected_count);
            let (_, listed) = properties.iter().find(|(name, _)| name == &key).unwrap();
            assert_eq!(listed.typ(db), Some(&LuaType::Number));
            let member = get_property_of_type(db, &mut cache, &typ, &key).unwrap();
            assert_eq!(member.typ(db), Some(&LuaType::Number));
            assert!(Rc::ptr_eq(listed, &member));
        }
    }

    #[test]
    fn type_resolution_distinguishes_targets_and_properties() {
        let mut cache = SemanticLocalCache::new();
        assert!(cache.push_type_resolution(
            TypeSystemEntity::Type(LuaType::Number),
            TypeSystemPropertyName::ReducedType,
        ));
        assert!(cache.type_member_entries.is_empty());

        let id = get_or_create_member_entry_id(&mut cache, &LuaType::Number);
        let resolutions = [
            (
                TypeSystemEntity::MemberEntry(id),
                TypeSystemPropertyName::ReducedType,
            ),
            (
                TypeSystemEntity::Type(LuaType::String),
                TypeSystemPropertyName::ReducedType,
            ),
            (
                TypeSystemEntity::Type(LuaType::Number),
                TypeSystemPropertyName::PropertyList,
            ),
            (
                TypeSystemEntity::Type(LuaType::Number),
                TypeSystemPropertyName::Property(LuaMemberKey::Name("value".into())),
            ),
            (
                TypeSystemEntity::Type(LuaType::Number),
                TypeSystemPropertyName::Property(LuaMemberKey::Name("other".into())),
            ),
        ];
        let count = resolutions.len() + 1;
        for (target, property_name) in resolutions {
            assert!(cache.push_type_resolution(target, property_name));
        }
        for _ in 0..count {
            assert!(cache.pop_type_resolution());
        }
        assert!(cache.type_resolutions.is_empty());
    }

    #[test]
    fn type_resolution_cycles_invalidate_only_dependent_resolutions() {
        let mut cache = SemanticLocalCache::new();
        let id = get_or_create_member_entry_id(&mut cache, &LuaType::Number);
        let typ = TypeSystemEntity::Type(LuaType::Number);
        let member = TypeSystemEntity::MemberEntry(id);
        for (target, dependency) in [(typ.clone(), member.clone()), (member, typ)] {
            assert!(cache.push_type_resolution(
                TypeSystemEntity::Type(LuaType::Boolean),
                TypeSystemPropertyName::ReducedType,
            ));
            assert!(
                cache.push_type_resolution(target.clone(), TypeSystemPropertyName::PropertyList,)
            );
            assert!(cache.push_type_resolution(
                target.clone(),
                TypeSystemPropertyName::Property(LuaMemberKey::Name("value".into())),
            ));
            assert!(cache.push_type_resolution(dependency, TypeSystemPropertyName::ReducedType));
            assert!(
                !cache.push_type_resolution(target.clone(), TypeSystemPropertyName::PropertyList,)
            );
            for _ in 0..3 {
                assert!(!cache.pop_type_resolution());
            }
            assert!(cache.pop_type_resolution());
            assert!(cache.type_resolutions.is_empty());

            assert!(cache.push_type_resolution(target, TypeSystemPropertyName::PropertyList));
            assert!(cache.pop_type_resolution());
        }
    }

    #[test]
    fn generic_alias_self_inheritance_keeps_declared_members() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Identity<T> T
            ---@class Grow<T>: Identity<Grow<T[]>>
            ---@field value T
            "#,
        );
        let typ = ws.ty("Grow<number>");
        let db = ws.analysis.compilation.get_db();
        let mut cache = SemanticLocalCache::new();
        assert!(
            get_type_decl_base_types(db, &mut cache, &typ)
                .unwrap()
                .is_empty()
        );

        let key = LuaMemberKey::Name("value".into());
        let member = get_property_of_type(db, &mut cache, &typ, &key).unwrap();
        assert_eq!(member.typ(db), Some(&LuaType::Number));
        let properties = get_properties_of_type(db, &mut cache, &typ).unwrap();
        assert_eq!(properties.len(), 1);
        assert_eq!(properties[0].0, key);
        assert!(Rc::ptr_eq(&properties[0].1, &member));
        assert_eq!(cache.type_member_entries.len(), 1);
    }

    #[test]
    fn growing_alias_inheritance_stops_at_depth_limit() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias Identity<T> T
            ---@class GrowA<T>: Identity<GrowB<T[]>>
            ---@field value T
            ---@class GrowB<T>: Identity<GrowA<T[]>>
            ---@field inherited T
            ---@class Ordinary
            ---@field value string
            "#,
        );
        let typ = ws.ty("GrowA<number>");
        let inherited = ws.ty("number[]");
        let ordinary = ws.ty("Ordinary");
        let db = ws.analysis.compilation.get_db();
        let mut cache = SemanticLocalCache::new();
        let properties = get_properties_of_type(db, &mut cache, &typ).unwrap();
        assert_eq!(properties.len(), 2);
        for (name, expected) in [("value", LuaType::Number), ("inherited", inherited)] {
            let key = LuaMemberKey::Name(name.into());
            let member = get_property_of_type(db, &mut cache, &typ, &key).unwrap();
            assert_eq!(member.typ(db), Some(&expected));
        }
        assert!(cache.type_member_entries.len() <= 129);
        assert!(cache.type_resolutions.is_empty());
        assert_eq!(cache.inheritance_depth, 0);
        let repeated = get_properties_of_type(db, &mut cache, &typ).unwrap();
        assert!(Rc::ptr_eq(&properties, &repeated));
        let member = get_property_of_type(
            db,
            &mut cache,
            &ordinary,
            &LuaMemberKey::Name("value".into()),
        )
        .unwrap();
        assert_eq!(member.typ(db), Some(&LuaType::String));
        assert!(cache.type_resolutions.is_empty());
        assert_eq!(cache.inheritance_depth, 0);
    }
}
