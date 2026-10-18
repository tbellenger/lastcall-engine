// Host page that mimics dw: the player sits in a wrapper, under a header,
// with the bundle served from /games/charlie-next/.
import http from "node:http";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const webRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const bundleDir = path.join(webRoot, "dist-bundle");
const publicDir = path.join(webRoot, "public");
const port = Number(process.env.HOST_PORT ?? 4174);

const hostPage = `<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <title>Charlie on dw</title>
    <link rel="stylesheet" href="/games/charlie-next/player.css" />
    <style>
      body { margin: 0; background: #111; color: #eee; font-family: system-ui, sans-serif; }
      header { padding: 16px 24px; background: #1b2430; }
      #__next { min-height: 100vh; }
      main { padding: 16px; }
      #below { margin: 12px auto; max-width: 960px; padding: 24px; background: #242424; }
      #site-search { display: block; margin: 12px auto; width: 240px; }
    </style>
  </head>
  <body>
    <div id="__next">
      <header>dw</header>
      <main>
        <div id="player"></div>
        <div id="below">Under the picture</div>
        <input id="site-search" aria-label="Site search" />
        <button id="site-menu" type="button">Menu</button>
      </main>
    </div>
    <script type="module">
      import { mountPlayer } from "/games/charlie-next/player.js";
      const root = document.querySelector("#player");
      mountPlayer(root, {
        game: "charlie",
        basePath: "/games/charlie-next/",
        page: true,
      });
    </script>
  </body>
</html>
`;

const types = {
  ".css": "text/css; charset=utf-8",
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json",
  ".png": "image/png",
  ".wasm": "application/wasm",
};

function send(res, status, body, type) {
  res.writeHead(status, {
    "Content-Type": type,
    "Content-Length": Buffer.byteLength(body),
    "Cache-Control": "no-store",
  });
  res.end(body);
}

function fileFor(urlPath) {
  if (urlPath === "/games/charlie-next/charlie.advpak") {
    return path.join(publicDir, "charlie.advpak");
  }
  if (urlPath === "/games/charlie-next/charlie-title.png") {
    return path.join(publicDir, "charlie-title.png");
  }
  const prefix = "/games/charlie-next/";
  if (!urlPath.startsWith(prefix)) {
    return null;
  }
  const rel = decodeURIComponent(urlPath.slice(prefix.length));
  const full = path.resolve(bundleDir, rel);
  if (full !== bundleDir && !full.startsWith(bundleDir + path.sep)) {
    return null;
  }
  return full;
}

if (!fs.existsSync(path.join(bundleDir, "player.js"))) {
  console.error("dist-bundle/player.js is missing. Run npm run build:bundle in web/ first.");
  process.exit(1);
}

const server = http.createServer((req, res) => {
  const url = new URL(req.url ?? "/", "http://127.0.0.1");
  if (url.pathname === "/games/charlie") {
    send(res, 200, hostPage, "text/html; charset=utf-8");
    return;
  }
  const file = fileFor(url.pathname);
  if (!file) {
    send(res, 404, "not found", "text/plain; charset=utf-8");
    return;
  }
  fs.readFile(file, (err, data) => {
    if (err) {
      send(res, 404, "not found", "text/plain; charset=utf-8");
      return;
    }
    const type = types[path.extname(file)] ?? "application/octet-stream";
    res.writeHead(200, {
      "Content-Type": type,
      "Content-Length": data.length,
      "Cache-Control": "no-store",
    });
    res.end(data);
  });
});

server.listen(port, "127.0.0.1", () => {
  console.log(`host page http://127.0.0.1:${port}/games/charlie?engine=next`);
});
