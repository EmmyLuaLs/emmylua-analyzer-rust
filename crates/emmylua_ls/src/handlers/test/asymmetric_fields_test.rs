#![cfg(test)]
use crate::handlers::{
    completion::completion, definition::definition, hover::hover,
    test_lib::ProviderVirtualWorkspace,
};
use googletest::prelude::*;
use lsp_types::{
    CompletionResponse, CompletionTriggerKind, GotoDefinitionResponse, HoverContents, Position,
};
use tokio_util::sync::CancellationToken;

const DEFS: &str = "---@meta\n---@class Property\n--- A transformed property.\n---@field (read) value number\n---@field (write) value 'coal'|'wood'\n---@type Property\nproperty = {}\n";

#[gtest]
fn asymmetric_fields_hover_and_navigation() {
    let mut ws = ProviderVirtualWorkspace::new();
    ws.def_file("defs.lua", DEFS);
    let file = ws.def_file("use.lua", "local x = property.value\nproperty.value = 'coal'\nlocal y = property.value\nproperty['value'] = 'wood'");
    for (line, col, target_line) in [(0, 20, 3), (1, 11, 4), (2, 20, 3), (3, 12, 4)] {
        let result = hover(&ws.analysis, file, Position::new(line, col)).expect("hover");
        let HoverContents::Markup(text) = result.contents else {
            panic!("markup");
        };
        assert!(text.value.contains("Read: number"), "{}", text.value);
        assert!(text.value.contains("Write:"), "{}", text.value);
        assert!(text.value.contains("coal"), "{}", text.value);
        assert!(
            text.value.contains("A transformed property."),
            "{}",
            text.value
        );
        let definition =
            definition(&ws.analysis, file, Position::new(line, col)).expect("definition");
        let locations = match definition {
            GotoDefinitionResponse::Scalar(location) => vec![location],
            GotoDefinitionResponse::Array(locations) => locations,
            _ => panic!("locations"),
        };
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].range.start.line, target_line);
    }
}

#[gtest]
fn asymmetric_fields_property_references_and_rename() {
    let mut ws = ProviderVirtualWorkspace::new();
    let source = "---@class Property\n---@field (read) value number\n---@field (write) value string\n---@param property Property\nlocal function check(property)\nlocal x = property.value\nproperty.value = 'coal'\nend";
    let file = ws.def_file("paired.lua", &source);
    let ranges = source
        .lines()
        .enumerate()
        .filter_map(|(line, text)| {
            let start = text.find("value")? as u32;
            Some(lsp_types::Range::new(
                Position::new(line as u32, start),
                Position::new(line as u32, start + 5),
            ))
        })
        .collect::<Vec<_>>();
    assert_eq!(ranges.len(), 4);
    for range in &ranges {
        let cursor = Position::new(range.start.line, range.start.character + 2);
        let refs = crate::handlers::references::references(&ws.analysis, file, cursor, true)
            .expect("references");
        let mut hits: Vec<_> = refs.iter().map(|location| location.range).collect();
        hits.sort_by_key(|r| r.start);
        assert!(
            hits.starts_with(&ranges[..2]),
            "both accessor declarations from {cursor:?}: {hits:?}"
        );
        let uses = crate::handlers::references::references(&ws.analysis, file, cursor, false)
            .expect("uses");
        assert!(
            uses.iter()
                .all(|location| !ranges[..2].contains(&location.range))
        );
        if range == &ranges[3] {
            assert!(
                ranges[2..].iter().all(|runtime| hits.contains(runtime)),
                "indexed runtime uses: {hits:?}"
            );
        }
        let changes = crate::handlers::rename::rename(&ws.analysis, file, cursor, "fuel".into())
            .expect("rename")
            .changes
            .expect("edits");
        let mut edits: Vec<_> = changes
            .values()
            .flatten()
            .map(|edit| {
                assert_eq!(edit.new_text, "fuel");
                edit.range
            })
            .collect();
        edits.sort_by_key(|r| r.start);
        assert_eq!(edits, ranges, "rename from {cursor:?}");
    }
}

#[gtest]
fn asymmetric_fields_completion() -> Result<()> {
    let mut ws = ProviderVirtualWorkspace::new();
    ws.def_file("defs.lua", DEFS);
    for (source, expected, count) in [
        ("property.<??>", "value", 1),
        ("property.value = <??>", "coal", 1),
    ] {
        let (source, position) = ProviderVirtualWorkspace::handle_file_content(source)?;
        let file = ws.def(&source);
        let response = completion(
            &ws.analysis,
            file,
            position,
            CompletionTriggerKind::INVOKED,
            CancellationToken::new(),
        )
        .expect("completion");
        let items = match response {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };
        if expected == "value" {
            let item = items.iter().find(|item| item.label == "value").unwrap();
            let resolved = crate::handlers::completion::completion_resolve(
                &ws.analysis,
                item.clone(),
                crate::context::ClientId::Intellij,
            );
            let detail = serde_json::to_string(&resolved).unwrap();
            assert!(
                detail.contains("Read: number")
                    && detail.contains("Write:")
                    && detail.contains("coal"),
                "{detail}"
            );
        }
        assert_eq!(
            items
                .iter()
                .filter(|item| item.label.contains(expected))
                .count(),
            count,
            "{items:?}"
        );
    }
    Ok(())
}

#[gtest]
fn asymmetric_fields_review_hover_paragraphs() {
    for (key, expression) in [
        ("value", "property.va<??>lue"),
        ("['odd-key']", "property['odd-<??>key']"),
        ("[1]", "property[1<??>]"),
    ] {
        for newline in ["\n", "\r\n"] {
            let mut ws = ProviderVirtualWorkspace::new();
            ws.def_file("defs.lua", &format!(
                "--- 中文 🦀\n---@class Property\n--- First paragraph.\n---\n--- Second paragraph.\n---@field (read) {key} number\n---@field (write) {key} string\n---@type Property\nproperty = {{}}",
            ).replace('\n', newline));
            let (source, position) = ProviderVirtualWorkspace::handle_file_content(&format!(
                "local value = {expression}"
            ))
            .expect("position");
            let file = ws.def_file("use.lua", &source);
            let result = hover(&ws.analysis, file, position).expect("hover");
            let HoverContents::Markup(text) = result.contents else {
                panic!("markup");
            };
            assert!(
                text.value.contains("First paragraph.\n\nSecond paragraph."),
                "{key}: {}",
                text.value
            );
            assert!(!text.value.contains("中文"), "{}", text.value);
        }
    }
}

#[gtest]
fn asymmetric_fields_review_hover_comment_without_space() {
    let mut ws = ProviderVirtualWorkspace::new();
    ws.def_file(
        "defs.lua",
        "---@class Property\n---Property documentation.\n---@field (read) value number\n---@field (write) value string\n---@type Property\nproperty = {}",
    );
    let file = ws.def_file("use.lua", "local value = property.value");
    let result = hover(&ws.analysis, file, Position::new(0, 25)).expect("hover");
    let HoverContents::Markup(text) = result.contents else {
        panic!("markup");
    };
    assert!(
        text.value.contains("Property documentation."),
        "{}",
        text.value
    );
}

#[gtest]
fn asymmetric_fields_review_hover_does_not_borrow_another_field_description() {
    let mut ws = ProviderVirtualWorkspace::new();
    ws.def_file(
        "defs.lua",
        "---@class Property\n---@field other number # Other field documentation.\n---@field (read) value number\n---@field (write) value string\n---@type Property\nproperty = {}",
    );
    let file = ws.def_file("use.lua", "local value = property.value");
    let result = hover(&ws.analysis, file, Position::new(0, 25)).expect("hover");
    let HoverContents::Markup(text) = result.contents else {
        panic!("markup");
    };
    assert!(text.value.contains("Read: number"), "{}", text.value);
    assert!(
        !text.value.contains("Other field documentation."),
        "{}",
        text.value
    );
}

#[gtest]
fn asymmetric_fields_review_hover_unicode_inline_and_prefix() {
    let mut ws = ProviderVirtualWorkspace::new();
    let defs = ws.def_file(
        "defs.lua",
        "--- 中文 🦀\n---@class Property\n--- Preceding documentation.\n---@field (read) value number # Inline documentation.\n---@field (write) value string\n---@type Property\nproperty = {}",
    );
    let file = ws.def_file("use.lua", "local value = property.value");
    let result = hover(&ws.analysis, file, Position::new(0, 25)).expect("hover");
    assert_eq!(result.range.unwrap().start, Position::new(0, 23));
    let HoverContents::Markup(text) = result.contents else {
        panic!("markup");
    };
    // The upstream parser attaches preceding and inline documentation together.
    assert!(
        text.value
            .contains("Preceding documentation.\n\nInline documentation."),
        "{}",
        text.value
    );
    assert!(!text.value.contains("中文"), "{}", text.value);
    let definition =
        definition(&ws.analysis, file, Position::new(0, 18)).expect("prefix definition");
    let locations = match definition {
        GotoDefinitionResponse::Scalar(location) => vec![location],
        GotoDefinitionResponse::Array(locations) => locations,
        _ => panic!("locations"),
    };
    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].range.start.line, 6);
    assert_eq!(locations[0].uri, ws.analysis.get_uri(defs).unwrap());
}

#[gtest]
fn asymmetric_fields_review_assignment_completion_controls() -> Result<()> {
    for (declaration, expected) in [
        ("---@type 'coal'|'wood'\nlocal target", "\"coal\""),
        (
            "---@alias Callback fun(value: string)\n---@type Callback\nlocal target",
            "function(value) end",
        ),
        (
            "---@type (fun(value: string))|'coal'\nlocal target",
            "function(value) end",
        ),
    ] {
        let mut ws = ProviderVirtualWorkspace::new();
        let (source, position) = ProviderVirtualWorkspace::handle_file_content(&format!(
            "{declaration}\ntarget = <??>"
        ))?;
        let file = ws.def(&source);
        let response = completion(
            &ws.analysis,
            file,
            position,
            CompletionTriggerKind::INVOKED,
            CancellationToken::new(),
        )
        .expect("completion");
        let items = match response {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };
        assert_eq!(
            items.iter().filter(|item| item.label == expected).count(),
            1,
            "{items:?}"
        );
    }
    Ok(())
}

#[gtest]
fn asymmetric_fields_review_hover_keeps_field_types_separate() {
    for (field, directional) in [
        ("---@field (read) va<??>lue Value", true),
        ("---@field (write) va<??>lue Value", true),
        ("---@field (read) value Val<??>ue", false),
        ("---@field (write) value Val<??>ue", false),
    ] {
        let mut ws = ProviderVirtualWorkspace::new();
        let (source, position) = ProviderVirtualWorkspace::handle_file_content(&format!(
            "---@class Value\n---@field name string\n---@class Property\n{field}"
        ))
        .expect("position");
        let file = ws.def_file("defs.lua", &source);
        let result = hover(&ws.analysis, file, position).expect("hover");
        let HoverContents::Markup(text) = result.contents else {
            panic!("markup")
        };
        assert_eq!(
            text.value.contains("Read:"),
            directional,
            "{field}: {}",
            text.value
        );
        assert!(text.value.contains("Value"), "{}", text.value);
    }
}

#[gtest]
fn asymmetric_fields_review_assignment_completion_providers() {
    for (typ, expected) in [
        ("'coal'|'wood'", vec!["coal", "wood"]),
        ("Choice", vec!["coal", "wood"]),
        ("42|43", vec!["42", "43"]),
        ("Callback", vec!["function(value) end"]),
        ("Callback|'coal'", vec!["function(value) end", "coal"]),
        ("number", vec![]),
        ("unknown", vec![]),
        ("Callable", vec![]),
        ("CallableAlias", vec![]),
    ] {
        for read in [
            "---@field (read) value boolean\n---@field (write)",
            "---@field",
        ] {
            let mut ws = ProviderVirtualWorkspace::new();
            let (source, position) = ProviderVirtualWorkspace::handle_file_content(&format!(
                "---@alias Choice 'coal'|'wood'\n---@alias Callback fun(value: string)\n---@class Callable\n---@field __call fun(self: Callable, value: string)\n---@alias CallableAlias Callable\n---@class Property\n{read} value {typ}\n---@type Property\nlocal property = {{}}\nlocal available = 1\nproperty.value = <??>"
            )).expect("position");
            let file = ws.def_file("use.lua", &source);
            let response = completion(
                &ws.analysis,
                file,
                position,
                CompletionTriggerKind::INVOKED,
                CancellationToken::new(),
            )
            .expect("completion");
            let items = match response {
                CompletionResponse::Array(items) => items,
                CompletionResponse::List(list) => list.items,
            };
            // Ordinary callable targets use the earlier implementation-snippet path.
            // Directional targets reach the shared type dispatch and retain enum alternatives.
            let expected = if read == "---@field" && typ == "Callback|'coal'" {
                vec!["function(value) end"]
            } else {
                expected.clone()
            };
            for label in &expected {
                assert_eq!(
                    items
                        .iter()
                        .filter(|item| item.label.trim_matches('"') == *label)
                        .count(),
                    1,
                    "{read} {typ}: {label}: {items:?}"
                );
            }
            if expected.is_empty() {
                assert!(
                    items.iter().any(|item| item.label == "available"),
                    "{typ}: {items:?}"
                );
                assert!(
                    !items.iter().any(|item| item.label == "function(value) end"),
                    "{typ}: {items:?}"
                );
            }
        }
    }
}

#[gtest]
fn asymmetric_fields_review_assignment_completion_uses_writer_with_callable_getter() {
    for (writer, expected) in [
        ("fun(setter: string)", vec!["function(setter) end"]),
        ("'coal'|'wood'", vec!["coal", "wood"]),
        (
            "(fun(setter: string))|'coal'",
            vec!["function(setter) end", "coal"],
        ),
        ("unknown", vec!["available"]),
    ] {
        let mut ws = ProviderVirtualWorkspace::new();
        let (source, position) = ProviderVirtualWorkspace::handle_file_content(&format!(
            "---@class Property\n---@field (read) value fun(getter: boolean)\n---@field (write) value {writer}\n---@type Property\nlocal property = {{}}\nlocal available = 1\nproperty.value = <??>"
        )).expect("position");
        let file = ws.def_file("use.lua", &source);
        let response = completion(
            &ws.analysis,
            file,
            position,
            CompletionTriggerKind::INVOKED,
            CancellationToken::new(),
        )
        .expect("completion");
        let items = match response {
            CompletionResponse::Array(items) => items,
            CompletionResponse::List(list) => list.items,
        };
        for label in expected {
            assert_eq!(
                items
                    .iter()
                    .filter(|item| item.label.trim_matches('"') == label)
                    .count(),
                1,
                "{writer}: {label}: {items:?}"
            );
        }
        assert!(
            !items
                .iter()
                .any(|item| item.label == "function(getter) end"),
            "{writer}: {items:?}"
        );
    }
}
