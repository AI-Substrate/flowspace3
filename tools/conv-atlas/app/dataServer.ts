/**
 * Dev/preview middleware: serves $CONV_ATLAS_HOME/{bundles,insights} under /data.
 * The same mapping is implemented by `conv-atlas serve` for long-lived use.
 * Only those two subtrees are reachable; anything else is 404.
 */
import { createReadStream, statSync } from "node:fs";
import { homedir } from "node:os";
import { extname, join, normalize, resolve, sep } from "node:path";
import type { Connect, Plugin } from "vite";

export const dataHome = (): string =>
  resolve(process.env.CONV_ATLAS_HOME ?? join(homedir(), ".local", "share", "flowspace3", "conv-atlas"));

const TYPES: Record<string, string> = { ".json": "application/json", ".md": "text/markdown; charset=utf-8" };

function middleware(root: string): Connect.NextHandleFunction {
  return (req, res, next) => {
    const url = (req.url ?? "").split("?")[0];
    if (!url.startsWith("/data/")) return next();
    const rel = normalize(decodeURIComponent(url.slice("/data/".length)));
    const top = rel.split(sep)[0];
    const file = resolve(root, rel);
    if ((top !== "bundles" && top !== "insights") || !file.startsWith(root + sep)) {
      res.statusCode = 404;
      return res.end();
    }
    try {
      const st = statSync(file);
      if (!st.isFile()) throw new Error("not a file");
      res.setHeader("Content-Type", TYPES[extname(file)] ?? "application/octet-stream");
      res.setHeader("Cache-Control", "no-store");
      res.setHeader("Content-Length", st.size);
      createReadStream(file).pipe(res);
    } catch {
      res.statusCode = 404;
      res.end();
    }
  };
}

export function convAtlasData(): Plugin {
  const root = dataHome();
  return {
    name: "conv-atlas-data",
    configureServer(server) {
      server.config.logger.info(`  conv-atlas data: ${root}`);
      server.middlewares.use(middleware(root));
    },
    configurePreviewServer(server) {
      server.middlewares.use(middleware(root));
    },
  };
}
