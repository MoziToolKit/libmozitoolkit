/**
 * Frontend player-head skin fetcher with configurable concurrency, timeout, and network gating.
 * Candidate URL derivation itself lives in Rust (`mtk_resource::skin::candidate_skin_urls`).
 */

export function buildCandidateSkinUrls(req) {
  if (Array.isArray(req.candidate_urls) && req.candidate_urls.length > 0) {
    return req.candidate_urls;
  }
  return req.skin_url ? [req.skin_url] : [];
}

async function fetchWithTimeout(url, timeoutMs) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await fetch(url, { mode: 'cors', signal: controller.signal });
  } finally {
    clearTimeout(timer);
  }
}

async function fetchFirstValidSkin(saveWorld, req, skinCache, timeoutMs) {
  if (skinCache.has(req.key)) {
    const cached = skinCache.get(req.key);
    return cached ? saveWorld.register_skin_png(req.key, cached) : false;
  }

  for (const url of buildCandidateSkinUrls(req)) {
    try {
      const resp = await fetchWithTimeout(url, timeoutMs);
      if (!resp.ok) continue;
      const bytes = new Uint8Array(await resp.arrayBuffer());
      if (saveWorld.register_skin_png(req.key, bytes)) {
        skinCache.set(req.key, bytes);
        return true;
      }
    } catch {
      // Try next mirror silently; failed/blocked/timed-out mirrors are tolerated.
    }
  }

  skinCache.set(req.key, null);
  return false;
}

export async function resolveSkinsInBackground({
  saveWorld,
  pending,
  skinCache,
  skinConfig,
  isCurrent,
  onProgress,
}) {
  const total = pending.length;
  let completed = 0;
  let resolved = 0;
  onProgress({ resolved, completed, total, done: false });

  let cursor = 0;
  const workerCount = Math.min(Math.max(1, skinConfig.concurrency), total);
  const workers = Array.from({ length: workerCount }, async () => {
    while (cursor < pending.length) {
      if (!isCurrent()) return;
      const req = pending[cursor++];
      const ok = await fetchFirstValidSkin(
        saveWorld,
        req,
        skinCache,
        skinConfig.requestTimeoutMs,
      );
      completed += 1;
      if (ok) resolved += 1;
      if (isCurrent()) {
        onProgress({ resolved, completed, total, done: false });
      }
    }
  });

  await Promise.all(workers);
  if (isCurrent()) {
    onProgress({ resolved, completed, total, done: true });
  }
  return resolved;
}
