use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EmmyrcDoc {
    /// List of glob patterns that enable treating specific field names as private.
    ///
    /// For example, `m_*` would make fields `Type.m_id` and `Type.m_type` private.
    #[serde(default)]
    pub private_name: Vec<String>,

    /// List of known documentation tags.
    #[serde(default)]
    pub known_tags: Vec<String>,

    /// Syntax for highlighting documentation.
    #[serde(default)]
    pub syntax: DocSyntax,

    /// When `syntax` is `Myst` or `Rst`, specifies [primary domain] used
    /// with RST processor.
    ///
    /// [primary domain]: https://www.sphinx-doc.org/en/master/usage/configuration.html#confval-primary_domain
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rst_primary_domain: Option<String>,

    /// When `syntax` is `Myst` or `Rst`, specifies [default role] used
    /// with RST processor.
    ///
    /// [default role]: https://www.sphinx-doc.org/en/master/usage/configuration.html#confval-default_role
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rst_default_role: Option<String>,
}

impl Default for EmmyrcDoc {
    fn default() -> Self {
        Self {
            private_name: Default::default(),
            known_tags: Default::default(),
            syntax: Default::default(),
            rst_primary_domain: None,
            rst_default_role: None,
        }
    }
}

/// Syntax for highlighting documentation.
#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(rename_all = "kebab-case")]
pub enum DocSyntax {
    /// Documentation is not highlighted.
    None,

    /// Documentation is highlighted as plain [MarkDown].
    ///
    /// [MarkDown]: https://commonmark.org/
    Md,

    /// Documentation is highlighted as [MySt], a MarkDown plugin for [Sphinx].
    ///
    /// Enables Autocompletion and Go To Definition for sphinx cross-references.
    ///
    /// [MySt]: https://myst-parser.readthedocs.io/
    /// [Sphinx]: https://www.sphinx-doc.org/
    Myst,

    /// Documentation is highlighted as [ReStructured Text].
    ///
    /// Enables Autocompletion and Go To Definition for [sphinx] cross-references.
    ///
    /// [ReStructured Text]: https://www.sphinx-doc.org/en/master/usage/restructuredtext/basics.html
    /// [sphinx]: https://www.sphinx-doc.org/
    Rst,
}

impl Default for DocSyntax {
    fn default() -> Self {
        DocSyntax::Md
    }
}
