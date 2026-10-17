# Adventure package format v0

A `.advpak` is a zip archive. `manifest.json` sits at the archive root. Every other file is listed there with its SHA-256 and byte size. The manifest's `contentHash` is the SHA-256 of those entries sorted by path, each line `path`, a NUL, the hex digest, a NUL, the decimal size, and a newline. JSON entries are deflated. PNG entries are stored uncompressed so the bytes stay stable.

`formatVersion` is `major.minor.patch`. This engine accepts major `0` only. A newer minor loads and emits a warning. `engineMin` is the oldest engine release the package expects.

| Path | Contents |
| --- | --- |
| `manifest.json` | Format version, engine floor, game id, version, content hash, resource list |
| `game.json` | Identity, 384×216 resolution, start room and spawn, hero id, score max, rules |
| `vocab.json` | Parser vocabulary |
| `items.json` | Inventory nouns |
| `rooms/<id>/room.json` | Spawns, exits, hotspots, props, actors, rules, priority bands |
| `rooms/<id>/picture.png` | Opaque 384×216 background |
| `rooms/<id>/walk.png` | Walk mask, 255 walkable |
| `rooms/<id>/priority.png` | Occlusion map. A non-zero paint pixel overrides the horizon band |
| `rooms/<id>/props.png` | Optional prop atlas |
| `actors/<id>/actor.json` | Costumes and animation frame rectangles |
| `actors/<id>/atlas.png` | Sprite atlas. Anchors are the foot point inside each frame |

Authoring folders are not the zip. `advc pack` compiles them: walk polygons (with holes) are rasterised, horizon bands are merged with an optional paint layer, horizontal sprite strips are shelved into an atlas no wider than 1024 pixels, and compact rules become an effect list.

## Rules

A rule matches a verb id and, when set, a noun id and a second noun id. `req` entries are `has:`, `flag:`, `room:`, `var:` (optional `%modulus`), `count:`, or a leading `!`. Room rules outrank global rules. A more specific pattern wins. `once` records the rule id only after a walk to the hotspot succeeds.

Compact authoring fields, used only when `effects` is omitted, compile in this order: `walkTo`, `face`, `take`, `give`, `set`, `clear`, `inc`, `setVar`, `score`, `sfx`, `say` / `text` / `texts`, `checkpoint`, `goto`, `death`, `ending`, `wait`. `once` inserts `markOnce` at the front of that list. The runtime still inserts its own walk and face before the list. An explicit `effects` array is stored as written, plus `markOnce` when `once` is set and the author did not already include it.

`onEnter` rules have no verb. Each matching rule's effects run when the hero arrives, including after a checkpoint restore. The start checkpoint is taken before the first `onEnter`.

## Saves and scripts

Save JSON is format `1` and must fit in 32 KB. Headless scripts (`.walk`) drive the same core as the browser and the desktop player: typed lines, `@walk`, `@walkto`, `@wait`, `@dismiss`, `@expect`, `@snapshot`, `@save`, and `@restore`.

Schemas for the authoring files and the manifest live in `schemas/`.
