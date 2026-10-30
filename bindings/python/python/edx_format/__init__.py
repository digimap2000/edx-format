"""Reader, checker, merger and writer for EDX, the Electronics Design eXchange format.

An EDX document is one JSON file holding a printed circuit board's design, rules, constraints, layout and report.
Documents are plain dictionaries, as ``json.loads`` gives them. The checks, merging and canonical form run in the
Rust reference library, so they agree exactly with every other ``edx-format`` package.

    >>> import edx_format
    >>> result = edx_format.check('{"edx": "1.0"}')
    >>> result.ok
    True
"""

from __future__ import annotations

import json
import os
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional, Tuple, Union

from . import _native

__all__ = [
    "CODES",
    "EDX_VERSION",
    "SCHEMA",
    "CheckResult",
    "Diagnostic",
    "EdxError",
    "check",
    "check_file",
    "dumps",
    "hashes",
    "load",
    "loads",
    "merge",
]

__version__: str = _native.VERSION
#: The EDX version this library reads and writes, ``MAJOR.MINOR``.
EDX_VERSION: str = _native.EDX_VERSION
#: The JSON Schema documents are checked against, as text.
SCHEMA: str = _native.SCHEMA
#: Every diagnostic code, with a one-line summary.
CODES: Dict[str, str] = dict(_native.CODES)

Source = Union[str, bytes, bytearray, memoryview]
Document = Dict[str, Any]


@dataclass(frozen=True)
class Diagnostic:
    """One problem found in a document (specification, section 14.2)."""

    code: str
    severity: str
    pointer: str
    message: str
    line: Optional[int] = None
    column: Optional[int] = None

    @property
    def is_error(self) -> bool:
        return self.severity == "error"

    def __str__(self) -> str:
        at = self.pointer or "(document)"
        text = f"{self.severity} {self.code}: {at}: {self.message}"
        if self.line is not None:
            text += f" (line {self.line}, column {self.column})"
        return text


@dataclass
class CheckResult:
    """The outcome of checking a document: whether it has no errors, every diagnostic, and the document."""

    ok: bool
    diagnostics: List[Diagnostic] = field(default_factory=list)
    #: The document, once it has passed the schema; None before that.
    document: Optional[Document] = None

    @property
    def errors(self) -> List[Diagnostic]:
        return [d for d in self.diagnostics if d.is_error]

    @property
    def warnings(self) -> List[Diagnostic]:
        return [d for d in self.diagnostics if not d.is_error]

    def __bool__(self) -> bool:
        return self.ok


class EdxError(ValueError):
    """A document has errors. ``diagnostics`` holds them all."""

    def __init__(self, diagnostics: List[Diagnostic], message: Optional[str] = None):
        self.diagnostics = diagnostics
        errors = [d for d in diagnostics if d.is_error]
        super().__init__(message or "\n".join(str(d) for d in errors) or "the document cannot be used")


def _result(raw: str) -> CheckResult:
    data = json.loads(raw)
    diagnostics = [Diagnostic(**d) for d in data["diagnostics"]]
    return CheckResult(ok=data["ok"], diagnostics=diagnostics, document=data.get("document"))


def _bytes(source: Union[Source, Document]) -> bytes:
    if isinstance(source, dict):
        return json.dumps(source, ensure_ascii=False).encode()
    if isinstance(source, str):
        return source.encode()
    return bytes(source)


def check(source: Union[Source, Document]) -> CheckResult:
    """Check a document given as JSON text, UTF-8 bytes, or a dictionary.

    Runs all four layers of the specification: parse, version and extensions, schema, semantics.
    """
    return _result(_native.check(_bytes(source)))


def check_file(path: Union[str, "os.PathLike[str]"]) -> CheckResult:
    """Check the document in a file."""
    with open(path, "rb") as f:
        return check(f.read())


def loads(source: Source) -> Document:
    """Read a document from JSON text or bytes, raising EdxError if it has errors. Warnings are ignored."""
    result = check(source)
    if not result.ok or result.document is None:
        raise EdxError(result.diagnostics)
    return result.document


def load(path: Union[str, "os.PathLike[str]"]) -> Document:
    """Read a document from a file, raising EdxError if it has errors."""
    with open(path, "rb") as f:
        return loads(f.read())


def _canonical(document: Union[Source, Document]) -> Tuple[str, Dict[str, str]]:
    data = json.loads(_native.canonical(_bytes(document)))
    if not data["ok"]:
        diagnostics = [Diagnostic(**d) for d in data["diagnostics"]]
        raise EdxError(diagnostics, data.get("reason"))
    return data["canonical"], data["hashes"]


def dumps(document: Union[Document, Source], canonical: bool = False) -> str:
    """Write a document as JSON text: indented by two spaces, numbers in plain decimal, ending with a newline.

    With ``canonical=True`` the document is checked and written in canonical form (specification, section 13.1),
    one spelling per meaning; EdxError is raised if it has errors. The document may then also be JSON text or bytes.
    """
    if canonical:
        return _canonical(document)[0]
    return _native.write(json.dumps(document, ensure_ascii=False))


def hashes(document: Union[Source, Document]) -> Dict[str, str]:
    """The ``sha256:`` hashes of a document's design, rules and constraints, for those it has (section 13.2)."""
    return _canonical(document)[1]


def merge(first: Union[Source, Document], second: Union[Source, Document]) -> CheckResult:
    """Merge two documents by record identity (section 3.2) and check the result.

    Each document is checked first. Conflicts are reported as EDX-E2007. The order of the two does not matter.
    """
    def text(d: Union[Source, Document]) -> str:
        return d if isinstance(d, str) else _bytes(d).decode()

    return _result(_native.merge(text(first), text(second)))
