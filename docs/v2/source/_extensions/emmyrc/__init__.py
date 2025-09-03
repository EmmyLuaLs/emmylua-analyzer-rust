from __future__ import annotations

from emmyrc.auto import AutodocDirective, ExampleValue
from emmyrc.domain import EmmyRcDomain
from sphinx.application import Sphinx


def suppress_auto_ref_warnings(app, domain, node):
    if node["refdomain"] == "emmyrc" and node["reftype"] == "_auto":
        return True


def setup(app: Sphinx):
    app.add_domain(EmmyRcDomain)
    app.add_directive_to_domain("emmyrc", "auto", AutodocDirective)
    app.add_directive_to_domain("emmyrc", "auto-example", ExampleValue)
    app.add_config_value("emmylua_schema", None, "html", str)
    app.connect("warn-missing-reference", suppress_auto_ref_warnings)
    return {
        "parallel_read_safe": True,
        "parallel_write_safe": True,
    }
