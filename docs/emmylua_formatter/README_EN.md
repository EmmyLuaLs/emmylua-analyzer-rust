# EmmyLua Formatter Guide

[中文文档](./README_CN.md)

This document is the entry point for the EmmyLua formatter documentation. It summarizes the formatter's goals, behavior, configuration model, and the recommended reading path for users who want either a quick setup or a deeper understanding of layout decisions.

## Scope

The formatter is responsible for:

- Lua and EmmyLua source formatting
- width-aware line breaking
- controlled trailing-comment alignment
- EmmyLua doc-tag normalization and alignment
- CLI and library-based formatting workflows

The formatter is intentionally conservative around comments and ambiguous syntax. When a rewrite would be risky, the implementation prefers preserving structure over forcing a prettier result.

## Documentation Map

- [Formatting Examples](./examples_EN.md): before-and-after examples for common formatter decisions
- [Formatter Options](./options_EN.md): configuration groups, defaults, and what each option changes
- [Recommended Profiles](./profiles_EN.md): suggested formatter configurations for common team styles
- [Formatter Tutorial](./tutorial_EN.md): practical setup, CLI workflows, and before/after examples

## Layout Model

Recent formatter work introduced candidate-based layout selection for sequence-like constructs such as call arguments, parameters, table fields, binary-expression chains, and statement expression lists.

For these constructs, the formatter can compare multiple candidates:

- flat
- progressive fill
- balanced packed layout
- one item per line
- aligned variants when comment alignment is enabled and justified by the input

The selected result is based on rendered output rather than a fixed priority chain. Overflow is penalized first, then line count, then optional line-balance scoring for targeted sites, then style preference, and finally remaining line slack.

## Recommended Reading

If you are new to the formatter:

1. Read [Formatter Tutorial](./tutorial_EN.md) for installation, config discovery, and day-to-day usage.
2. Read [Formatter Options](./options_EN.md) when you need to tune width, spacing, comments, or doc-tag behavior.

If you are integrating the formatter into tooling:

1. Start with the crate README at `crates/emmylua_formatter/README.md`.
2. Use [Formatter Options](./options_EN.md) as the public configuration reference.
