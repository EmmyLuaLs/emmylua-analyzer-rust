use rowan::TextSize;
use smol_str::SmolStr;

use super::super::{SalsaLookupBucket, build_lookup_buckets, find_bucket_indices};

use crate::{
    SalsaCallUseSummary, SalsaDeclId, SalsaMemberTargetId, SalsaMemberTargetSummary,
    SalsaMemberUseSummary, SalsaNameUseResolutionSummary, SalsaNameUseSummary,
    SalsaSyntaxIdSummary, SalsaUseSiteIndexSummary, SalsaUseSiteRoleSummary,
};

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum SalsaLexicalUseSummary {
    Call {
        syntax_offset: TextSize,
        syntax_id: SalsaSyntaxIdSummary,
        kind: crate::SalsaCallKindSummary,
        is_colon_call: bool,
        arg_count: usize,
        require_path: Option<SmolStr>,
        callee_name: Option<SmolStr>,
        callee_member: Option<SalsaMemberTargetId>,
    },
    Name {
        syntax_offset: TextSize,
        syntax_id: SalsaSyntaxIdSummary,
        name: SmolStr,
        role: SalsaUseSiteRoleSummary,
        resolution: SalsaNameUseResolutionSummary,
    },
    Member {
        syntax_offset: TextSize,
        syntax_id: SalsaSyntaxIdSummary,
        role: SalsaUseSiteRoleSummary,
        target: SalsaMemberTargetId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaLexicalUseIndex {
    pub uses: Vec<SalsaLexicalUseSummary>,
    all_name_uses: Vec<SalsaNameUseSummary>,
    decl_name_uses: Vec<SalsaNameUseSummary>,
    global_name_uses: Vec<SalsaNameUseSummary>,
    member_uses: Vec<SalsaMemberUseSummary>,
    call_uses: Vec<SalsaCallUseSummary>,
    by_decl: Vec<SalsaLookupBucket<SalsaDeclId>>,
    by_global_name: Vec<SalsaLookupBucket<SmolStr>>,
    by_name_role: Vec<SalsaLookupBucket<SalsaUseSiteRoleSummary>>,
    by_member: Vec<SalsaLookupBucket<SalsaMemberTargetId>>,
    by_member_role: Vec<SalsaLookupBucket<SalsaUseSiteRoleSummary>>,
    by_call_offset: Vec<SalsaLookupBucket<TextSize>>,
    by_call_syntax_id: Vec<SalsaLookupBucket<SalsaSyntaxIdSummary>>,
    by_name_syntax_id: Vec<SalsaLookupBucket<SalsaSyntaxIdSummary>>,
    by_member_syntax_id: Vec<SalsaLookupBucket<SalsaSyntaxIdSummary>>,
    by_callee_name: Vec<SalsaLookupBucket<SmolStr>>,
    by_callee_member: Vec<SalsaLookupBucket<SalsaMemberTargetId>>,
}

pub fn build_lexical_use_index(use_sites: &SalsaUseSiteIndexSummary) -> SalsaLexicalUseIndex {
    let mut uses =
        Vec::with_capacity(use_sites.names.len() + use_sites.members.len() + use_sites.calls.len());
    let all_name_uses = use_sites.names.clone();
    let mut decl_name_uses = Vec::new();
    let mut global_name_uses = Vec::new();
    let member_uses = use_sites.members.clone();
    let call_uses = use_sites.calls.clone();
    let mut decl_entries = Vec::new();
    let mut global_name_entries = Vec::new();
    let mut name_role_entries = Vec::new();
    let mut member_entries = Vec::new();
    let mut member_role_entries = Vec::new();
    let mut call_offset_entries = Vec::new();
    let mut call_syntax_id_entries = Vec::new();
    let mut name_syntax_id_entries = Vec::new();
    let mut member_syntax_id_entries = Vec::new();
    let mut callee_name_entries = Vec::new();
    let mut callee_member_entries = Vec::new();

    uses.extend(
        use_sites
            .calls
            .iter()
            .cloned()
            .map(|call_use| SalsaLexicalUseSummary::Call {
                syntax_offset: call_use.syntax_offset,
                syntax_id: call_use.syntax_id,
                kind: call_use.kind,
                is_colon_call: call_use.is_colon_call,
                arg_count: call_use.arg_count,
                require_path: call_use.require_path,
                callee_name: call_use.callee_name,
                callee_member: call_use.callee_member,
            }),
    );

    uses.extend(
        use_sites
            .names
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, name_use)| {
                name_role_entries.push((name_use.role.clone(), index));
                name_syntax_id_entries.push((name_use.syntax_id, index));

                match &name_use.resolution {
                    SalsaNameUseResolutionSummary::LocalDecl(decl_id) => {
                        decl_entries.push((*decl_id, decl_name_uses.len()));
                        decl_name_uses.push(name_use.clone());
                    }
                    SalsaNameUseResolutionSummary::Global => {
                        global_name_entries.push((name_use.name.clone(), global_name_uses.len()));
                        global_name_uses.push(name_use.clone());
                    }
                }

                SalsaLexicalUseSummary::Name {
                    syntax_offset: name_use.syntax_offset,
                    syntax_id: name_use.syntax_id,
                    name: name_use.name,
                    role: name_use.role,
                    resolution: name_use.resolution,
                }
            }),
    );
    uses.extend(
        use_sites
            .members
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, member_use)| {
                member_entries.push((member_use.target.clone(), index));
                member_role_entries.push((member_use.role.clone(), index));
                member_syntax_id_entries.push((member_use.syntax_id, index));

                SalsaLexicalUseSummary::Member {
                    syntax_offset: member_use.syntax_offset,
                    syntax_id: member_use.syntax_id,
                    role: member_use.role,
                    target: member_use.target,
                }
            }),
    );

    for (index, call_use) in call_uses.iter().enumerate() {
        call_offset_entries.push((call_use.syntax_offset, index));
        call_syntax_id_entries.push((call_use.syntax_id, index));
        if let Some(callee_name) = &call_use.callee_name {
            callee_name_entries.push((callee_name.clone(), index));
        }
        if let Some(callee_member) = &call_use.callee_member {
            callee_member_entries.push((callee_member.clone(), index));
        }
    }

    uses.sort_by_key(|use_summary| match use_summary {
        SalsaLexicalUseSummary::Member { syntax_offset, .. } => (*syntax_offset, 0_u8),
        SalsaLexicalUseSummary::Name { syntax_offset, .. } => (*syntax_offset, 1_u8),
        SalsaLexicalUseSummary::Call { syntax_offset, .. } => (*syntax_offset, 2_u8),
    });

    SalsaLexicalUseIndex {
        uses,
        all_name_uses,
        decl_name_uses,
        global_name_uses,
        member_uses,
        call_uses,
        by_decl: build_lookup_buckets(decl_entries),
        by_global_name: build_lookup_buckets(global_name_entries),
        by_name_role: build_lookup_buckets(name_role_entries),
        by_member: build_lookup_buckets(member_entries),
        by_member_role: build_lookup_buckets(member_role_entries),
        by_call_offset: build_lookup_buckets(call_offset_entries),
        by_call_syntax_id: build_lookup_buckets(call_syntax_id_entries),
        by_name_syntax_id: build_lookup_buckets(name_syntax_id_entries),
        by_member_syntax_id: build_lookup_buckets(member_syntax_id_entries),
        by_callee_name: build_lookup_buckets(callee_name_entries),
        by_callee_member: build_lookup_buckets(callee_member_entries),
    }
}

pub fn find_lexical_use_at(
    lexical_uses: &SalsaLexicalUseIndex,
    syntax_offset: TextSize,
) -> Option<SalsaLexicalUseSummary> {
    if let Some(exact) = lexical_uses
        .uses
        .iter()
        .find(|use_summary| use_syntax_offset(use_summary) == syntax_offset)
    {
        return Some(exact.clone());
    }

    lexical_uses
        .uses
        .iter()
        .filter(|use_summary| use_syntax_id(use_summary).contains_offset(syntax_offset))
        .min_by_key(|use_summary| {
            (
                use_syntax_id(use_summary).span_len(),
                use_syntax_offset(use_summary),
            )
        })
        .cloned()
}

pub fn find_call_use_at(
    use_sites: &SalsaUseSiteIndexSummary,
    syntax_offset: TextSize,
) -> Option<SalsaCallUseSummary> {
    if let Some(exact) = use_sites
        .calls
        .iter()
        .find(|call_use| call_use.syntax_offset == syntax_offset)
    {
        return Some(exact.clone());
    }

    use_sites
        .calls
        .iter()
        .filter(|call_use| call_use.syntax_id.contains_offset(syntax_offset))
        .min_by_key(|call_use| (call_use.syntax_id.span_len(), call_use.syntax_offset))
        .cloned()
}

pub fn find_call_use_by_syntax_id(
    index: &SalsaLexicalUseIndex,
    syntax_id: SalsaSyntaxIdSummary,
) -> Option<SalsaCallUseSummary> {
    find_bucket_indices(&index.by_call_syntax_id, &syntax_id)
        .and_then(|indices| indices.first().copied())
        .and_then(|entry_index| index.call_uses.get(entry_index).cloned())
}

pub fn find_name_use_at(
    use_sites: &SalsaUseSiteIndexSummary,
    syntax_offset: TextSize,
) -> Option<SalsaNameUseSummary> {
    if let Some(exact) = use_sites
        .names
        .iter()
        .find(|name_use| name_use.syntax_offset == syntax_offset)
    {
        return Some(exact.clone());
    }

    use_sites
        .names
        .iter()
        .filter(|name_use| name_use.syntax_id.contains_offset(syntax_offset))
        .min_by_key(|name_use| (name_use.syntax_id.span_len(), name_use.syntax_offset))
        .cloned()
}

pub fn find_name_use_by_syntax_id(
    index: &SalsaLexicalUseIndex,
    syntax_id: SalsaSyntaxIdSummary,
) -> Option<SalsaNameUseSummary> {
    find_bucket_indices(&index.by_name_syntax_id, &syntax_id)
        .and_then(|indices| indices.first().copied())
        .and_then(|entry_index| index.all_name_uses.get(entry_index).cloned())
}

pub fn find_member_use_at(
    use_sites: &SalsaUseSiteIndexSummary,
    syntax_offset: TextSize,
) -> Option<SalsaMemberUseSummary> {
    if let Some(exact) = use_sites
        .members
        .iter()
        .find(|member_use| member_use.syntax_offset == syntax_offset)
    {
        return Some(exact.clone());
    }

    use_sites
        .members
        .iter()
        .filter(|member_use| member_use.syntax_id.contains_offset(syntax_offset))
        .min_by_key(|member_use| (member_use.syntax_id.span_len(), member_use.syntax_offset))
        .cloned()
}

pub fn find_member_use_by_syntax_id(
    index: &SalsaLexicalUseIndex,
    syntax_id: SalsaSyntaxIdSummary,
) -> Option<SalsaMemberUseSummary> {
    find_bucket_indices(&index.by_member_syntax_id, &syntax_id)
        .and_then(|indices| indices.first().copied())
        .and_then(|entry_index| index.member_uses.get(entry_index).cloned())
}

pub fn collect_decl_references(
    use_sites: &SalsaUseSiteIndexSummary,
    decl_id: SalsaDeclId,
) -> Vec<SalsaNameUseSummary> {
    let index = build_lexical_use_index(use_sites);
    collect_decl_references_in_index(&index, decl_id)
}

pub fn collect_global_name_references(
    use_sites: &SalsaUseSiteIndexSummary,
    name: &str,
) -> Vec<SalsaNameUseSummary> {
    let index = build_lexical_use_index(use_sites);
    collect_global_name_references_in_index(&index, name)
}

pub fn collect_name_references_by_role(
    use_sites: &SalsaUseSiteIndexSummary,
    role: &SalsaUseSiteRoleSummary,
) -> Vec<SalsaNameUseSummary> {
    let index = build_lexical_use_index(use_sites);
    collect_name_references_by_role_in_index(&index, role)
}

pub fn collect_member_references(
    use_sites: &SalsaUseSiteIndexSummary,
    member_target: &SalsaMemberTargetSummary,
) -> Vec<SalsaMemberUseSummary> {
    let index = build_lexical_use_index(use_sites);
    collect_member_references_in_index(&index, member_target)
}

pub fn collect_member_references_by_role(
    use_sites: &SalsaUseSiteIndexSummary,
    role: &SalsaUseSiteRoleSummary,
) -> Vec<SalsaMemberUseSummary> {
    let index = build_lexical_use_index(use_sites);
    collect_member_references_by_role_in_index(&index, role)
}

pub fn collect_call_references_for_name(
    use_sites: &SalsaUseSiteIndexSummary,
    callee_name: &str,
) -> Vec<SalsaCallUseSummary> {
    let index = build_lexical_use_index(use_sites);
    collect_call_references_for_name_in_index(&index, callee_name)
}

pub fn collect_call_references_for_member(
    use_sites: &SalsaUseSiteIndexSummary,
    member_target: &SalsaMemberTargetSummary,
) -> Vec<SalsaCallUseSummary> {
    let index = build_lexical_use_index(use_sites);
    collect_call_references_for_member_in_index(&index, member_target)
}

pub fn collect_decl_references_in_index(
    index: &SalsaLexicalUseIndex,
    decl_id: SalsaDeclId,
) -> Vec<SalsaNameUseSummary> {
    collect_name_uses(
        &index.decl_name_uses,
        find_bucket_indices(&index.by_decl, &decl_id),
    )
}

pub fn collect_global_name_references_in_index(
    index: &SalsaLexicalUseIndex,
    name: &str,
) -> Vec<SalsaNameUseSummary> {
    collect_name_uses(
        &index.global_name_uses,
        find_bucket_indices(&index.by_global_name, &name.into()),
    )
}

pub fn collect_name_references_by_role_in_index(
    index: &SalsaLexicalUseIndex,
    role: &SalsaUseSiteRoleSummary,
) -> Vec<SalsaNameUseSummary> {
    collect_name_uses(
        &index.all_name_uses,
        find_bucket_indices(&index.by_name_role, role),
    )
}

pub fn collect_member_references_in_index(
    index: &SalsaLexicalUseIndex,
    member_target: &SalsaMemberTargetSummary,
) -> Vec<SalsaMemberUseSummary> {
    let member_target = SalsaMemberTargetId::from(member_target);
    collect_member_uses(
        &index.member_uses,
        find_bucket_indices(&index.by_member, &member_target),
    )
}

pub fn collect_member_references_by_role_in_index(
    index: &SalsaLexicalUseIndex,
    role: &SalsaUseSiteRoleSummary,
) -> Vec<SalsaMemberUseSummary> {
    collect_member_uses(
        &index.member_uses,
        find_bucket_indices(&index.by_member_role, role),
    )
}

pub fn collect_call_references_for_name_in_index(
    index: &SalsaLexicalUseIndex,
    callee_name: &str,
) -> Vec<SalsaCallUseSummary> {
    collect_call_uses(
        &index.call_uses,
        find_bucket_indices(&index.by_callee_name, &callee_name.into()),
    )
}

pub fn collect_call_references_for_member_in_index(
    index: &SalsaLexicalUseIndex,
    member_target: &SalsaMemberTargetSummary,
) -> Vec<SalsaCallUseSummary> {
    let member_target = SalsaMemberTargetId::from(member_target);
    collect_call_uses(
        &index.call_uses,
        find_bucket_indices(&index.by_callee_member, &member_target),
    )
}

pub fn find_call_use_in_index(
    index: &SalsaLexicalUseIndex,
    syntax_offset: TextSize,
) -> Option<SalsaCallUseSummary> {
    find_bucket_indices(&index.by_call_offset, &syntax_offset)
        .and_then(|indices| indices.first().copied())
        .map(|call_index| index.call_uses[call_index].clone())
}

fn use_syntax_offset(use_summary: &SalsaLexicalUseSummary) -> TextSize {
    match use_summary {
        SalsaLexicalUseSummary::Call { syntax_offset, .. }
        | SalsaLexicalUseSummary::Name { syntax_offset, .. }
        | SalsaLexicalUseSummary::Member { syntax_offset, .. } => *syntax_offset,
    }
}

fn use_syntax_id(use_summary: &SalsaLexicalUseSummary) -> SalsaSyntaxIdSummary {
    match use_summary {
        SalsaLexicalUseSummary::Call { syntax_id, .. }
        | SalsaLexicalUseSummary::Name { syntax_id, .. }
        | SalsaLexicalUseSummary::Member { syntax_id, .. } => *syntax_id,
    }
}

fn collect_name_uses(
    names: &[SalsaNameUseSummary],
    indices: Option<&[usize]>,
) -> Vec<SalsaNameUseSummary> {
    indices
        .into_iter()
        .flatten()
        .map(|index| names[*index].clone())
        .collect()
}

fn collect_member_uses(
    members: &[SalsaMemberUseSummary],
    indices: Option<&[usize]>,
) -> Vec<SalsaMemberUseSummary> {
    indices
        .into_iter()
        .flatten()
        .map(|index| members[*index].clone())
        .collect()
}

fn collect_call_uses(
    calls: &[SalsaCallUseSummary],
    indices: Option<&[usize]>,
) -> Vec<SalsaCallUseSummary> {
    indices
        .into_iter()
        .flatten()
        .map(|index| calls[*index].clone())
        .collect()
}
