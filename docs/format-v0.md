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

`onEnter` rules have no verb. Each matching rule's effects run when the hero arrives. Restoring the checkpoint stack replays that entry. A death that names a checkpoint adopts the snapshot as it was saved. The start checkpoint is taken before the first `onEnter`.

`anyNoun` and `anySecond` match that slot no matter what was typed, including nothing. `vocab.json` `bare` fills a missing noun for one verb (`go` → `out`).

## Format 0.2

Minor `2` adds the fields the 0.1 reader ignores. `game.json` may set `carrying` (boot inventory), `deathPrompt`, `hooks` (`early`, `command`, `enter`, `cutsceneEnd`, `cutsceneCommand`, `after`), `timers`, and `declarations` (flags, vars, scores, endings the script touches).

`scripts/game.rhai` is the hook source. A script reads a snapshot and queues effects. It does not see the effects it just queued. `early` runs before move timers. While a timer's `when` conds hold, each ordinary command increments `var` until `limit`; the command that would pass the limit is replaced by the timer's effects. `keepAutosave` freezes the autosave so later room changes leave that snapshot in place.

`cutscenes/<id>.json` is a list of beats (`say`, `frame`, `flipFrom` / `flipTo` / `flipTicks`, `effects`) plus `onCommand` rules and `onEnd` effects. Frames, when present, are `cutscenes/<id>/fNNN.png`.

## Saves and playback

Save JSON is format `1` and must fit in 32 KB. Headless `.walk` scripts and `tests/parity/*.json` transcripts drive the same core as the browser and the desktop player. A walk line is a command, or `@walk`, `@walkto`, `@wait`, `@dismiss`, `@expect`, `@snapshot`, `@save`, `@restore`. `advc test` replays both.

Schemas for the authoring files and the manifest live in `schemas/`.
