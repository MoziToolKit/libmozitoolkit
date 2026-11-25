#!/usr/bin/env python3
"""Build `mtk-wasm` web bindings, run Node.js smoke tests, and optionally package a tarball."""

from __future__ import annotations

import argparse
import io
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request

LIB_ROOT = Path(__file__).resolve().parents[1]


def read_wasm_bindgen_version() -> str:
    lock_text = (LIB_ROOT / "Cargo.lock").read_text(encoding="utf-8")
    m = re.search(
        r'\[\[package\]\]\s+name = "wasm-bindgen"\s+version = "([^"]+)"',
        lock_text,
    )
    if not m:
        raise RuntimeError("Could not find wasm-bindgen version in Cargo.lock")
    return m.group(1)


def read_workspace_version() -> str:
    cargo_toml = (LIB_ROOT / "Cargo.toml").read_text(encoding="utf-8")
    m = re.search(
        r'\[workspace\.package\]\s+version = "([^"]+)"',
        cargo_toml,
    )
    if not m:
        raise RuntimeError("Could not find [workspace.package].version in Cargo.toml")
    return m.group(1)


def ensure_wasm_bindgen_cli(version: str, install_cli: bool) -> str:
    existing = shutil.which("wasm-bindgen")
    if existing:
        out = subprocess.check_output([existing, "--version"], text=True).strip()
        if version in out:
            return existing
        if not install_cli:
            raise RuntimeError(f"Found `{out}`, expected wasm-bindgen {version}")

    if not install_cli:
        raise RuntimeError(
            f"wasm-bindgen {version} not found in PATH (pass --install-cli in CI)"
        )

    bin_dir = Path(tempfile.mkdtemp(prefix="wasm-bindgen-cli-"))
    url = (
        f"https://github.com/wasm-bindgen/wasm-bindgen/releases/download/"
        f"{version}/wasm-bindgen-{version}-x86_64-unknown-linux-musl.tar.gz"
    )
    print(f"[build_wasm] Downloading {url}")
    try:
        with urllib.request.urlopen(url, timeout=60) as resp:
            archive_bytes = resp.read()
        with tarfile.open(fileobj=io.BytesIO(archive_bytes), mode="r:gz") as tf:
            for member in tf.getmembers():
                if member.isfile() and member.name.endswith("/wasm-bindgen"):
                    data = tf.extractfile(member)
                    if data is None:
                        continue
                    dest = bin_dir / "wasm-bindgen"
                    dest.write_bytes(data.read())
                    dest.chmod(0o755)
                    return str(dest)
        raise RuntimeError("wasm-bindgen binary missing from downloaded release tarball")
    except Exception as exc:
        print(f"[build_wasm] Prebuilt download failed ({exc}); falling back to cargo install")
        subprocess.run(
            ["cargo", "install", "--locked", "wasm-bindgen-cli", "--version", version],
            cwd=LIB_ROOT,
            check=True,
        )
        installed = shutil.which("wasm-bindgen")
        if not installed:
            raise RuntimeError("cargo install wasm-bindgen-cli succeeded but binary not in PATH")
        return installed


def run_build(out_dir: Path, install_cli: bool, smoke: bool, tarball_dir: Path | None) -> None:
    wb_version = read_wasm_bindgen_version()
    pkg_version = read_workspace_version()
    wb_bin = ensure_wasm_bindgen_cli(wb_version, install_cli=install_cli)

    subprocess.run(
        ["cargo", "build", "-p", "mtk-wasm", "--target", "wasm32-unknown-unknown", "--release"],
        cwd=LIB_ROOT,
        check=True,
    )

    wasm_path = LIB_ROOT / "target/wasm32-unknown-unknown/release/mtk_wasm.wasm"
    if not wasm_path.is_file():
        raise RuntimeError(f"Expected built wasm at {wasm_path}")

    out_dir.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [wb_bin, "--target", "web", "--out-dir", str(out_dir), str(wasm_path)],
        cwd=LIB_ROOT,
        check=True,
    )

    if smoke:
        subprocess.run(
            ["node", str(LIB_ROOT / "tools/wasm_smoke.mjs"), str(out_dir)],
            cwd=LIB_ROOT,
            check=True,
        )

    if tarball_dir is not None:
        tarball_dir.mkdir(parents=True, exist_ok=True)
        tar_path = tarball_dir / f"mtk-wasm-web-{pkg_version}.tar.gz"
        stage_root = Path(tempfile.mkdtemp(prefix="mtk-wasm-stage-"))
        try:
            stage_pkg = stage_root / "mtk-wasm-web"
            stage_pkg.mkdir(parents=True, exist_ok=True)
            for name in (
                "mtk_wasm.js",
                "mtk_wasm.d.ts",
                "mtk_wasm_bg.wasm",
                "mtk_wasm_bg.wasm.d.ts",
            ):
                src = out_dir / name
                if not src.is_file():
                    raise RuntimeError(f"Missing generated wasm artifact: {src}")
                shutil.copy2(src, stage_pkg / name)
            shutil.copy2(LIB_ROOT / "bindings/mtk-wasm/README.md", stage_pkg / "README.md")
            license_file = LIB_ROOT / "LICENSE"
            if license_file.is_file():
                shutil.copy2(license_file, stage_pkg / "LICENSE")
            with tarfile.open(tar_path, "w:gz") as tf:
                tf.add(stage_pkg, arcname="mtk-wasm-web")
            print(f"[build_wasm] Packaged {tar_path} ({tar_path.stat().st_size} bytes)")
        finally:
            shutil.rmtree(stage_root, ignore_errors=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out-dir", type=Path, required=True, help="Output directory for wasm-bindgen files")
    parser.add_argument("--install-cli", action="store_true", help="Download/install matching wasm-bindgen-cli if absent")
    parser.add_argument("--smoke", action="store_true", help="Run Node.js smoke test against generated package")
    parser.add_argument("--tarball-dir", type=Path, default=None, help="Directory to write mtk-wasm-web-<ver>.tar.gz")
    args = parser.parse_args()

    out_dir = args.out_dir if args.out_dir.is_absolute() else LIB_ROOT / args.out_dir
    tarball_dir = None
    if args.tarball_dir is not None:
        tarball_dir = args.tarball_dir if args.tarball_dir.is_absolute() else LIB_ROOT / args.tarball_dir

    run_build(out_dir, install_cli=args.install_cli, smoke=args.smoke, tarball_dir=tarball_dir)
    return 0


if __name__ == "__main__":
    os.environ.setdefault("PYTHONUNBUFFERED", "1")
    sys.exit(main())
