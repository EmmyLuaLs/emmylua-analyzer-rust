# Salsa-First Architecture Direction

## Why this refactor exists

The codebase is moving away from an analyzer-centered architecture where `update_index()` runs a mutation-heavy pipeline that writes many legacy indexes into `DbIndex`.

The target architecture makes salsa summary data the source of truth and treats the rest of the system as projections or queries over that fact layer.

## Target shape

### 1. `DbIndex` becomes a container, not the semantic source of truth

- `DbIndex` keeps workspace state, VFS-backed file snapshots, and the mounted `SalsaSummaryDatabase`.
- Production inputs enter through `DbIndex`, but derived semantic data should come from salsa facts and salsa queries.
- Legacy mutable caches inside `DbIndex` are being reduced to compatibility shells and should disappear once consumers are migrated.

### 2. `summary_builder.analysis` is the single-file fact layer

- This layer is syntax-driven and file-local.
- It may read AST and parser structures.
- It should not read `DbIndex`.
- It should not perform cross-file resolution.
- It should not write compatibility state for old consumers.

Typical facts in this layer are:

- decl tree
- members
- doc summaries and doc type nodes
- flow summaries
- lexical use summaries
- property summaries
- table shape summaries
- signature summaries
- module export syntax facts

### 3. `summary_builder.query` is the derived query layer

- This layer builds reusable indexes and derived summaries on top of file facts.
- It is the right place for hot-path lookup structures, reverse indexes, cross-summary joins, graph summaries, and fixpoint inputs.
- It must stay read-only with respect to compilation state.

Current direction in this layer is clear:

- move repeated scans behind explicit query indexes
- expose practical lookup APIs directly from summary/query
- make syntax id and syntax offset handles usable without reparsing or re-searching AST

Recent examples of that direction include:

- property reverse indexes
- lexical reverse indexes
- module export resolve indexes
- table shape direct lookup
- decl/member by syntax id lookup
- flow exact lookup by syntax offset/node offset

### 4. `compilation` becomes the projection layer

- `compilation` should adapt salsa/query results into shapes that current callers still expect.
- This is the migration landing zone for code that used to read analyzer-written structures directly.
- It may combine VFS context, summary facts, and query outputs, but it should not rebuild analyzer-style global mutable state.

This means `compilation` is not the new analyzer. It is a bridge layer that gradually shrinks as more callers become summary/query-native.

### 5. `analyzer` is being replaced, not rebuilt

- The old analyzer pipeline is no longer the architectural center.
- `LuaCompilation::update_index()` is already being pushed toward "sync facts" instead of "run analysis pipeline".
- Remaining analyzer-era behavior should be migrated into either:
  - file-local fact extraction in `summary_builder.analysis`
  - derived indexing/query logic in `summary_builder.query`
  - consumer-facing projection code in `compilation`

The intended end state is that `compilation::analyzer` becomes unnecessary and can be deleted rather than modernized.

## Practical migration rule

When a consumer still needs a legacy answer, prefer this order:

1. Ask whether the needed data is already present in file facts.
2. If yes, add or improve a summary/query index instead of rescanning syntax or vectors in consumers.
3. If the caller still needs an old shape, project it in `compilation`.
4. Only add new fact fields when the information is truly missing, not when the real gap is just lookup ergonomics.

This rule is important because several recent bottlenecks were not missing facts. They were missing practical query APIs.

## What the architecture is currently optimizing for

The present direction is not just "use salsa" in the abstract. It is specifically optimizing for:

- single source of truth in summary data
- explicit fact/query/projection boundaries
- cheap incremental rebuilds per file
- reusable indexed hot paths instead of repeated linear scans
- replacing syntax re-search with stable handles such as syntax ids, offsets, and summary-owned IDs
- removing analyzer-only compatibility state over time

## Near-term consequences

If this direction is followed consistently, the codebase should gradually converge on this workflow:

1. VFS/workspace changes enter through `DbIndex`.
2. Salsa recomputes file facts.
3. Query indexes derive practical lookup and semantic structures.
4. `compilation` projects those results for existing consumers.
5. Old analyzer write paths are deleted once all read paths have moved.

## Current status summary

The migration is already past the "just planning" stage.

What is clearly in place now:

- salsa is mounted in `DbIndex`
- summary sync is the primary update path
- module and decl projections have started moving into `compilation`
- several hot lookup paths have been converted from repeated scans to query indexes

What remains structurally important:

- continue migrating read paths off analyzer-backed state
- keep turning summary facts into practical IDs and indexed lookups
- finish moving semantic and type-query consumers onto summary/query-backed paths
- remove analyzer-era compatibility caches once no longer read

## Short answer

The architecture is moving toward a salsa-first, summary-driven system where:

- `DbIndex` is the container for inputs and mounted databases
- `summary_builder.analysis` owns file-local facts
- `summary_builder.query` owns derived indexes and semantic queries
- `compilation` owns compatibility projections for callers
- `analyzer` is being retired instead of preserved as the core execution model
