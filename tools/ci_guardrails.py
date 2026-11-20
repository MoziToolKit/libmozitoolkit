#!/usr/bin/env python3
"""
libmozitoolkit Architectural Guardrails (CI & Local Gate).

Enforces the Golden Rules in `libmozitoolkit/GEMINI.md`:
1. Module Privacy (Rule 2): Internal modules in `crates/*/src/lib.rs` and `bindings/mtk-py/src/lib.rs`
   must be private `mod` (except explicitly whitelisted domain namespaces).
2. Minimal Binding Surface (Rule 2): `bindings/mtk-py/src/lib.rs` must not expose `pub use` to external Rust crates.
3. No Dead Root Exports (Rule 2 & 9): Every `pub use` symbol in core crates and adapters must have at least
   one non-test consumer outside its defining crate.
4. Panic Defense in Bindings/Adapters (Rule 8): Non-test code in `bindings/mtk-py` and `adapters/` must not
   call `.unwrap()` or `.expect()`.
5. Documentation Drift Gate (Rule 10): Public documentation (`README.md`, `docs/*.md`, `crates/*/README.md`,
   `bindings/*/README.md`) must not advertise retired/deleted symbols as active APIs.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import sys

LIB_ROOT = Path(__file__).resolve().parents[1]

# Explicitly permitted `pub mod` declarations in crate roots:
ALLOWED_PUB_MOD = {
    "crates/mtk-core/src/lib.rs": {"constants", "threading"},
    "crates/mtk-resource/src/lib.rs": {"banner", "skin"},
    "crates/mtk-sync/src/lib.rs": {"protocol", "server"},
}

# Symbols in crate root `pub use` that are intentionally exported for CLI / external contract
# or PyO3/WASM boundary even if only referenced via macro/trait/type inference:
ALLOWED_ROOT_EXPORTS = {
    "mtk-core": set(),
    "mtk-cull": set(),
    "mtk-model": set(),
    "mtk-voxel": set(),
    "mtk-sync": set(),
    "mtk-resource": set(),
    "mtk-texture": set(),
    "mtk-material": set(),
    "mtk-save": set(),
    "mtk-package": set(),
    "libmtk": set(),
    "mtk-host-memory": set(),
    "mtk-blender-dna": set(),
}

# Retired symbols that must not be documented as current APIs in reference docs
# (historical audit docs like API_SURFACE_AUDIT.md and CHANGELOG/TODO are excluded).
FORBIDDEN_DOC_SYMBOLS = (
    "with_capacity(",
    "get_flat_secondary_uvs",
    "face_tint_indices_memoryview",
    "is_face_occluded(",
    "cache_len",
    "batch_analyze_transparent_faces_u8",
    "blend_biome_colors",
    "sample_colormap_pixel",
    "srgb_to_linear",
    "linear_to_srgb",
    "remap_mesh_multi_uvs_parallel",
    "cull_faces(",
    "loop_starts_memoryview",
    "loop_totals_memoryview",
    "read_point_cloud(",
    "read_models(",
    "read_atlas(",
    "compute_biome_tint_attributes(",
)


def check_pub_mod_and_mtk_py_pub_use() -> list[str]:
    errors: list[str] = []
    lib_files = list((LIB_ROOT / "crates").glob("*/src/lib.rs")) + [
        LIB_ROOT / "bindings/mtk-py/src/lib.rs",
        LIB_ROOT / "adapters/mtk-host-memory/src/lib.rs",
        LIB_ROOT / "adapters/blender/mtk-blender-dna/src/lib.rs",
        LIB_ROOT / "adapters/blender/mtk-blender-dna-py/src/lib.rs",
    ]
    pub_mod_re = re.compile(r"^\s*pub\s+mod\s+([a-zA-Z0-9_]+)\s*;")
    pub_use_re = re.compile(r"^\s*pub\s+use\s+")

    for lib_rs in sorted(lib_files):
        if not lib_rs.exists():
            continue
        rel = lib_rs.relative_to(LIB_ROOT).as_posix()
        allowed_mods = ALLOWED_PUB_MOD.get(rel, set())
        lines = lib_rs.read_text(encoding="utf-8").splitlines()
        for lineno, line in enumerate(lines, 1):
            m = pub_mod_re.match(line)
            if m:
                mod_name = m.group(1)
                if mod_name not in allowed_mods:
                    errors.append(
                        f"{rel}:{lineno}: forbidden `pub mod {mod_name};` (Rule 2: internal modules must be private `mod`)"
                    )
            if rel == "bindings/mtk-py/src/lib.rs" and pub_use_re.match(line):
                errors.append(
                    f"{rel}:{lineno}: forbidden `pub use` in mtk-py root (Rule 2: use `pub(crate) use`)"
                )
    return errors


def _extract_pub_Items_from_lib(lib_rs: Path) -> set[str]:
    text = lib_rs.read_text(encoding="utf-8")
    # Strip #[cfg(test)] mod tests... at the end
    test_idx = text.find("#[cfg(test)]")
    if test_idx != -1:
        text = text[:test_idx]

    symbols: set[str] = set()
    # Match pub fn / pub unsafe fn / pub struct / pub enum / pub type / pub const
    for m in re.finditer(
        r"^\s*pub\s+(?:unsafe\s+)?(?:fn|struct|enum|type|const|trait)\s+([A-Za-z0-9_]+)",
        text,
        re.MULTILINE,
    ):
        symbols.add(m.group(1))

    # Match pub use ...
    pub_use_blocks = re.findall(r"pub\s+use\s+([^;]+);", text, re.DOTALL)
    for block in pub_use_blocks:
        if "{" in block:
            inner = block[block.find("{") + 1 : block.rfind("}")]
            for part in inner.split(","):
                part = part.strip()
                if not part:
                    continue
                if " as " in part:
                    sym = part.split(" as ")[-1].strip()
                else:
                    sym = part.split("::")[-1].strip()
                if sym and sym != "*" and sym != "self":
                    symbols.add(sym)
        else:
            part = block.strip()
            if " as " in part:
                sym = part.split(" as ")[-1].strip()
            else:
                sym = part.split("::")[-1].strip()
            if sym and sym != "*" and sym != "self":
                symbols.add(sym)
    return symbols


def check_dead_rust_exports() -> list[str]:
    errors: list[str] = []
    # Collect all non-test rust sources per crate/package
    all_rs_files: list[tuple[str, Path, str]] = []
    for root_dir in ("crates", "bindings", "adapters"):
        base = LIB_ROOT / root_dir
        if not base.exists():
            continue
        for rs_path in base.rglob("*.rs"):
            rel = rs_path.relative_to(LIB_ROOT).as_posix()
            if "/tests/" in f"/{rel}" or "/benches/" in f"/{rel}" or rs_path.name == "tests.rs":
                continue
            # Determine package name from Cargo.toml in ancestors
            pkg_dir = rs_path.parent
            while pkg_dir != LIB_ROOT and not (pkg_dir / "Cargo.toml").exists():
                pkg_dir = pkg_dir.parent
            pkg_name = pkg_dir.name
            content = rs_path.read_text(encoding="utf-8", errors="ignore")
            # Strip inline #[cfg(test)] module if at the end of file
            cfg_test_pos = content.find("#[cfg(test)]\nmod tests")
            if cfg_test_pos != -1:
                content = content[:cfg_test_pos]
            all_rs_files.append((pkg_name, rs_path, content))

    target_libs = [
        ("mtk-host-memory", LIB_ROOT / "adapters/mtk-host-memory/src/lib.rs"),
        ("mtk-blender-dna", LIB_ROOT / "adapters/blender/mtk-blender-dna/src/lib.rs"),
        ("mtk-material", LIB_ROOT / "crates/mtk-material/src/lib.rs"),
    ]

    for pkg_name, lib_rs in target_libs:
        if not lib_rs.exists():
            continue
        exported = _extract_pub_Items_from_lib(lib_rs)
        allowed = ALLOWED_ROOT_EXPORTS.get(pkg_name, set())
        other_contents = [
            content for (other_pkg, _, content) in all_rs_files if other_pkg != pkg_name
        ]
        for sym in sorted(exported):
            if sym in allowed:
                continue
            pat = re.compile(rf"\b{re.escape(sym)}\b")
            if not any(pat.search(c) for c in other_contents):
                rel = lib_rs.relative_to(LIB_ROOT).as_posix()
                errors.append(
                    f"{rel}: exported symbol `{sym}` has 0 non-test consumers outside `{pkg_name}` (Rule 1 & 2)"
                )
    return errors


def check_unwrap_expect_in_bindings_and_adapters() -> list[str]:
    errors: list[str] = []
    targets = [
        LIB_ROOT / "bindings/mtk-py/src",
        LIB_ROOT / "adapters/mtk-host-memory/src",
        LIB_ROOT / "adapters/blender/mtk-blender-dna/src",
        LIB_ROOT / "adapters/blender/mtk-blender-dna-py/src",
    ]
    unwrap_re = re.compile(r"\.(unwrap|expect)\s*\(")
    for base in targets:
        if not base.exists():
            continue
        for rs_path in sorted(base.rglob("*.rs")):
            if rs_path.name == "tests.rs" or "/tests/" in rs_path.as_posix():
                continue
            rel = rs_path.relative_to(LIB_ROOT).as_posix()
            content = rs_path.read_text(encoding="utf-8")
            cfg_test_pos = content.find("#[cfg(test)]")
            if cfg_test_pos != -1:
                content = content[:cfg_test_pos]
            for lineno, line in enumerate(content.splitlines(), 1):
                stripped = line.strip()
                if stripped.startswith("//"):
                    continue
                if unwrap_re.search(line):
                    errors.append(
                        f"{rel}:{lineno}: forbidden `.unwrap()`/`.expect()` in non-test binding/adapter code (Rule 8)"
                    )
    return errors


# ---------------------------------------------------------------------------
# Threading / GIL guardrails
# ---------------------------------------------------------------------------

# A variable that holds a core progress callback, i.e. host code invoked by native work.
_CALLBACK_VAR_RE = re.compile(
    r"let\s+(?:mut\s+)?(\w+)\s*:\s*Option<\s*(?:mtk_core::)?ProgressCallback"
)
_DETACH_CALL_RE = re.compile(r"\b(?:py\s*\.\s*detach|run_detached)\s*\(")
_PARALLEL_CALL_RE = re.compile(r"\b(?:into_)?par_iter\s*\(")


def _code_mask(text: str) -> list[bool]:
    """Marks characters that are Rust code (not comment/string/char literal).

    Brace matching must ignore braces inside `format!("{x}")`, raw strings and comments,
    otherwise every `fn` body would appear to end at the first format placeholder.
    """
    mask = [True] * len(text)
    i = 0
    n = len(text)
    while i < n:
        ch = text[i]
        if ch == "/" and i + 1 < n and text[i + 1] == "/":
            j = text.find("\n", i)
            j = n if j == -1 else j
            for k in range(i, j):
                mask[k] = False
            i = j
            continue
        if ch == "/" and i + 1 < n and text[i + 1] == "*":
            depth = 1
            j = i + 2
            while j < n and depth:
                if text.startswith("/*", j):
                    depth += 1
                    j += 2
                elif text.startswith("*/", j):
                    depth -= 1
                    j += 2
                else:
                    j += 1
            for k in range(i, j):
                mask[k] = False
            i = j
            continue
        # raw strings: r"..." / r#"..."# / br#"..."#
        if ch in "rb" and i + 1 < n:
            j = i
            if text[j] == "b":
                j += 1
            if j < n and text[j] == "r":
                j += 1
                hashes = 0
                while j < n and text[j] == "#":
                    hashes += 1
                    j += 1
                if j < n and text[j] == '"':
                    terminator = '"' + "#" * hashes
                    end = text.find(terminator, j + 1)
                    end = n if end == -1 else end + len(terminator)
                    for k in range(i, end):
                        mask[k] = False
                    i = end
                    continue
        if ch == '"':
            j = i + 1
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == '"':
                    j += 1
                    break
                j += 1
            for k in range(i, j):
                mask[k] = False
            i = j
            continue
        if ch == "'":
            # char literal ('{' / '\n') vs lifetime annotation ('a). Only mask real literals.
            j = i + 1
            if j < n and text[j] == "\\":
                j += 2
            else:
                j += 1
            if j < n and text[j] == "'":
                for k in range(i, j + 1):
                    mask[k] = False
                i = j + 1
                continue
            i += 1
            continue
        i += 1
    return mask


def _match_pair(text: str, mask: list[bool], start: int, opener: str, closer: str) -> int:
    depth = 0
    for i in range(start, len(text)):
        if not mask[i]:
            continue
        if text[i] == opener:
            depth += 1
        elif text[i] == closer:
            depth -= 1
            if depth == 0:
                return i
    return len(text) - 1


def _iter_functions(text: str, mask: list[bool]):
    """Yields (name, body_start, body_end) for every `fn` with a block body."""
    for match in re.finditer(r"\bfn\s+([A-Za-z_]\w*)", text):
        if not mask[match.start()]:
            continue
        i = match.end()
        while i < len(text) and not (mask[i] and text[i] == "("):
            if mask[i] and text[i] in "{;":
                i = -1
                break
            i += 1
        if i < 0 or i >= len(text):
            continue
        params_end = _match_pair(text, mask, i, "(", ")")
        j = params_end + 1
        while j < len(text) and not (mask[j] and text[j] in "{;"):
            j += 1
        if j >= len(text) or not mask[j] or text[j] != "{":
            continue  # trait declaration without body
        body_end = _match_pair(text, mask, j, "{", "}")
        yield match.group(1), j, body_end


def _detach_spans(text: str, mask: list[bool], body_start: int, body_end: int) -> list[tuple[int, int]]:
    spans = []
    for match in _DETACH_CALL_RE.finditer(text, body_start, body_end + 1):
        if not mask[match.start()]:
            continue
        open_paren = text.index("(", match.start())
        spans.append((match.start(), _match_pair(text, mask, open_paren, "(", ")")))
    return spans


def _inside(spans: list[tuple[int, int]], position: int) -> bool:
    return any(start <= position <= end for start, end in spans)


def check_threaded_bindings_detach() -> list[str]:
    """Host callbacks and rayon work must only run inside a GIL-released (detached) region.

    The 2026-11 deadlock: the save loader decoded regions on rayon workers and fired the Python
    progress callback from them while the binding still held the GIL. A function-level "does it
    detach somewhere" check would have missed it, so this checks that every *use* of a
    progress-callback variable, and every direct rayon call, sits inside a detach closure.
    """
    errors: list[str] = []
    base = LIB_ROOT / "bindings/mtk-py/src"
    if not base.exists():
        return errors

    for rs_path in sorted(base.rglob("*.rs")):
        if rs_path.name == "tests.rs" or "/tests/" in rs_path.as_posix():
            continue
        rel = rs_path.relative_to(LIB_ROOT).as_posix()
        content = rs_path.read_text(encoding="utf-8")
        cfg_test_pos = content.find("#[cfg(test)]")
        if cfg_test_pos != -1:
            content = content[:cfg_test_pos]
        mask = _code_mask(content)

        for _name, body_start, body_end in _iter_functions(content, mask):
            body_text = content[body_start : body_end + 1]
            spans = _detach_spans(content, mask, body_start, body_end)

            callback_vars = [
                (match.group(1), match.start(1))
                for match in _CALLBACK_VAR_RE.finditer(content, body_start, body_end + 1)
                if mask[match.start()]
            ]
            for var, decl_offset in callback_vars:
                for use in re.finditer(rf"\b{re.escape(var)}\b", body_text):
                    absolute = body_start + use.start()
                    if not mask[absolute]:
                        continue
                    if absolute == decl_offset:
                        continue  # the binding itself, not a use
                    if _inside(spans, absolute):
                        continue
                    line = content.count("\n", 0, absolute) + 1
                    errors.append(
                        f"{rel}:{line}: `{var}` is passed to native work outside a "
                        "`py.detach`/`run_detached` region; host callbacks must never run while "
                        "the GIL is held (see BINDINGS_DESIGN threading contract)"
                    )

            for call in _PARALLEL_CALL_RE.finditer(body_text):
                absolute = body_start + call.start()
                if not mask[absolute] or _inside(spans, absolute):
                    continue
                line = content.count("\n", 0, absolute) + 1
                errors.append(
                    f"{rel}:{line}: direct rayon call outside a `py.detach`/`run_detached` "
                    "region; the binding must release the GIL first"
                )

    return errors


# ---------------------------------------------------------------------------
# Layering guardrail: internal dependency whitelist
# ---------------------------------------------------------------------------

# Allowed normal (non-dev/build) dependencies between workspace crates. This encodes the layering
# in `docs/ARCHITECTURE_2.0.md`, which prose alone has already failed to protect once: the voxel
# core used to own the skin resolver / banner baker / dynamic atlas that now live in
# `mtk-block-entity-textures`.
#
# `*` means "this layer may depend on any internal crate" (tool and binding crates, which are the
# top of the graph by design).
_INTERNAL_DEP_WHITELIST = {
    # L0: no internal dependencies at all.
    "mtk-core": set(),
    # L0/L1 primitives.
    "mtk-cull": {"mtk-core"},
    "mtk-package": set(),
    "mtk-resource": {"mtk-core"},
    "mtk-texture": {"mtk-core", "mtk-resource"},
    "mtk-material": {"mtk-core", "mtk-resource", "mtk-texture"},
    "mtk-model": {"mtk-core", "mtk-cull", "mtk-material", "mtk-resource"},
    # L0 voxel: consumes block-entity textures through a host-injected trait only. It must never
    # depend on `mtk-block-entity-textures` (that would put HTTP + baking back inside L0), nor on
    # `mtk-sync` / `libmtk`.
    "mtk-voxel": {
        "mtk-core",
        "mtk-cull",
        "mtk-material",
        "mtk-model",
        "mtk-resource",
        "mtk-texture",
    },
    # L1/L2.
    "mtk-block-entity-textures": {
        "mtk-core",
        "mtk-model",
        "mtk-resource",
        "mtk-texture",
        "mtk-voxel",
    },
    "mtk-save": {"mtk-core", "mtk-model", "mtk-voxel"},
    "mtk-sync": {
        "mtk-core",
        "mtk-cull",
        "mtk-model",
        "mtk-voxel",
        "mtk-block-entity-textures",
    },
    # L2 facade.
    "libmtk": {
        "mtk-core",
        "mtk-cull",
        "mtk-material",
        "mtk-model",
        "mtk-package",
        "mtk-resource",
        "mtk-save",
        "mtk-texture",
        "mtk-voxel",
    },
    # L3 hosts / tools / bindings.
    "mtk-cli": {"*"},
    "mtk-testkit": set(),
    "mtk-py": {"*"},
    "mtk-jni": {"mtk-model", "mtk-sync"},
    "mtk-wasm": {"mtk-core", "mtk-cull", "mtk-material", "mtk-model", "mtk-voxel"},
    "mtk-host-memory": set(),
    "mtk-blender-dna": {"mtk-host-memory"},
    "mtk-blender-dna-py": {"mtk-blender-dna"},
}


def check_internal_dependency_layering() -> list[str]:
    """Rejects workspace-crate dependencies outside the layering whitelist."""
    errors: list[str] = []
    import subprocess

    try:
        metadata = subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            cwd=LIB_ROOT,
            capture_output=True,
            text=True,
            check=True,
        )
    except FileNotFoundError:
        # No cargo in this environment: nothing to inspect. CI and the local gate always have it.
        print("[ci_guardrails] skipping layering check: cargo not found")
        return errors
    except subprocess.CalledProcessError as exc:
        # Any other failure means the workspace itself is unreadable (e.g. a dependency cycle).
        # Skipping here would silently disable the check.
        return [f"cargo metadata failed ({exc}); the layering check cannot run"]


    packages = json.loads(metadata.stdout)["packages"]
    internal = {package["name"] for package in packages}

    for package in sorted(packages, key=lambda item: item["name"]):
        name = package["name"]
        allowed = _INTERNAL_DEP_WHITELIST.get(name)
        if allowed is None:
            continue  # a workspace member we are not policing yet
        for dep in package["dependencies"]:
            if dep["kind"] is not None:
                continue  # dev/build dependencies may diverge (dev-deps may even cycle)
            if dep["name"] not in internal:
                continue
            if "*" in allowed or dep["name"] in allowed:
                continue
            errors.append(
                f"{name}: depends on `{dep['name']}`, which the layering whitelist forbids "
                "(see docs/ARCHITECTURE_2.0.md; update tools/ci_guardrails.py when the layer changes)"
            )
    return errors


def check_docs_drift() -> list[str]:
    errors: list[str] = []
    doc_files = [
        LIB_ROOT / "README.md",
        LIB_ROOT / "bindings/mtk-py/README.md",
        LIB_ROOT / "docs/API_REFERENCE.md",
        LIB_ROOT / "docs/BINDINGS_DESIGN.md",
        LIB_ROOT / "crates/mtk-material/README.md",
    ]
    for doc in doc_files:
        if not doc.exists():
            continue
        rel = doc.relative_to(LIB_ROOT).as_posix()
        for lineno, line in enumerate(doc.read_text(encoding="utf-8").splitlines(), 1):
            # Ignore lines explicitly noting retirement/removal
            if any(k in line for k in ("已退役", "已移除", "已物理删除", "不再", "删除")):
                continue
            for token in FORBIDDEN_DOC_SYMBOLS:
                if token in line:
                    errors.append(
                        f"{rel}:{lineno}: documentation references retired/deleted API `{token}` (Rule 10)"
                    )
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true", help="Output JSON report")
    args = parser.parse_args()

    errors: list[str] = []
    errors.extend(check_pub_mod_and_mtk_py_pub_use())
    errors.extend(check_dead_rust_exports())
    errors.extend(check_unwrap_expect_in_bindings_and_adapters())
    errors.extend(check_threaded_bindings_detach())
    errors.extend(check_internal_dependency_layering())
    errors.extend(check_docs_drift())

    if args.json:
        print(
            json.dumps(
                {"status": "pass" if not errors else "failed", "errors": errors},
                indent=2,
            )
        )
    else:
        if errors:
            print(f"[libmozitoolkit ci_guardrails] FAILED with {len(errors)} violation(s):")
            for err in errors:
                print(f"  - {err}")
        else:
            print("[libmozitoolkit ci_guardrails] PASSED (0 violations)")
    return 0 if not errors else 1


if __name__ == "__main__":
    sys.exit(main())
