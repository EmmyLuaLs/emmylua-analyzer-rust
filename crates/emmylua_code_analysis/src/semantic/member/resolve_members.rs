//! 成员缓存结构原型, 入口仅由本文件测试调用, 暂不接入实际语义流程.
//! 本文件所有内容均为对 `&db` 的引用解析, 即意味着所有类型都是确定的, 不存在未解析的情况.
//! - 暂不对 union 类型实现 Partial 标识
//! - `SemanticLocalCache` 在真实环境内应为单线程全局缓存
//! - 测试尽量使用 `VirtualWorkspace` 的 `ty` `expr_ty` `def系列` 等根据文本构造待测试类型, 不能虚空构造类型编造在实际上不可能发生的错误
#![allow(dead_code)]

use std::{
    borrow::Cow,
    cell::{Cell, OnceCell, RefCell},
    iter::once,
    mem::take,
    rc::Rc,
    slice::Iter,
};

use flagset::{FlagSet, flags};
use hashbrown::{HashMap, HashSet};
use indexmap::IndexMap;
use rustc_hash::FxBuildHasher;

use crate::{
    DbIndex, LuaGenericType, LuaMemberIndexItem, LuaMemberKey, LuaMemberOwner, LuaType,
    LuaTypeDeclId, MultiLineUnionIter, TypeOps, TypeSubstitutor, UnionIter,
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
}

impl MemberSymbol {
    pub fn new(origin: MemberOrigin) -> Self {
        Self {
            origin,
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

/// 完整键索引, 成员来源和解析结果通过 Rc 共享.
pub type MemberMap = IndexMap<LuaMemberKey, Rc<MemberSymbol>, FxBuildHasher>;

type IndexInfoMap = IndexMap<LuaType, Rc<MemberSymbol>, FxBuildHasher>;

#[derive(Debug, Default)]
struct DeclaredMembers {
    properties: MemberMap,
    indexes: IndexInfoMap,
}

#[derive(Debug)]
enum PropertyCache {
    /// 已确定的属性结果, None 表示不存在, 未缓存的键仍需解析.
    Partial(IndexMap<LuaMemberKey, Option<Rc<MemberSymbol>>, FxBuildHasher>),
    /// 完整属性表, 缺失键表示不存在.
    Complete(Rc<MemberMap>),
}

impl Default for PropertyCache {
    fn default() -> Self {
        Self::Partial(IndexMap::default())
    }
}

flags! {
    pub enum MemberCacheFlag: u8 {
        /// 本类型声明的成员已发布, 继承成员仍可能不完整.
        MembersPublished,
        /// 结构化成员整体已解析, 包含索引和调用签名, 组合类型的完整属性表除外.
        MembersResolved,
    }
}

#[derive(Debug, Default)]
pub struct MemberCacheEntry {
    flags: Cell<FlagSet<MemberCacheFlag>>,
    properties: RefCell<PropertyCache>,
    /// 索引键
    indexes: RefCell<IndexInfoMap>,
    /// 可调用函数, 属于结构化成员解析结果.
    call_signatures: OnceCell<Option<Rc<[LuaType]>>>,
}

impl MemberCacheEntry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_or_create(cache: &mut SemanticLocalCache, typ: &LuaType) -> Rc<Self> {
        cache.type_entries.entry_ref(typ).or_default().clone()
    }

    fn has_complete_properties(&self) -> bool {
        matches!(&*self.properties.borrow(), PropertyCache::Complete(_))
    }

    fn resolved_properties(&self) -> Option<Rc<MemberMap>> {
        match &*self.properties.borrow() {
            PropertyCache::Partial(_) => None,
            PropertyCache::Complete(properties) => Some(properties.clone()),
        }
    }

    fn complete_properties(&self) {
        let mut cache = self.properties.borrow_mut();
        let PropertyCache::Partial(properties) = &mut *cache else {
            return;
        };
        let properties = take(properties)
            .into_iter()
            .filter_map(|(key, member)| Some((key, member?)))
            .collect();
        *cache = PropertyCache::Complete(Rc::new(properties));
    }

    fn members_resolved(&self) -> bool {
        self.flags.get().contains(MemberCacheFlag::MembersResolved)
    }

    fn cached_property(&self, key: &LuaMemberKey) -> Option<Option<Rc<MemberSymbol>>> {
        match &*self.properties.borrow() {
            PropertyCache::Partial(properties) => properties.get(key).cloned(),
            PropertyCache::Complete(properties) => Some(properties.get(key).cloned()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TypeSystemEntity {
    Type(LuaType),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TypeSystemPropertyName {
    Property(LuaMemberKey),
    Properties,
    Members,
}

#[derive(Debug)]
struct TypeResolution {
    target: TypeSystemEntity,
    property_name: TypeSystemPropertyName,
    /// 是否未发生循环依赖
    result: bool,
}

#[derive(Debug, Default)]
pub struct SemanticLocalCache {
    declared_members: HashMap<LuaTypeDeclId, Rc<DeclaredMembers>, FxBuildHasher>,
    base_types: HashMap<LuaTypeDeclId, Rc<[LuaType]>, FxBuildHasher>,
    type_entries: HashMap<LuaType, Rc<MemberCacheEntry>, FxBuildHasher>,
    type_resolutions: Vec<TypeResolution>,
}

impl SemanticLocalCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.type_resolutions.clear();
        self.type_entries.clear();
        self.base_types.clear();
        self.declared_members.clear();
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
            // 若调用栈中的某祖先请求在当前缓存中已有确定结果, 则说明可以安全阻断, 因为此时绝不会在此处发生死环.
            // 注意: 现阶段所有完成标志(MembersResolved / Complete 属性表 / Property 键写入)都发生在对应 pop 之后,
            // 组合类型也不提前发布属性表, 因此栈上祖先的请求不可能已有确定结果, 此分支暂不可达; 留待人工审查后决定去留.
            if self.type_resolution_has_property(&self.type_resolutions[index]) {
                break;
            }
            if self.type_resolutions[index].target == target
                && self.type_resolutions[index].property_name == property_name
            {
                for resolution in &mut self.type_resolutions[index..] {
                    resolution.result = false;
                }
                return false;
            }
        }
        self.type_resolutions.push(TypeResolution {
            target,
            property_name,
            result: true,
        });
        true
    }

    fn pop_type_resolution(&mut self) -> bool {
        self.type_resolutions
            .pop()
            .is_some_and(|resolution| resolution.result)
    }

    fn type_resolution_has_property(&self, resolution: &TypeResolution) -> bool {
        let TypeSystemEntity::Type(typ) = &resolution.target;
        self.type_entries
            .get(typ)
            .is_some_and(|entry| match &resolution.property_name {
                TypeSystemPropertyName::Property(key) => entry.cached_property(key).is_some(),
                TypeSystemPropertyName::Properties => entry.has_complete_properties(),
                TypeSystemPropertyName::Members => entry.members_resolved(),
            })
    }
}

/// 仅返回完整属性表, 点查使用独立入口.
pub fn get_properties_of_type(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    typ: &LuaType,
) -> Option<Rc<MemberMap>> {
    let typ = get_reduced_type(db, typ)?;
    let entry = match typ.as_ref() {
        LuaType::Union(_) | LuaType::MultiLineUnion(_) | LuaType::Intersection(_) => {
            get_properties_of_union_or_intersection_type(db, cache, &typ)
        }
        _ => resolve_structured_type_members(db, cache, &typ),
    }?;
    entry.resolved_properties()
}

fn resolve_structured_type_members(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    typ: &LuaType,
) -> Option<Rc<MemberCacheEntry>> {
    if !is_structured_type(typ) {
        return None;
    }
    let entry = MemberCacheEntry::get_or_create(cache, typ);
    if entry.members_resolved() {
        return Some(entry);
    }
    if is_object_type(typ) {
        if !entry
            .flags
            .get()
            .contains(MemberCacheFlag::MembersPublished)
        {
            let mut properties = MemberMap::default();
            let mut indexes = IndexInfoMap::default();
            collect_declared_type_members(db, cache, typ, &mut properties, &mut indexes)?;
            publish_declared_type_members(&entry, properties, indexes);
        }
        // 先发布声明成员, 继承中的循环不妨碍读取已确定的属性.
        if !cache.push_type_resolution(
            TypeSystemEntity::Type(typ.clone()),
            TypeSystemPropertyName::Members,
        ) {
            return Some(entry);
        }
        let inherited = match typ {
            LuaType::Ref(id) | LuaType::Def(id) => {
                inherit_parent_members(db, cache, id, None, &entry)
            }
            LuaType::Generic(generic) => {
                let substitutor = TypeSubstitutor::from_type_array(generic.get_params().clone());
                inherit_parent_members(
                    db,
                    cache,
                    generic.get_base_type_id_ref(),
                    Some(&substitutor),
                    &entry,
                )
            }
            _ => Some(()),
        };

        let no_cycle = cache.pop_type_resolution();
        if inherited.is_some() && no_cycle {
            entry.complete_properties();
            entry
                .flags
                .set(entry.flags.get() | MemberCacheFlag::MembersResolved);
        }
        return Some(entry);
    }

    if !cache.push_type_resolution(
        TypeSystemEntity::Type(typ.clone()),
        TypeSystemPropertyName::Members,
    ) {
        return None;
    }
    let indexes = match typ {
        LuaType::Union(_) | LuaType::MultiLineUnion(_) => resolve_union_index_infos(db, cache, typ),
        LuaType::Intersection(_) => resolve_intersection_index_infos(db, cache, typ),
        _ => None,
    };
    let no_cycle = cache.pop_type_resolution();
    let indexes = indexes?;
    if !no_cycle {
        return None;
    }
    *entry.indexes.borrow_mut() = indexes;
    entry
        .flags
        .set(entry.flags.get() | MemberCacheFlag::MembersResolved);
    Some(entry)
}

fn publish_declared_type_members(
    entry: &MemberCacheEntry,
    properties: MemberMap,
    indexes: IndexInfoMap,
) {
    // 保持声明顺序, 并复用配置表点查时已创建的成员.
    let properties = properties
        .into_iter()
        .map(|(key, member)| {
            let member = entry.cached_property(&key).flatten().unwrap_or(member);
            (key, Some(member))
        })
        .collect();
    *entry.properties.borrow_mut() = PropertyCache::Partial(properties);
    *entry.indexes.borrow_mut() = indexes;
    entry
        .flags
        .set(entry.flags.get() | MemberCacheFlag::MembersPublished);
}

fn get_properties_of_union_or_intersection_type(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    typ: &LuaType,
) -> Option<Rc<MemberCacheEntry>> {
    let entry = MemberCacheEntry::get_or_create(cache, typ);
    if entry.has_complete_properties() {
        return Some(entry);
    }
    if !cache.push_type_resolution(
        TypeSystemEntity::Type(typ.clone()),
        TypeSystemPropertyName::Properties,
    ) {
        return None;
    }
    let resolved = collect_union_or_intersection_properties(db, cache, typ);
    let no_cycle = cache.pop_type_resolution();
    resolved?;
    if !no_cycle {
        return None;
    }
    entry.complete_properties();
    Some(entry)
}

fn collect_union_or_intersection_properties(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    typ: &LuaType,
) -> Option<()> {
    let is_union = matches!(typ, LuaType::Union(_) | LuaType::MultiLineUnion(_));
    for current in get_union_or_intersection_types(typ)? {
        let current = get_reduced_type(db, &current)?;
        let current_properties = match get_properties_of_type(db, cache, &current) {
            Some(properties) => properties,
            None if is_structured_type(&current) => return None,
            None => continue,
        };
        for key in current_properties.keys() {
            if get_property_of_type(db, cache, typ, key).is_none() {
                // 缺失键已写入缓存, 未完成的查询仍需重试.
                cache.type_entries.get(typ)?.cached_property(key)?;
            }
        }
        // 组合索引可能范围重叠但键不相同, 仅对象的完整空索引表允许提前停止.
        if is_union
            && is_object_type(&current)
            && get_index_infos_of_type(db, cache, &current)?
                .indexes
                .borrow()
                .is_empty()
        {
            break;
        }
    }
    Some(())
}

/// 按固定属性键查询成员, 索引签名由索引接口解析.
/// 缺失或查询尚未完成时返回 None, 仅确定的结果写入缓存.
pub fn get_property_of_type(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    typ: &LuaType,
    key: &LuaMemberKey,
) -> Option<Rc<MemberSymbol>> {
    let typ = get_reduced_type(db, typ)?;
    if !is_structured_type(&typ) || !matches!(key, LuaMemberKey::Name(_) | LuaMemberKey::Integer(_))
    {
        return None;
    }
    let entry = MemberCacheEntry::get_or_create(cache, &typ);
    if let Some(member) = entry.cached_property(key) {
        return member;
    }

    // 仅支持部分类型点查, 对大部分类型来说仍然应直接构建完整的属性表
    let member = match typ.as_ref() {
        LuaType::Union(_) | LuaType::MultiLineUnion(_) | LuaType::Intersection(_) => {
            get_property_of_union_or_intersection_type(db, cache, &typ, key)?
        }
        LuaType::TableConst(range) => {
            let owner = LuaMemberOwner::Element(range.clone());
            db.get_member_index()
                .get_member_item(&owner, key)
                .map(|item| Rc::new(MemberSymbol::new(MemberOrigin::InDb(item.clone()))))
        }
        _ => {
            let entry = resolve_structured_type_members(db, cache, &typ)?;
            entry.cached_property(key)?
        }
    };
    match &mut *entry.properties.borrow_mut() {
        PropertyCache::Partial(properties) => {
            properties.entry(key.clone()).or_insert(member).clone()
        }
        PropertyCache::Complete(properties) => properties.get(key).cloned(),
    }
}

fn get_property_of_union_or_intersection_type(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    typ: &LuaType,
    key: &LuaMemberKey,
) -> Option<Option<Rc<MemberSymbol>>> {
    if !cache.push_type_resolution(
        TypeSystemEntity::Type(typ.clone()),
        TypeSystemPropertyName::Property(key.clone()),
    ) {
        return None;
    }
    let member = create_union_or_intersection_property(db, cache, typ, key);
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
    let member = combine_member_symbols(
        is_union,
        types
            .filter(|current| {
                !matches!(
                    get_reduced_type(db, current).as_deref(),
                    Some(LuaType::Never)
                )
            })
            .map(|current| {
                let property = get_property_of_type(db, cache, &current, key);
                if property.is_some() {
                    has_property = true;
                    return Some(property);
                }
                let current = get_reduced_type(db, &current)?;
                // 只有确定缺失的属性才能由索引补足或跳过.
                if is_structured_type(&current) {
                    cache.type_entries.get(&*current)?.cached_property(key)?;
                }
                if is_union {
                    get_applicable_index_info_for_name(db, cache, &current, key)
                } else {
                    Some(None)
                }
            }),
    )?;
    // 索引签名只补足已有属性, 不能凭空生成任意具名属性.
    Some(if has_property { member } else { None })
}

/// 仅返回完整解析的索引信息.
pub fn get_index_infos_of_type(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    typ: &LuaType,
) -> Option<Rc<MemberCacheEntry>> {
    let typ = get_reduced_type(db, typ)?;
    let entry = resolve_structured_type_members(db, cache, &typ)?;
    entry.members_resolved().then_some(entry)
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
    let typ = get_reduced_type(db, typ)?;
    if !is_structured_type(&typ) {
        return Some(None);
    }
    if let Some(types) = get_union_or_intersection_types(&typ) {
        let is_union = matches!(typ.as_ref(), LuaType::Union(_) | LuaType::MultiLineUnion(_));
        let mut infos = Vec::new();
        for current in types.filter(|current| {
            !matches!(
                get_reduced_type(db, current).as_deref(),
                Some(LuaType::Never)
            )
        }) {
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
    let entry = get_index_infos_of_type(db, cache, &typ)?;
    find_applicable_index_info(db, &entry.indexes.borrow(), key_type)
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

fn get_reduced_type<'a>(db: &'a DbIndex, typ: &'a LuaType) -> Option<Cow<'a, LuaType>> {
    get_reduced_type_with_depth(db, typ, 0)
}

fn get_reduced_type_with_depth<'a>(
    db: &'a DbIndex,
    typ: &'a LuaType,
    depth: u32,
) -> Option<Cow<'a, LuaType>> {
    if depth >= 32 {
        return Some(Cow::Borrowed(typ));
    }
    match typ {
        LuaType::Ref(id) => {
            let type_decl = db.get_type_index().get_type_decl(id)?;
            if type_decl.is_alias() {
                return get_reduced_type_with_depth(db, type_decl.get_alias_ref()?, depth + 1);
            }
            Some(Cow::Borrowed(typ))
        }
        LuaType::Def(id) => {
            let def_as_ref = LuaType::Ref(id.clone());
            let reduced = get_reduced_type_with_depth(db, &def_as_ref, depth + 1)?;
            match reduced {
                Cow::Borrowed(_) => Some(Cow::Owned(def_as_ref)),
                Cow::Owned(reduced) => Some(Cow::Owned(reduced)),
            }
        }
        LuaType::Generic(generic) => {
            let type_decl = db
                .get_type_index()
                .get_type_decl(generic.get_base_type_id_ref())?;
            if !type_decl.is_alias() {
                return Some(Cow::Borrowed(typ));
            }
            let substitutor = TypeSubstitutor::from_type_array(generic.get_params().clone());
            let origin = type_decl.get_alias_origin(db, Some(&substitutor))?;
            let expanded = get_reduced_type_with_depth(db, &origin, depth + 1)?;
            match expanded {
                Cow::Borrowed(_) => Some(origin),
                Cow::Owned(expanded) => Some(Cow::Owned(expanded)),
            }
        }
        _ => Some(Cow::Borrowed(typ)),
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
            let declared = resolve_declared_members(db, cache, id)?;
            properties.extend(
                declared
                    .properties
                    .iter()
                    .map(|(key, member)| (key.clone(), member.clone())),
            );
            indexes.extend(
                declared
                    .indexes
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

fn resolve_declared_members(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    id: &LuaTypeDeclId,
) -> Option<Rc<DeclaredMembers>> {
    if let Some(declared) = cache.declared_members.get(id) {
        return Some(declared.clone());
    }
    if db.get_type_index().get_type_decl(id)?.is_alias() {
        return None;
    }
    // 声明成员独立于继承结果, 原始类型和各个实例共享同一符号来源.
    let mut declared = DeclaredMembers::default();
    collect_owner_members(
        db,
        &LuaMemberOwner::Type(id.clone()),
        &mut declared.properties,
        &mut declared.indexes,
    );
    let declared = Rc::new(declared);
    cache.declared_members.insert(id.clone(), declared.clone());
    Some(declared)
}

fn collect_generic_declared_members(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    generic: &LuaGenericType,
    properties: &mut MemberMap,
    indexes: &mut IndexInfoMap,
) -> Option<()> {
    let id = generic.get_base_type_id_ref();
    let source = resolve_declared_members(db, cache, id)?;
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
    for (key_type, member) in &source.indexes {
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

fn get_base_types(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    id: &LuaTypeDeclId,
) -> Rc<[LuaType]> {
    if let Some(base_types) = cache.base_types.get(id) {
        return base_types.clone();
    }
    // 穿透 alias
    let mut seen = HashSet::new();
    let base_types: Rc<[LuaType]> = db
        .get_type_index()
        .get_super_types_iter(id)
        .into_iter()
        .flatten()
        .filter_map(|super_type| get_reduced_type(db, super_type).map(Cow::into_owned))
        .filter(|real_type| seen.insert(real_type.clone()))
        .collect();
    cache.base_types.insert(id.clone(), base_types.clone());
    base_types
}

fn inherit_parent_members(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    id: &LuaTypeDeclId,
    substitutor: Option<&TypeSubstitutor>,
    entry: &MemberCacheEntry,
) -> Option<()> {
    let base_types = get_base_types(db, cache, id);
    for parent in base_types.iter() {
        let parent = match substitutor {
            Some(substitutor) => Cow::Owned(instantiate_type_generic(db, parent, substitutor)),
            None => Cow::Borrowed(parent),
        };
        let parent = get_reduced_type(db, &parent)?;
        if !is_structured_type(&parent) {
            continue;
        }

        let Some(parent_properties) = get_properties_of_type(db, cache, &parent) else {
            continue;
        };

        let Some(parent_indexes) = get_index_infos_of_type(db, cache, &parent) else {
            continue;
        };
        // 只合并完整父类型, 保证已发布成员不会被后续继承改写.
        if let PropertyCache::Partial(properties) = &mut *entry.properties.borrow_mut() {
            for (key, member) in parent_properties.iter() {
                properties
                    .entry(key.clone())
                    .or_insert_with(|| Some(member.clone()));
            }
        }

        {
            let parent_indexes = parent_indexes.indexes.borrow();
            let mut indexes = entry.indexes.borrow_mut();
            for (key, member) in parent_indexes.iter() {
                indexes.entry(key.clone()).or_insert_with(|| member.clone());
            }
        }
    }
    Some(())
}

fn collect_owner_members(
    db: &DbIndex,
    owner: &LuaMemberOwner,
    properties: &mut MemberMap,
    indexes: &mut IndexInfoMap,
) {
    if let Some(items) = db.get_member_index().get_member_items(owner) {
        for (key, item) in items {
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

fn resolve_union_index_infos(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    typ: &LuaType,
) -> Option<IndexInfoMap> {
    let mut types = get_union_or_intersection_types(typ)?.filter(|current| {
        !matches!(
            get_reduced_type(db, current).as_deref(),
            Some(LuaType::Never)
        )
    });
    let mut indexes = IndexInfoMap::default();
    let Some(first) = types.next() else {
        return Some(indexes);
    };
    let first = get_reduced_type(db, &first)?;
    if !is_structured_type(&first) {
        return Some(indexes);
    }
    let first_entry = get_index_infos_of_type(db, cache, &first)?;
    let first_indexes = first_entry.indexes.borrow().clone();
    for (key, first_member) in first_indexes {
        let members = once(Some(Some(first_member))).chain(types.clone().map(|current| {
            let current = get_reduced_type(db, &current)?;
            if !is_structured_type(&current) {
                return Some(None);
            }
            let current_entry = get_index_infos_of_type(db, cache, &current)?;
            let member = current_entry.indexes.borrow().get(&key).cloned();
            Some(member)
        }));
        if let Some(member) = combine_member_symbols(true, members)? {
            indexes.insert(key, member);
        }
    }
    Some(indexes)
}

fn resolve_intersection_index_infos(
    db: &DbIndex,
    cache: &mut SemanticLocalCache,
    typ: &LuaType,
) -> Option<IndexInfoMap> {
    let mut grouped = IndexMap::<LuaType, Vec<Rc<MemberSymbol>>, FxBuildHasher>::default();
    for current in get_union_or_intersection_types(typ)? {
        let current = get_reduced_type(db, &current)?;
        if !is_structured_type(&current) {
            continue;
        }
        let current_entry = get_index_infos_of_type(db, cache, &current)?;
        for (key, member) in current_entry.indexes.borrow().iter() {
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
    Some(indexes)
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use crate::{LuaMemberIndexItem, LuaMemberKey, LuaType, VirtualWorkspace};

    use super::{
        MemberOrigin, SemanticLocalCache, get_index_infos_of_type, get_properties_of_type,
        get_property_of_type,
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
        assert!(cache.type_entries.is_empty());

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
        let prop_from_all = properties.get(&forward_key).unwrap();
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
        let prop_from_all = properties.get(&forward_key).unwrap();
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

        assert!(properties.get(&LuaMemberKey::Name("a".into())).is_some());
        assert!(properties.get(&LuaMemberKey::Name("b".into())).is_some());
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
        assert_eq!(properties.get(&x_key).unwrap().typ(db), Some(&parent_ty));

        // 顶层类型与解析中实例化出的父类各占一个条目
        assert_eq!(cache.type_entries.len(), 2);
        assert!(cache.type_entries.contains_key(&ty));
        assert!(cache.type_entries.contains_key(&parent_ty));

        // 后续直接查询命中已有条目, 不再重新解析
        let entry = get_index_infos_of_type(db, &mut cache, &parent_ty).unwrap();
        assert!(Rc::ptr_eq(
            &entry,
            cache.type_entries.get(&parent_ty).unwrap()
        ));
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
            assert!(entry.members_resolved());
            let member = entry.indexes.borrow().get(&LuaType::String).cloned();
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
        assert!(properties.contains_key(&LuaMemberKey::Name("own_field".into())));
        let entry = get_index_infos_of_type(db, &mut cache, &ty).unwrap();
        assert!(entry.members_resolved());
        let member = entry.indexes.borrow().get(&LuaType::String).cloned();
        assert_eq!(member.map(|m| m.typ(db).cloned()), Some(Some(expected)));
    }
}
