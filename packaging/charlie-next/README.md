# Charlie: Last Call — next engine web player

Self-contained build of the Last Call engine and the Charlie package. Serve this directory as static files, ideally at `/games/charlie-next/`. Every asset URL is relative to this directory. Pass `basePath` when the package does not sit next to the page.

## Files

| File | Role |
| --- | --- |
| `index.html` | Minimal page that mounts the player |
| `player.js` | ES module. Exports `mountPlayer` |
| `player.css` | Styles scoped under `.lastcall-player` |
| `*.wasm` | Engine module, next to `player.js` or under `assets/` |
| `charlie.advpak` | Game package |
| `charlie-title.png` | Title card |

## Mount

```html
<link rel="stylesheet" href="./player.css" />
<div id="charlie-next"></div>
<script type="module">
  import { mountPlayer } from "./player.js";

  const el = document.querySelector("#charlie-next");
  const handle = await mountPlayer(el, {
    game: "charlie",
    basePath: "/games/charlie-next/",
  });
  // handle.dispose() removes listeners and stops the frame loop
</script>
```

`mountPlayer(el, options)` returns `Promise<{ dispose(): void }>`.

| Option | Default | Meaning |
| --- | --- | --- |
| `game` | `"charlie"` | Package stem. The player fetches `${basePath}${game}.advpak`. |
| `basePath` | directory of `player.js` | Directory that holds the `.advpak` and `charlie-title.png`. A path such as `/games/charlie-next/` or `./` is resolved against the page URL. A trailing slash is added when it is missing. Omit it when those files sit next to `player.js`. |
| `storageKey` | `lastcall-charlie-next` for Charlie | IndexedDB database name. |
| `page` | `false` | `true` listens for pointer and keyboard events on the document and window. That is what `index.html` uses for a full-page mount. `false` listens only on `el`, so a host page keeps its own keys. |

The `.wasm` file is loaded from `import.meta.url` inside `player.js`, not from `basePath`. Keep `player.js`, `player.css`, and the wasm file in the layout this archive ships.

## Saves

Slots 1–5 and the autosave are stored in IndexedDB:

- database: `lastcall-charlie-next` (override with `storageKey`)
- object store: `saves`
- keys: `"1"` … `"5"` and `"autosave"`

That database name is separate from the current game's saves and from the Gatehouse shell, which uses `lastcall`.

## Headers

Serve the wasm module as:

```
Content-Type: application/wasm
```

`player.js` needs a JavaScript type (`text/javascript` or `application/javascript`). `charlie.advpak` can be `application/octet-stream`.

## Sub-path

Opening `index.html` from `/games/charlie-next/` works with no extra configuration: the page sets `basePath` to its own directory. To mount the same files inside another page:

```js
mountPlayer(el, { game: "charlie", basePath: "/games/charlie-next/" });
```
