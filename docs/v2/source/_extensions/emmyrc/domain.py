from __future__ import annotations

import re
import typing as _t
import urllib.parse

from docutils.parsers.rst import directives
from docutils import nodes
from docutils.nodes import Element, Node
from sphinx import addnodes
from sphinx.addnodes import desc_signature, pending_xref
from sphinx.builders import Builder
from sphinx.directives import ObjectDescription
from sphinx.domains import Domain, ObjType
from sphinx.environment import BuildEnvironment
from sphinx.locale import _, __
from sphinx.roles import XRefRole
from sphinx.util import logging
from sphinx.util.nodes import make_id, make_refnode

logger = logging.getLogger("emmyrc")


_TYPE_PARSE_RE = re.compile(
    r"""
    # Skip spaces, they're not meaningful in this context.
    \s+
    |
    (?P<dots>[.]{3})
    |
    # Literal string with escapes.
    # Example: `"foo"`, `"foo-\"-bar"`.
    (?P<string>(?P<string_q>['"`])(?:\\.|[^\\])*?(?P=string_q))
    |
    # Number with optional exponent.
    # Example: `1.0`, `.1`, `1.`, `1e+5`.
    (?P<number>(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?)
    |
    # Built-in type.
    # Example: `string`, `string?`.
    (?P<type>null|true|false|boolean|integer|number|string|any|never|object|set|array)\b
    \s*(?P<type_qm>\??)\s*
    |
    # Name component.
    (?P<name>\w[\w.#-]*)
    |
    # Punctuation that we separate with spaces.
    (?P<punct>[=:,|&])
    |
    # Punctuation that we copy as-is, without adding spaces.
    (?P<other_punct>[-!#$%()*+/;<>?@[\]^_{}~]+)
    |
    # Anything else is copied as-is.
    (?P<other>.)
    """,
    re.VERBOSE,
)


def type_to_nodes(typ: str, inliner) -> list[nodes.Node]:
    res = []

    for match in _TYPE_PARSE_RE.finditer(typ):
        if text := match.group("dots"):
            res.append(addnodes.desc_sig_name(text, text))
        elif text := match.group("string"):
            res.append(addnodes.desc_sig_literal_string(text, text))
        elif text := match.group("number"):
            res.append(addnodes.desc_sig_literal_number(text, text))
        elif text := match.group("type"):
            res.append(addnodes.desc_sig_keyword_type(text, text))
            if qm := match.group("type_qm"):
                res.append(addnodes.desc_sig_punctuation(qm, qm))
        elif text := match.group("name"):
            ref_nodes, warn_nodes = EmmyRcXRefRole()(
                "emmyrc:_auto", text, text, 0, inliner
            )
            res.extend(ref_nodes)
            res.extend(warn_nodes)
        elif text := match.group("punct"):
            if text in "=|&":
                res.append(addnodes.desc_sig_space())
            res.append(addnodes.desc_sig_punctuation(text, text))
            res.append(addnodes.desc_sig_space())
        elif text := match.group("other_punct"):
            res.append(addnodes.desc_sig_punctuation(text, text))
        elif text := match.group("other"):
            res.append(nodes.Text(text))

    return res


def make_anchor(name: str) -> str:
    return urllib.parse.quote(name)


class EmmyRcObject(ObjectDescription[tuple[str, str]]):
    option_spec = {
        "hide-prefix": directives.flag,
        **ObjectDescription.option_spec,
    }

    def get_display_prefix(self) -> list[Node]:
        return []

    def handle_signature(self, sig: str, signode: desc_signature) -> tuple[str, str]:
        sig = sig.replace("/$defs/", "").strip()

        prefix = self.env.ref_context.get("emmyrc:object", "")

        if self.objtype == "property":
            member_prefix, member_name, sym, tail = "", sig, "", ""
        else:
            if ":" in sig or "=" in sig:
                member, sym, tail = re.split(r"([:=])", sig, maxsplit=1)
            else:
                member, sym, tail = sig, "", ""
            member = member.strip()
            tail = tail.strip()

            try:
                member_prefix, member_name = member.rsplit(".", 1)
            except ValueError:
                member_name = member
                member_prefix = ""
            if prefix and member_prefix:
                prefix = f"{prefix}.{member_prefix}"
            elif member_prefix:
                prefix = member_prefix

        if prefix:
            fullname = f"{prefix}.{member_name}"
        else:
            fullname = member_name

        signode["object"] = prefix
        signode["fullname"] = fullname

        display_prefix = self.get_display_prefix()
        if display_prefix:
            signode += addnodes.desc_annotation("", "", *display_prefix)

        if member_prefix and "hide-prefix" not in self.options:
            add_name = addnodes.desc_addname("", "")
            for p in member_prefix.split("."):
                add_name += addnodes.desc_sig_name(p, p)
                add_name += addnodes.desc_sig_punctuation(".", ".")
            signode += add_name
        if self.objtype == "property":
            signode += addnodes.desc_name(
                "", "", *type_to_nodes(member_name, self.state.inliner)
            )
        else:
            signode += addnodes.desc_name(
                "", "", addnodes.desc_sig_name(member_name, member_name)
            )
        if type:
            if sym == "=":
                signode += addnodes.desc_sig_space()
            signode += addnodes.desc_sig_punctuation(sym, sym)
            signode += addnodes.desc_sig_space()
            signode += addnodes.desc_type(
                "", "", *type_to_nodes(tail, self.state.inliner)
            )
        return fullname, prefix

    def _object_hierarchy_parts(self, sig_node: desc_signature) -> tuple[str, ...]:
        if "fullname" not in sig_node:
            return ()
        return tuple(sig_node["fullname"].split("."))

    def add_target_and_index(
        self, name_obj: tuple[str, str], sig: str, signode: desc_signature
    ) -> None:
        fullname = name_obj[0]
        anchor = make_anchor(fullname)

        if anchor not in self.state.document.ids:
            signode["names"].append(anchor)
            signode["ids"].append(anchor)
            signode["first"] = not self.names
            self.state.document.note_explicit_target(signode)

            domain = self.env.domains["emmyrc"]
            assert isinstance(domain, EmmyRcDomain)
            domain.note_object(fullname, self.objtype, anchor, location=signode)

        if "no-index-entry" not in self.options:
            if index_text := self.get_index_text("", name_obj):
                self.indexnode["entries"].append(
                    (
                        "single",
                        index_text,
                        anchor,
                        "",
                        None,
                    )
                )

    def get_index_text(self, objectname: str, name_obj: tuple[str, str]) -> str:
        name, obj = name_obj
        if obj:
            return _("%s (%s attribute)") % (name, obj)
        else:
            return _("%s (global variable or constant)") % name

    def before_content(self) -> None:
        if self.names:
            fullname, _ = self.names[-1]
            objects = self.env.ref_context.setdefault("emmyrc:objects", [])
            objects.append(self.env.ref_context.get("emmyrc:object"))
            if fullname:
                self.env.ref_context["emmyrc:object"] = fullname
            else:
                self.env.ref_context.pop("emmyrc:object", None)

    def after_content(self) -> None:
        if self.names:
            objects = self.env.ref_context.setdefault("emmyrc:objects", [])
            if objects:
                self.env.ref_context["emmyrc:object"] = objects.pop()
            else:
                self.env.ref_context.pop("emmyrc:object", None)

    def _toc_entry_name(self, sig_node: desc_signature) -> str:
        if not sig_node.get("_toc_parts"):
            return ""

        config = self.config
        *parents, name = sig_node["_toc_parts"]
        if config.toc_object_entries_show_parents == "domain":
            return sig_node.get("fullname", name)
        if config.toc_object_entries_show_parents == "hide":
            return name
        if config.toc_object_entries_show_parents == "all":
            return ".".join([*parents, name])
        return ""


class EmmyRcXRefRole(XRefRole):
    def process_link(
        self,
        env: BuildEnvironment,
        refnode: Element,
        has_explicit_title: bool,
        title: str,
        target: str,
    ) -> tuple[str, str]:
        refnode["emmyrc:object"] = env.ref_context.get("emmyrc:object")
        if not has_explicit_title:
            title = title.lstrip(".")
            target = target.lstrip("~")
            if title[0:1] == "~":
                title = title[1:]
                dot = title.rfind(".")
                if dot != -1:
                    title = title[dot + 1 :]
        if target[0:1] == ".":
            target = target[1:]
            refnode["refspecific"] = True
        return title, target


class EmmyRcDomain(Domain):
    name = "emmyrc"
    label = "EmmyLua Config"
    object_types = {
        "object": ObjType(_("object"), "obj", "_auto"),
        "property": ObjType(_("property"), "obj", "_auto"),
    }
    directives = {
        "object": EmmyRcObject,
        "property": EmmyRcObject,
    }
    roles = {
        "obj": EmmyRcXRefRole(),
        "_auto": EmmyRcXRefRole(),
    }
    initial_data: dict[str, dict[str, tuple[str, str]]] = {
        "objects": {},  # fullname -> docname, node_id, objtype
    }

    @property
    def objects(self) -> dict[str, tuple[str, str, str]]:
        # fullname -> docname, node_id, objtype
        return self.data.setdefault("objects", {})

    def note_object(
        self, fullname: str, objtype: str, node_id: str, location: _t.Any = None
    ) -> None:
        if fullname in self.objects:
            docname = self.objects[fullname][0]
            logger.warning(
                __("duplicate %s description of %s, other %s in %s"),
                objtype,
                fullname,
                objtype,
                docname,
                location=location,
            )
        self.objects[fullname] = (self.env.docname, node_id, objtype)

    def clear_doc(self, docname: str) -> None:
        for fullname, (pkg_docname, _node_id, _l) in list(self.objects.items()):
            if pkg_docname == docname:
                del self.objects[fullname]

    def merge_domaindata(
        self, docnames: set[str], otherdata: dict[str, _t.Any]
    ) -> None:
        for fullname, (fn, node_id, objtype) in otherdata["objects"].items():
            if fn in docnames:
                self.objects[fullname] = (fn, node_id, objtype)

    def find_obj(
        self,
        env: BuildEnvironment,
        prefix: str,
        name: str,
        typ: str | None,
        searchorder: int = 0,
    ) -> tuple[str | None, tuple[str, str, str] | None]:
        searches = []
        if prefix:
            searches.append(f"{prefix}.{name}")
        searches.append(name)

        if searchorder == 0:
            searches.reverse()

        newname = None
        object = None
        for search_name in searches:
            if search_name in self.objects:
                newname = search_name
                object = self.objects[search_name]

        return newname, object

    def resolve_xref(
        self,
        env: BuildEnvironment,
        fromdocname: str,
        builder: Builder,
        typ: str,
        target: str,
        node: pending_xref,
        contnode: Element,
    ) -> nodes.reference | None:
        prefix = node.get("emmyrc:object")
        searchorder = 1 if node.hasattr("refspecific") else 0
        name, obj = self.find_obj(env, prefix, target, typ, searchorder)
        if not obj:
            return None
        return make_refnode(builder, fromdocname, obj[0], obj[1], contnode, name)

    def resolve_any_xref(
        self,
        env: BuildEnvironment,
        fromdocname: str,
        builder: Builder,
        target: str,
        node: pending_xref,
        contnode: Element,
    ) -> list[tuple[str, nodes.reference]]:
        prefix = node.get("emmyrc:object")
        name, obj = self.find_obj(env, prefix, target, None, 1)
        if not obj:
            return []
        return [
            (
                f"emmyrc:{self.role_for_objtype(obj[2])}",
                make_refnode(builder, fromdocname, obj[0], obj[1], contnode, name),
            )
        ]

    def get_objects(self) -> _t.Iterator[tuple[str, str, str, str, str, int]]:
        for refname, (docname, node_id, typ) in list(self.objects.items()):
            yield refname, refname, typ, docname, node_id, 1

    def get_full_qualified_name(self, node: Element) -> str | None:
        prefix = node.get("emmyrc:object")
        target = node.get("reftarget")
        if target is None:
            return None
        else:
            return ".".join(filter(None, [prefix, target]))
