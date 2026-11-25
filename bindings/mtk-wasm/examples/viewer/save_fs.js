/**
 * Browser save directory indexing helpers (FSA directory picker, webkitdirectory, drag-and-drop,
 * and remote JSON index manifest).
 */

export function normalizeSaveFileList(entries) {
  const normalized = entries.map(([rawPath, readerFn]) => [
    rawPath.replace(/\\/g, '/').replace(/^\.\//, '').replace(/^\/+/, ''),
    readerFn,
  ]);
  const levelEntry = normalized.find(([p]) => p === 'level.dat' || p.endsWith('/level.dat'));
  if (!levelEntry) {
    throw new Error('所选目录中未找到 level.dat');
  }
  const levelPath = levelEntry[0];
  const prefix =
    levelPath === 'level.dat' ? '' : levelPath.slice(0, levelPath.length - 'level.dat'.length);

  const fileMap = new Map();
  const relPaths = [];
  for (const [fullPath, readerFn] of normalized) {
    if (prefix && !fullPath.startsWith(prefix)) continue;
    const rel = prefix ? fullPath.slice(prefix.length) : fullPath;
    if (!rel) continue;
    fileMap.set(rel, readerFn);
    relPaths.push(rel);
  }
  return {
    relPaths,
    readFile: (relPath) => {
      const fn = fileMap.get(relPath);
      if (!fn) throw new Error(`Missing save file: ${relPath}`);
      return fn();
    },
  };
}

export async function collectFromDirectoryHandle(dirHandle, prefix = '') {
  const entries = [];
  for await (const [name, handle] of dirHandle.entries()) {
    const rel = prefix ? `${prefix}/${name}` : name;
    if (handle.kind === 'directory') {
      entries.push(...(await collectFromDirectoryHandle(handle, rel)));
    } else if (handle.kind === 'file') {
      entries.push([
        rel,
        async () => new Uint8Array(await (await handle.getFile()).arrayBuffer()),
      ]);
    }
  }
  return entries;
}

export async function collectFromWebkitEntry(entry, prefix = '') {
  const rel = prefix ? `${prefix}/${entry.name}` : entry.name;
  if (entry.isFile) {
    return [
      [
        rel,
        () =>
          new Promise((resolve, reject) => {
            entry.file(
              async (file) => resolve(new Uint8Array(await file.arrayBuffer())),
              reject,
            );
          }),
      ],
    ];
  }
  if (entry.isDirectory) {
    const reader = entry.createReader();
    const batchRead = () =>
      new Promise((resolve, reject) => reader.readEntries(resolve, reject));
    const entries = [];
    while (true) {
      const chunk = await batchRead();
      if (!chunk || chunk.length === 0) break;
      for (const child of chunk) {
        entries.push(...(await collectFromWebkitEntry(child, rel)));
      }
    }
    return entries;
  }
  return [];
}

export async function fetchRemoteSaveEntries(indexUrl) {
  const absIndexUrl = new URL(indexUrl, window.location.href);
  const resp = await fetch(absIndexUrl);
  if (!resp.ok) throw new Error(`HTTP ${resp.status} for ${indexUrl}`);
  const manifest = await resp.json();
  const baseUrl = new URL('./', absIndexUrl);
  const relFiles = Array.isArray(manifest.files) ? manifest.files : [];
  const entries = relFiles.map((relPath) => [
    relPath,
    async () => {
      const fileUrl = new URL(relPath, baseUrl);
      const r = await fetch(fileUrl);
      if (!r.ok) throw new Error(`HTTP ${r.status} for ${relPath}`);
      return new Uint8Array(await r.arrayBuffer());
    },
  ]);
  return { name: manifest.name || 'remote-save', entries };
}
