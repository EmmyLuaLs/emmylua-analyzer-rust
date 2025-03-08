use lsp_types::CompletionItemKind;

pub struct KeywordCompletionInfo {
    pub label: &'static str,
    pub detail: &'static str,
    pub insert_text: &'static str,
    pub kind: CompletionItemKind,
}
pub const KEYWORD_COMPLETIONS: &[KeywordCompletionInfo] = &[
    KeywordCompletionInfo {
        label: "if",
        detail: " (if condition then .. end)",
        insert_text: "if ${1:condition} then\n\t${0}\nend",
        kind: CompletionItemKind::SNIPPET,
    },
    KeywordCompletionInfo {
        label: "else",
        detail: " (else ..)",
        insert_text: "else\n\t${0}",
        kind: CompletionItemKind::SNIPPET,
    },
    KeywordCompletionInfo {
        label: "else",
        detail: " (else .. end)",
        insert_text: "else\n\t${0}\nend",
        kind: CompletionItemKind::SNIPPET,
    },
    KeywordCompletionInfo {
        label: "elseif",
        detail: " (elseif condition then .. )",
        insert_text: "elseif ${1:condition} then\n\t${0}",
        kind: CompletionItemKind::SNIPPET,
    },
    KeywordCompletionInfo {
        label: "then",
        detail: " (then .. )",
        insert_text: "then\n\t${0}",
        kind: CompletionItemKind::SNIPPET,
    },
    KeywordCompletionInfo {
        label: "then",
        detail: " (then .. end)",
        insert_text: "then\n\t${0}\nend",
        kind: CompletionItemKind::SNIPPET,
    },
    KeywordCompletionInfo {
        label: "end",
        detail: "",
        insert_text: "end",
        kind: CompletionItemKind::KEYWORD,
    },
    KeywordCompletionInfo {
        label: "fori",
        detail: " (for i = 1, finish do .. end)",
        insert_text: "for ${1:i} = ${2:1}, ${3:finish} do\n\t${0}\nend",
        kind: CompletionItemKind::SNIPPET,
    },
    KeywordCompletionInfo {
        label: "forp",
        detail: " (for k,v in pairs(table) do .. end)",
        insert_text: "for ${1:k}, ${2:v} in pairs(${3:table}) do\n\t${0}\nend",
        kind: CompletionItemKind::SNIPPET,
    },
    KeywordCompletionInfo {
        label: "forip",
        detail: " (for i, v in ipairs(table) do .. end)",
        insert_text: "for ${1:i},${2:v} in ipairs(${3:table}) do\n\t${0}\nend",
        kind: CompletionItemKind::SNIPPET,
    },
    KeywordCompletionInfo {
        label: "in",
        detail: " (in pairs(table) do .. end)",
        insert_text: "in pairs(${1:table}) do\n\t${0}\nend",
        kind: CompletionItemKind::SNIPPET,
    },
    KeywordCompletionInfo {
        label: "do",
        detail: " (do .. end)",
        insert_text: "do\n\t${0}\nend",
        kind: CompletionItemKind::SNIPPET,
    },
    KeywordCompletionInfo {
        label: "while",
        detail: " (while condition do .. end)",
        insert_text: "while ${1:condition} do\n\t${0}\nend",
        kind: CompletionItemKind::SNIPPET,
    },
    KeywordCompletionInfo {
        label: "repeat",
        detail: " (repeat .. until condition)",
        insert_text: "repeat\n\t${0}\nuntil ${1:condition}",
        kind: CompletionItemKind::SNIPPET,
    },
    KeywordCompletionInfo {
        label: "until",
        detail: " (until condition)",
        insert_text: "until ${1:condition}",
        kind: CompletionItemKind::SNIPPET,
    },
    KeywordCompletionInfo {
        label: "break",
        detail: "",
        insert_text: "break",
        kind: CompletionItemKind::KEYWORD,
    },
    KeywordCompletionInfo {
        label: "return",
        detail: "",
        insert_text: "return",
        kind: CompletionItemKind::KEYWORD,
    },
    KeywordCompletionInfo {
        label: "function",
        detail: " (function name(...) .. end)",
        insert_text: "function ${1:name}(${2:...})\n\t${0}\nend",
        kind: CompletionItemKind::SNIPPET,
    },
    KeywordCompletionInfo {
        label: "local",
        detail: "",
        insert_text: "local",
        kind: CompletionItemKind::KEYWORD,
    },
    KeywordCompletionInfo {
        label: "local function",
        detail: " (local function name(...) .. end)",
        insert_text: "local function ${1:name}(${2:...})\n\t${0}\nend",
        kind: CompletionItemKind::SNIPPET,
    },
    KeywordCompletionInfo {
        label: "goto",
        detail: " (goto label)",
        insert_text: "goto ${1:label}",
        kind: CompletionItemKind::SNIPPET,
    },
];

pub const KEYWORD_EXPR_COMPLETIONS: &[KeywordCompletionInfo] = &[
    KeywordCompletionInfo {
        label: "and",
        detail: "",
        insert_text: "and",
        kind: CompletionItemKind::KEYWORD,
    },
    KeywordCompletionInfo {
        label: "or",
        detail: "",
        insert_text: "or",
        kind: CompletionItemKind::KEYWORD,
    },
    KeywordCompletionInfo {
        label: "not",
        detail: "",
        insert_text: "not",
        kind: CompletionItemKind::KEYWORD,
    },
    KeywordCompletionInfo {
        label: "true",
        detail: "",
        insert_text: "true",
        kind: CompletionItemKind::CONSTANT,
    },
    KeywordCompletionInfo {
        label: "false",
        detail: "",
        insert_text: "false",
        kind: CompletionItemKind::CONSTANT,
    },
    KeywordCompletionInfo {
        label: "nil",
        detail: "",
        insert_text: "nil",
        kind: CompletionItemKind::CONSTANT,
    },
    KeywordCompletionInfo {
        label: "function",
        detail: "(function (...) .. end)",
        insert_text: "function(${1:...})\n\t${0}\nend",
        kind: CompletionItemKind::SNIPPET,
    },
    KeywordCompletionInfo {
        label: "and or",
        detail: "(a and b or c)",
        insert_text: "${1:a} and ${2:b} or ${3:c}",
        kind: CompletionItemKind::SNIPPET,
    },
];
