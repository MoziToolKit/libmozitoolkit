#!/usr/bin/env python3
"""Real-scenario meshing benchmark (not a synthetic kernel micro-benchmark).

Why this exists
---------------
`crates/mtk-bench` measured `ModelSource::None` on synthetic dense sections, and the
Dev-panel "Mesher Benchmark" measures only the native mesher on the sparse debug world.
Neither exercises the path a user actually waits on: region I/O -> padded extraction ->
model/atlas/CTM/biome meshing -> weld assembly -> Blender injection -> material binding.

This script times that whole path on a real save with a real asset cache and prints each
stage next to the end-to-end number, so "quads/s" always says which layer it means.

Usage (headless Blender):

    blender --background --factory-startup --python tools/blender/scenario_bench.py -- \
        --assets ../Minecraft_Source \
        --cache-dir ../MoziToolKit/.ci-cache \
        --save ../mtk-save-world \
        --bounds -64,-64,-64,512,320,512 \
        --iterations 3 --inject

--cache can point at a specific .mtkcache; otherwise the newest package in --cache-dir is
used. When no cache is available the run fails loudly instead of meshing fallback cubes.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from pathlib import Path
from typing import Any, Dict, List

PROJECT_DIR = Path(__file__).resolve().parents[2]
PARENT_DIR = PROJECT_DIR.parent


def _parse_args() -> argparse.Namespace:
    argv = sys.argv
    argv = argv[argv.index("--") + 1 :] if "--" in argv else []
    # A negative bound like `--bounds -64,...` looks like a flag to argparse; join it.
    joined: List[str] = []
    skip = False
    for index, token in enumerate(argv):
        if skip:
            skip = False
            continue
        if token in ("--bounds", "--save", "--assets") and index + 1 < len(argv):
            joined.append(f"{token}={argv[index + 1]}")
            skip = True
            continue
        joined.append(token)
    argv = joined
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--assets", default=os.environ.get("MTK_TEST_ASSETS", ""),
                        help="Resource pack root or jar (assets/minecraft must resolve)")
    parser.add_argument("--cache-dir", default=os.environ.get("MOZI_CACHE_DIR", ""),
                        help="Directory holding .mtkcache packages")
    parser.add_argument("--cache", default="", help="Explicit .mtkcache path")
    parser.add_argument("--save", required=True, help="Minecraft save directory")
    parser.add_argument("--dimension", default="overworld")
    parser.add_argument("--bounds", default="-64,-64,-64,512,320,512",
                        help="min_x,min_y,min_z,max_x,max_y,max_z")
    parser.add_argument("--iterations", type=int, default=3)
    parser.add_argument("--threads", type=int, default=0)
    parser.add_argument("--no-ctm", action="store_true")
    parser.add_argument("--inject", action="store_true",
                        help="Also inject into Blender and bind materials (host-side cost)")
    parser.add_argument("--json", default="", help="Write raw results as JSON")
    return parser.parse_args(argv)


def _bootstrap(args: argparse.Namespace) -> None:
    for path in (PROJECT_DIR, PARENT_DIR, PROJECT_DIR / "dev" / "lib"):
        if path.exists() and str(path) not in sys.path:
            sys.path.insert(0, str(path))
    if args.assets:
        os.environ.setdefault("MTK_TEST_ASSETS", args.assets)
        os.environ.setdefault("MC_ASSETS_DIR", args.assets)
    if args.cache_dir:
        os.environ.setdefault("MOZI_CACHE_DIR", args.cache_dir)


def _open_cache(mtk: Any, args: argparse.Namespace) -> Any:
    if args.cache:
        return mtk.AssetCache.open(str(Path(args.cache).resolve()))
    cache_dir = Path(args.cache_dir or (PROJECT_DIR / ".ci-cache"))
    packages = sorted(cache_dir.glob("*.mtkcache"), key=lambda p: p.stat().st_mtime)
    if not packages:
        raise SystemExit(
            f"no .mtkcache in {cache_dir}; prebake the configured pack stack first "
            "(this benchmark refuses to measure the fallback-cube path)"
        )
    return mtk.AssetCache.open(str(packages[-1].resolve()))


def _profile_rows(mesh: Any) -> List[Dict[str, Any]]:
    profile = getattr(mesh, "profile", None)
    if profile is None:
        return []
    rows = []
    for stage in getattr(profile, "stages", []) or []:
        rows.append(
            {
                "name": getattr(stage, "name", "?"),
                "ms": float(getattr(stage, "duration_ms", 0.0)),
                "depth": int(getattr(stage, "depth", 0) or 0),
            }
        )
    return rows


def _last_profile_stages() -> List[Dict[str, Any]]:
    try:
        from MoziToolKit.utils.diagnostics import get_last_profile
    except Exception:
        return []
    profile = get_last_profile()
    if not profile:
        return []
    return [
        {"name": s["name"], "ms": float(s["duration_ms"]), "depth": int(s.get("depth", 0))}
        for s in profile.get("stages", [])
    ]


def main() -> int:
    args = _parse_args()
    _bootstrap(args)

    import bpy  # noqa: F401  (host check: this script is Blender-only)
    import MoziToolKit

    try:
        MoziToolKit.register()
    except Exception:
        pass

    from MoziToolKit.bridge import engine

    mtk = engine.get_libmtk()
    if mtk is None:
        raise SystemExit("libmtk_py is not importable (build it with `cargo build-all-py`)")

    cache = _open_cache(mtk, args)
    manifest = cache.get_manifest()

    t = time.perf_counter()
    models = cache.load_models()
    atlas = cache.load_atlas()
    biome = cache.load_biome_resolver()
    colormaps = cache.load_colormaps()
    ctm = None if args.no_ctm else cache.load_ctm_solver()
    asset_ms = (time.perf_counter() - t) * 1000.0

    print("=" * 78)
    print(" Real-scenario meshing benchmark")
    print("=" * 78)
    print(f" cache       : {cache.package_path}")
    print(f" manifest    : fmt={manifest['format_version']} packs={manifest['pack_count']} "
          f"models={manifest['baked_models']} atlas_chunks={manifest['atlas_chunks']}")
    print(f" asset load  : {asset_ms:.0f} ms (models={len(models)} ctm={ctm is not None})")

    from MoziToolKit.bridge.save import load_and_mesh_minecraft_save

    min_block = [int(v) for v in args.bounds.split(",")[:3]]
    max_block = [int(v) for v in args.bounds.split(",")[3:]]
    num_threads = args.threads or None

    results: List[Dict[str, Any]] = []
    for iteration in range(args.iterations):
        started = time.perf_counter()
        mesh, _meta, storage, native_ms = load_and_mesh_minecraft_save(
            args.save,
            dimension=args.dimension,
            min_block=tuple(min_block),
            max_block=tuple(max_block),
            prefs=None,
            model_db=models,
            atlas=atlas,
            biome_resolver=biome,
            colormaps=colormaps,
            ctm_solver=ctm,
            resolve_prefs=False,
            resolve_ctm=False,
            num_threads=num_threads,
            force=iteration == 0,
        )
        inject_ms = 0.0
        material_ms = 0.0
        if args.inject:
            from MoziToolKit.utils.world.injector import apply_voxel_mesh_to_blender

            t = time.perf_counter()
            apply_voxel_mesh_to_blender(
                mesh, storage, elapsed_ms=native_ms, atlas=atlas, sync_point_cloud=False
            )
            inject_ms = (time.perf_counter() - t) * 1000.0

        wall_ms = (time.perf_counter() - started) * 1000.0
        stages = _profile_rows(mesh) or _last_profile_stages()
        if args.inject:
            host_stages = {s["name"]: s["ms"] for s in _last_profile_stages()}
            inject_ms = host_stages.get("blender_mesh_injection", inject_ms)
            material_ms = host_stages.get("blender_material_binding", material_ms)

        quads = int(mesh.quad_count)
        verts = int(mesh.vertex_count)
        rows = {
            "iteration": iteration,
            "native_ms": native_ms,
            "wall_ms": wall_ms,
            "inject_ms": inject_ms,
            "material_ms": material_ms,
            "quads": quads,
            "verts": verts,
            "quads_per_sec_native": quads / (native_ms / 1000.0) if native_ms else 0.0,
            "quads_per_sec_wall": quads / (wall_ms / 1000.0) if wall_ms else 0.0,
            "stages": stages,
        }
        results.append(rows)

        print(f"\n[{iteration}] quads={quads:,} verts={verts:,}")
        print(f"    native mesh   : {native_ms:8.1f} ms  ({rows['quads_per_sec_native']:,.0f} quads/s)")
        if args.inject:
            print(f"    blender inject: {inject_ms:8.1f} ms")
            print(f"    material bind : {material_ms:8.1f} ms")
        print(f"    end to end    : {wall_ms:8.1f} ms  ({rows['quads_per_sec_wall']:,.0f} quads/s)")
        for stage in stages:
            if stage["depth"] <= 1:
                print(f"      - {stage['name']:<24s} {stage['ms']:8.2f} ms")

    best = min(r["wall_ms"] for r in results)
    best_native = min(r["native_ms"] for r in results)
    quads = results[-1]["quads"]
    print("\n" + "-" * 78)
    print(f" best native : {best_native:.1f} ms -> {quads / (best_native / 1000.0):,.0f} quads/s")
    print(f" best end2end: {best:.1f} ms -> {quads / (best / 1000.0):,.0f} quads/s")

    if args.json:
        Path(args.json).write_text(json.dumps({"results": results}, indent=2), encoding="utf-8")
        print(f" raw results : {args.json}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
