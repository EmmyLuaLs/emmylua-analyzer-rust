import contextlib
import dataclasses
import json
import typing as _t
from typing import Set, Tuple

import docutils.nodes
import docutils.statemachine
import sphinx.addnodes
import sphinx.directives
import sphinx.directives.code
import sphinx_immaterial.code_annotations
from docutils.parsers.rst import directives
from emmyrc.domain import EmmyRcObject
from emmyrc.loader import (
    UNKNOWN,
    AllOf,
    Any,
    Array,
    Boolean,
    Enum,
    Integer,
    Loader,
    Null,
    Number,
    Object,
    OneOf,
    Ref,
    String,
    Type,
    load_jsons,
)
from sphinx.util import logging
from sphinx.util.docutils import SphinxDirective

logger = logging.getLogger("emmyrc")


def parse_list_option(value: str | None):
    if not value:
        return []
    else:
        return value.split(",")


class AutoEmmyRcObject(EmmyRcObject):
    def __init__(
        self,
        *args,
        uri: str,
        path: str,
        root: Type,
        loader: Loader,
        rendered: set[str],
        **kwargs,
    ) -> None:
        super().__init__(*args, **kwargs)

        self.uri = uri
        self.path = path
        self.root = root
        self.loader = loader
        self.rendered = rendered

    def run(self) -> list[docutils.nodes.Node]:
        if (self.uri, self.path) in self.options.get("exclude", []):
            return []
        self.rendered.add(self.root.id)
        return super().run()

    def transform_content(self, content_node: sphinx.addnodes.desc_content) -> None:
        if self.root.description:
            content_node += self.parse_text_to_nodes(self.root.description)
        if "recursive" in self.options:
            content_node += self.render_children(self.root)

    def render_children(self, root: Type) -> list[docutils.nodes.Node]:
        root = root.unwrap_optional()
        if isinstance(root, Object) and root.named_children:
            return self.render_object_children(root)
        elif isinstance(root, Enum) and root.items:
            return self.render_enum_children(root)
        else:
            nodes = []
            for ref in root.find_refs():
                if self.loader.can_inline(ref.ref):
                    nodes.extend(self.render_children(self.loader.load(ref.ref)))
            return nodes

    def render_object_children(self, root: Object) -> list[docutils.nodes.Node]:
        nodes = []
        default = root.default
        if not isinstance(default, dict):
            default = {}
        for name, type in root.named_children.items():
            unwrapped = type.unwrap_optional()
            if isinstance(unwrapped, Ref) and self.loader.can_inline(unwrapped.ref):
                ref = self.loader.load(unwrapped.ref) or Any()
                type = dataclasses.replace(
                    ref,
                    title=type.title or ref.title,
                    description=type.description or ref.description,
                    deprecated=type.deprecated or ref.deprecated,
                    required=type.required or ref.required,
                )
            type = dataclasses.replace(type, default=default.get(name, type.default))
            nodes.extend(self.render_child(name, type))
        return nodes

    def render_enum_children(self, root: Enum) -> list[docutils.nodes.Node]:
        nodes = []

        for type in root.items:
            name = type.print(self.loader)
            nodes.extend(
                AutoEmmyRcObject(
                    "emmyrc:property",
                    [name],
                    self.options,
                    self.content,
                    self.lineno,
                    self.content_offset,
                    self.block_text,
                    self.state,
                    self.state_machine,
                    uri=self.uri,
                    path=".".join(filter(None, [self.path, name])),
                    root=type,
                    loader=self.loader,
                    rendered=self.rendered,
                ).run()
            )

        return nodes

    def render_child(self, name: str, root: Type) -> list[docutils.nodes.Node]:
        arg = name
        unwrapped_root = root.unwrap_optional()
        if not (isinstance(unwrapped_root, Object) and unwrapped_root.named_children):
            if self.loader.can_inline(root.id) and not isinstance(unwrapped_root, Enum):
                arg += f": {root.print(self.loader)}"
            if root.default is not UNKNOWN:
                arg += f" = {json.dumps(root.default)}"
            elif root.is_optional():
                arg += f" = null"

        return AutoEmmyRcObject(
            "emmyrc:object",
            [arg],
            self.options,
            self.content,
            self.lineno,
            self.content_offset,
            self.block_text,
            self.state,
            self.state_machine,
            uri=self.uri,
            path=".".join(filter(None, [self.path, name])),
            root=root,
            loader=self.loader,
            rendered=self.rendered,
        ).run()

    def get_display_prefix(self) -> list[docutils.nodes.Node]:
        prefix = []
        if self.root.required:
            prefix.append(sphinx.addnodes.desc_sig_keyword("required", "required"))
            prefix.append(sphinx.addnodes.desc_sig_space())
        return prefix


class AutodocDirective(SphinxDirective):
    required_arguments = 1
    optional_arguments = 0
    option_spec = {
        "recursive": directives.flag,
        "unwrap": directives.flag,
        "title": directives.unchanged,
        "exclude": parse_list_option,
        **EmmyRcObject.option_spec,
    }
    has_content = True

    def run(self) -> list[docutils.nodes.Node]:
        loader = Loader(load_jsons(self.env.app))
        root = loader.load(self.arguments[0])

        excludes = set()
        exclude: str
        for exclude in self.options.get("exclude", []):
            parts = exclude.strip().split(maxsplit=1)
            match len(parts):
                case 1:
                    excludes.add((loader.load(parts[0]).id, ""))
                case 2:
                    excludes.add((loader.load(parts[0]).id, parts[1]))
                case _:
                    pass
        self.options["exclude"] = excludes

        rendered = set()

        nodes = self.render_root(True, loader, root, rendered)

        if "recursive" not in self.options:
            return nodes

        title = "Config types"
        section = docutils.nodes.section("", names=[])
        section["name"] = docutils.nodes.fully_normalize_name(title)
        section["names"].append(section["name"])
        section += docutils.nodes.title("", title)
        for uri, type in loader.get_all_loaded():
            if uri not in rendered and not loader.can_inline(uri):
                section += self.render_root(False, loader, type, rendered)

        if len(section) > 1:
            nodes.append(section)
            self.state.document.note_implicit_target(section, section)

        return nodes

    def render_root(
        self, top_level: bool, loader: Loader, root: Type, rendered: set[str]
    ) -> list[docutils.nodes.Node]:
        if (
            "unwrap" in self.options
            and isinstance(root, Object)
            and root.named_children
        ):
            return self.render_unwrapped(top_level, loader, root, rendered)
        else:
            return self.render_wrapped(
                root.id, top_level, loader, root, root.id, rendered, ""
            )

    def render_unwrapped(
        self, top_level: bool, loader: Loader, root: Object, rendered: set[str]
    ) -> list[docutils.nodes.Node]:
        if (root.id, "") in self.options.get("exclude", []):
            return []

        rendered.add(root.id)

        nodes = []

        if top_level:
            nodes.extend(self.parse_content_to_nodes())
        if root.description:
            nodes.extend(self.parse_text_to_nodes(root.description))

        default = root.default
        if not isinstance(default, dict):
            default = {}
        for name, type in root.named_children.items():
            unwrapped = type.unwrap_optional()
            if isinstance(unwrapped, Ref) and loader.can_inline(unwrapped.ref):
                ref = loader.load(unwrapped.ref) or Any()
                type = dataclasses.replace(
                    ref,
                    title=type.title or ref.title,
                    description=type.description or ref.description,
                    deprecated=type.deprecated or ref.deprecated,
                    required=type.required or ref.required,
                    default=default.get(name, type.default),
                )
            arg = f"{root.id}.{name}"
            unwrapped = type.unwrap_optional()
            if not (isinstance(unwrapped, Object) and unwrapped.named_children):
                if loader.can_inline(root.id) and not isinstance(unwrapped, Enum):
                    arg += f": {root.print(loader)}"
                if root.default is not UNKNOWN:
                    arg += f" = {json.dumps(root.default)}"
            nodes.extend(self.render_wrapped(
                arg, False, loader, type, root.id, rendered, name
            ))

        return nodes

    def render_wrapped(
        self,
        arg: str,
        top_level: bool,
        loader: Loader,
        root: Type,
        uri: str,
        rendered: set[str],
        path: str,
    ) -> list[docutils.nodes.Node]:
        return AutoEmmyRcObject(
            "emmyrc:object",
            [arg],
            self.options,
            self.content if top_level else docutils.statemachine.StringList(),
            self.lineno if top_level else 0,
            self.content_offset if top_level else 0,
            self.block_text if top_level else "",
            self.state,
            self.state_machine,
            uri=uri,
            path=path,
            root=root,
            loader=loader,
            rendered=rendered,
        ).run()


class ExampleValue(SphinxDirective):
    required_arguments = 1
    optional_arguments = 0
    option_spec = {
        "kind": lambda value: directives.choice(value, ["all", "user", "project"])
    }

    def run(self) -> list[docutils.nodes.Node]:
        self.loader = Loader(load_jsons(self.env.app))
        root = self.loader.load(self.arguments[0])

        self.lines: list[str] = []
        self.chunks: list[str] = []
        self.indent = 0

        self.render_type(root, root.default)
        self.newline()

        return sphinx.directives.code.CodeBlock(
            "code-block",
            ["json"],
            {},
            docutils.statemachine.StringList(self.lines),
            self.lineno,
            self.content_offset,
            self.block_text,
            self.state,
            self.state_machine,
        ).run()

    @contextlib.contextmanager
    def indented(self):
        self.indent += 1
        yield
        self.indent -= 1

    def newline(self):
        self.lines.append("".join(self.chunks))
        self.chunks.clear()
        self.chunks.append("  " * self.indent)

    def render_type(self, type: Type, default: _t.Any):
        if default is UNKNOWN:
            default = type.default
        if isinstance(type, Object):
            if not isinstance(default, dict):
                default = {}
            self.chunks.append("{")
            sep = ""
            with self.indented():
                for name, type in type.named_children.items():
                    if name.startswith("$") or not self.filter(type):
                        continue
                    self.chunks.append(sep)
                    self.newline()
                    self.chunks.append(json.dumps(name))
                    self.chunks.append(": ")
                    self.render_type(type, default.get(name, type.default))
                    sep = ","
            if sep:
                self.newline()
            self.chunks.append("}")
        elif isinstance(type, Ref):
            self.render_type(self.loader.load(type.ref), default)
        elif default is not UNKNOWN:
            self.chunks.append(json.dumps(default))
        elif type.const is not UNKNOWN:
            self.chunks.append(json.dumps(type.const))
        elif isinstance(type, Array):
            self.chunks.append("[]")
        elif isinstance(type, Set):
            self.chunks.append("[]")
        elif isinstance(type, Tuple):
            self.chunks.append("[]")
        elif isinstance(type, Null):
            self.chunks.append("null")
        elif isinstance(type, Boolean):
            self.chunks.append("false")
        elif isinstance(type, Integer):
            self.chunks.append("0")
        elif isinstance(type, Number):
            self.chunks.append("0.0")
        elif isinstance(type, String):
            self.chunks.append('""')
        else:
            self.chunks.append("null")

    def filter(self, type: Type) -> bool:
        match self.options.get("kind", "all"):
            case "user":
                return type.has_user_settings
            case "project":
                return type.has_project_settings
            case _:
                return True
