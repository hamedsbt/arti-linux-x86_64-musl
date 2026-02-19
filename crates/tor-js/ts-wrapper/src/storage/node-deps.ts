import type path from 'node:path';

export type NodeDeps = {
  fs: typeof import('node:fs/promises');
  os: typeof import('node:os');
  path: path.PlatformPath;
};

let promise: Promise<NodeDeps> | undefined;

export function getNodeDeps(): Promise<NodeDeps> {
  if (!promise) {
    promise = (async () => {
      const [fs, os, path] = await Promise.all([
        import('node:fs/promises').then(m => m.default ?? m),
        import('node:os').then(m => m.default ?? m),
        import('node:path').then(m => m.default ?? m),
      ]);
      return { fs, os, path };
    })();
  }
  return promise;
}