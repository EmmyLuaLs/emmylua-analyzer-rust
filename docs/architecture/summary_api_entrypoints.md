# Summary API Entry Points

## Goal

Salsa summary data should be consumed through a small set of cohesive facade entry points.
Callers should not need to remember scattered build/find/collect helper functions from query modules.

## Preferred Entry

Use `db.get_summary_db()` as the only stable starting point, then enter a cohesive group:

- `file()` for syntax/file-level summaries
- `doc()` for doc summaries
- `doc().signature()` for signature/generic/call/return related queries
- `lexical()` for lexical references and scopes
- `flow()` for flow structures and control-flow facts
- `module()` for module export/import facts
- `semantic()` for semantic graph and semantic target queries
- `types()` for type summary queries

## Rules

1. Prefer adding a nested facade group before adding another root-level method.
2. Prefer exact/indexed lookup APIs over returning a full summary and rescanning it in callers.
3. Query module helpers should stay implementation detail unless a type must cross crate boundaries.
4. Compilation should act as projection/adaptation, not as a second fact source.
5. If a caller needs a new access pattern repeatedly, add one cohesive facade method instead of exposing another helper family.

## Current Example

Signature-related access now goes through `doc().signature()` instead of spreading across the root doc facade:

- `summary(file_id)`
- `explain(file_id, signature_offset)`
- `generic_param(file_id, owner_offset, name)`
- `call_explain(file_id, call_offset)`
- `return_query(file_id, signature_offset)`
- `owner_resolves(file_id, signature_offset)`

This is the pattern to continue for other dense API clusters.
