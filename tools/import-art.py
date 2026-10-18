#!/usr/bin/env python3
"""Bake dw pixel art into a lastcall-engine authoring folder.

Scales strips the way the pixel stage does (nearest neighbour, round half up),
writes per-actor occlusion masks, flipbook frames, and playfield references.
"""

import json
import math
import shutil
import sys
from pathlib import Path

from PIL import Image

W = 384
H = 216
DIRS = {
    "south": "s",
    "north": "n",
    "east": "e",
    "west": "w",
}
STEP = (
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
)


def js_round(value: float) -> int:
    if value >= 0:
        return int(math.floor(value + 0.5))
    return int(math.ceil(value - 0.5))


def scale_for(who: str) -> float:
    if who in ("charlie", "charlie_dry", "sal", "earl"):
        return 1.3
    if who == "tank":
        return 0.78
    return 1.0


def anim_name(kind: str, key: str) -> str:
    if kind == "stand" and key in DIRS:
        return f"stand_{DIRS[key]}"
    if key.startswith("walk_"):
        rest = key[len("walk_") :]
        if rest in DIRS:
            return f"walk_{DIRS[rest]}"
    return key


def rasterize(points: list[list[int]]) -> list[int]:
    mask = [0] * (W * H)
    ring = [(int(p[0]), int(p[1])) for p in points]
    if len(ring) < 3:
        return mask
    for y in range(H):
        xs: list[int] = []
        for i, (x0, y0) in enumerate(ring):
            x1, y1 = ring[(i + 1) % len(ring)]
            if y0 == y1:
                continue
            ymin, ymax = (y0, y1) if y0 < y1 else (y1, y0)
            if y < ymin or y >= ymax:
                continue
            xs.append(x0 + (x1 - x0) * (y - y0) // (y1 - y0))
        xs.sort()
        for i in range(0, len(xs) - 1, 2):
            x0 = max(xs[i], 0)
            x1 = min(xs[i + 1], W)
            while x0 < x1:
                mask[y * W + x0] = 255
                x0 += 1
    return mask


def in_mask(mask: list[int], x: int, y: int) -> bool:
    return 0 <= x < W and 0 <= y < H and mask[y * W + x] != 0


def can_step(mask: list[int], x: int, y: int, nx: int, ny: int) -> bool:
    if not in_mask(mask, nx, ny):
        return False
    dx = nx - x
    dy = ny - y
    if dx != 0 and dy != 0:
        return in_mask(mask, x + dx, y) and in_mask(mask, x, y + dy)
    return True


def flood(mask: list[int], start: tuple[int, int]) -> list[bool]:
    seen = [False] * (W * H)
    if not in_mask(mask, start[0], start[1]):
        return seen
    stack = [start]
    seen[start[1] * W + start[0]] = True
    while stack:
        x, y = stack.pop()
        for dx, dy in STEP:
            nx, ny = x + dx, y + dy
            if not can_step(mask, x, y, nx, ny):
                continue
            idx = ny * W + nx
            if not seen[idx]:
                seen[idx] = True
                stack.append((nx, ny))
    return seen


def nearest(mask: list[int], seen: list[bool] | None, x: int, y: int) -> tuple[int, int] | None:
    best = None
    best_d = 10**12
    for yy in range(H):
        row = yy * W
        for xx in range(W):
            if mask[row + xx] == 0:
                continue
            if seen is not None and not seen[row + xx]:
                continue
            d = (xx - x) * (xx - x) + (yy - y) * (yy - y)
            if d < best_d or (d == best_d and best is not None and (yy, xx) < (best[1], best[0])):
                best_d = d
                best = (xx, yy)
    return best


def scale_strip(image: Image.Image, frame_w: int, frame_h: int, scale: float) -> tuple[Image.Image, int, int]:
    frames = image.width // frame_w
    dw = max(1, js_round(frame_w * scale))
    dh = max(1, js_round(frame_h * scale))
    out = Image.new("RGBA", (dw * frames, dh), (0, 0, 0, 0))
    rgba = image.convert("RGBA")
    for index in range(frames):
        frame = rgba.crop((index * frame_w, 0, (index + 1) * frame_w, frame_h))
        out.paste(frame.resize((dw, dh), Image.Resampling.NEAREST), (index * dw, 0))
    return out, dw, dh


def scale_anchor(ax: int, ay: int, scale: float, dw: int, dh: int) -> tuple[int, int]:
    sx = min(max(js_round(ax * scale), 0), dw - 1)
    sy = min(max(js_round(ay * scale), 0), dh - 1)
    return sx, sy


def make_hairpick() -> Image.Image:
    image = Image.new("RGBA", (34, 14), (0, 0, 0, 0))
    px = image.load()
    black = (12, 10, 14, 255)
    for y in range(1, 13):
        for x in range(1, 8):
            px[x, y] = black
    for y in range(2, 5):
        for x in range(6, 33):
            px[x, y] = black
    for x in range(10, 33, 3):
        for tooth in range(2):
            for y in range(4, 13):
                if x + tooth < 34:
                    px[x + tooth, y] = black
    return image


def make_soap() -> Image.Image:
    image = Image.new("RGBA", (16, 6), (0, 0, 0, 0))
    px = image.load()
    body = (46, 158, 72, 255)
    light = (126, 214, 132, 255)
    dark = (24, 96, 48, 255)
    for y in range(6):
        for x in range(16):
            if x in (0, 15) and y in (0, 5):
                continue
            px[x, y] = body
    for x in range(2, 14):
        px[x, 1] = light
    for x in range(1, 15):
        px[x, 5] = dark
    return image


def load_json(path: Path):
    return json.loads(path.read_text())


def write_json(path: Path, value) -> None:
    path.write_text(json.dumps(value, indent=2) + "\n")


def ensure_prop(pixel: Path, name: str, size: tuple[int, int], factory) -> Path:
    path = pixel / "props" / name
    if path.is_file():
        with Image.open(path) as image:
            if image.size == size:
                return path
    image = factory()
    image.save(path)
    print(f"drew missing prop {name} {image.size}")
    return path


def band_for(y: int) -> int:
    return min(max(y, 0), 200)


def blit(fb: bytearray, priority: list[int], sprite: Image.Image, dest_x: int, dest_y: int, layer_priority: int, before: bytearray | None, mask: list[int] | None) -> None:
    if before is not None:
        before[:] = fb
    pixels = sprite.convert("RGBA").load()
    sw, sh = sprite.size
    for row in range(sh):
        for col in range(sw):
            dx = dest_x + col
            dy = dest_y + row
            if dx < 0 or dy < 0 or dx >= W or dy >= H:
                continue
            r, g, b, a = pixels[col, row]
            if a < 128:
                continue
            pi = dy * W + dx
            if priority[pi] > layer_priority:
                continue
            di = pi * 4
            fb[di : di + 4] = bytes((r, g, b, 255))
    if before is not None and mask is not None and len(mask) == W * H:
        for i in range(W * H):
            if mask[i] != 0:
                di = i * 4
                fb[di : di + 4] = before[di : di + 4]


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: import-art.py <pixel-root> <fixture-dir>", file=sys.stderr)
        return 2
    pixel = Path(sys.argv[1])
    out = Path(sys.argv[2])
    manifest = load_json(pixel / "manifest.json")
    characters = manifest["characters"]

    sheets: dict[str, dict[str, dict]] = {}
    for who, data in characters.items():
        if who == "charlie_dry":
            actor_id = "charlie"
            costume = "dry"
        else:
            actor_id = who
            costume = "default"
        scale = scale_for(who)
        actor_dir = out / "actors" / actor_id
        actor_dir.mkdir(parents=True, exist_ok=True)
        bucket = sheets.setdefault(actor_id, {}).setdefault(costume, {"sheets": []})
        entries = []
        for key, spec in (data.get("stand") or {}).items():
            entries.append((anim_name("stand", key), spec))
        for key, spec in (data.get("anims") or {}).items():
            entries.append((anim_name("anim", key), spec))
        for name, spec in entries:
            sheet_rel = spec["sheet"]
            src = pixel / sheet_rel
            if src.suffix.lower() != ".png" or not src.is_file():
                print(f"skip {who} {name}: {sheet_rel}", file=sys.stderr)
                continue
            with Image.open(src) as raw:
                frame_w = int(spec["frameW"])
                frame_h = int(spec["frameH"])
                if raw.size[1] != frame_h or raw.size[0] % frame_w != 0:
                    raise SystemExit(f"{src} is not a {frame_w}x{frame_h} strip")
                scaled, dw, dh = scale_strip(raw, frame_w, frame_h, scale)
            count = scaled.width // dw
            ax, ay = scale_anchor(int(spec["anchor"][0]), int(spec["anchor"][1]), scale, dw, dh)
            file_name = f"{name}.png" if costume == "default" else f"dry_{name}.png"
            scaled.save(actor_dir / file_name)
            frames = list(range(count))
            animations = {
                name: {
                    "fps": 8 if count > 1 else 1,
                    "loop": count > 1,
                    "frames": frames,
                }
            }
            if name == "guard_chair" and count > 4:
                animations["guard_asleep"] = {"fps": 4, "loop": True, "frames": [3, 4]}
            if name == "thug_ko" and count > 7:
                animations["thug_ko"] = {"fps": 1, "loop": False, "frames": [7]}
                animations["thug_ko_fall"] = {
                    "fps": 8,
                    "loop": False,
                    "frames": frames,
                }
            bucket["sheets"].append(
                {
                    "sheet": file_name,
                    "frameW": dw,
                    "frameH": dh,
                    "ax": ax,
                    "ay": ay,
                    "animations": animations,
                }
            )
        for stale in ("sheet.png", "sheet-dry.png"):
            leftover = actor_dir / stale
            if leftover.exists():
                leftover.unlink()

    for actor_id, costumes in sheets.items():
        costumes = {name: costume for name, costume in costumes.items() if costume["sheets"]}
        if "default" not in costumes:
            continue
        write_json(
            out / "actors" / actor_id / "actor.json",
            {"id": actor_id, "costumes": costumes},
        )

    hair = ensure_prop(pixel, "hairpick_overlay.png", (34, 14), make_hairpick)
    soap = ensure_prop(pixel, "soap_overlay.png", (16, 6), make_soap)
    prop_specs = {
        "alley": [
            ("umbrella", pixel / "props" / "umbrella_overlay.png", [208, 150], "!has:umbrella", []),
            ("matchbook", pixel / "props" / "matchbook_overlay.png", [147, 193], "flag:matchbookHere", ["!has:matchbook"]),
        ],
        "bathroom": [
            ("soap", soap, [218, 127], "!has:soap", []),
        ],
        "club": [
            ("hairpick", hair, [300, 128], "flag:tippedDancer", ["!has:pick"]),
        ],
    }
    placements = {
        "bar": [
            {"id": "sal", "actor": "sal", "pos": [78, 168], "anim": "wipe", "occlude": "counter"},
            {"id": "earl", "actor": "earl", "pos": [168, 203], "anim": "seated_idle"},
        ],
        "alley": [
            {"id": "professor", "actor": "professor", "pos": [62, 204], "anim": "seated"},
            {"id": "tank", "actor": "tank", "pos": [128, 196]},
        ],
        "club": [
            {"id": "dancer", "actor": "dancer", "pos": [356, 130], "anim": "dancer_dance"},
            {"id": "bartender", "actor": "club_bartender", "pos": [128, 151], "occlude": "counter"},
            {"id": "goon", "actor": "goon", "pos": [92, 194]},
        ],
        "street": [
            {"id": "doorman", "actor": "doorman", "pos": [248, 196]},
        ],
        "billiard": [
            {"id": "thug", "actor": "thug", "pos": [120, 198]},
        ],
        "hallway": [
            {"id": "guard", "actor": "lakehouse_guard", "pos": [296, 192], "anim": "guard_asleep"},
        ],
        "office": [
            {"id": "dana", "actor": "dana", "pos": [188, 206], "anim": "dana_tied"},
        ],
    }

    rooms = manifest["rooms"]
    for room_id, room_art in rooms.items():
        room_dir = out / "rooms" / room_id
        if not room_dir.is_dir():
            continue
        src = pixel / room_art["image"]
        shutil.copyfile(src, room_dir / "picture.png")
        polygon = room_art.get("walkPolygon") or [[1, 1], [382, 1], [382, 214], [1, 214]]
        mask = rasterize(polygon)
        if sum(1 for v in mask if v) < 8:
            polygon = [[1, 1], [382, 1], [382, 214], [1, 214]]
            mask = rasterize(polygon)
            print(f"{room_id}: walk polygon was empty, using the full floor", file=sys.stderr)
        write_json(room_dir / "walk.json", {"polygons": [{"points": polygon}]})
        write_json(
            room_dir / "priority.json",
            {"bands": [{"y": y, "priority": band_for(y)} for y in range(H)]},
        )
        paint = room_dir / "priority.png"
        if paint.exists():
            paint.unlink()

        room = load_json(room_dir / "room.json")
        spawns = room.get("spawns") or {"default": [176, 180]}
        default = spawns.get("default") or [176, 180]
        home = nearest(mask, None, int(default[0]), int(default[1]))
        if home is None:
            raise SystemExit(f"{room_id} has no walkable pixel")
        seen = flood(mask, home)
        snapped = {}
        for name, point in spawns.items():
            x, y = int(point[0]), int(point[1])
            if seen[y * W + x] if 0 <= x < W and 0 <= y < H else False:
                snapped[name] = [x, y]
                continue
            spot = nearest(mask, seen, x, y) or home
            if (x, y) != spot:
                print(f"{room_id} spawn {name} {x},{y} -> {spot[0]},{spot[1]}")
            snapped[name] = [spot[0], spot[1]]
        room["spawns"] = snapped
        for hotspot in room.get("hotspots") or []:
            stand = hotspot.get("stand")
            if not stand:
                continue
            x, y = int(stand[0]), int(stand[1])
            if 0 <= x < W and 0 <= y < H and seen[y * W + x]:
                continue
            spot = nearest(mask, seen, x, y)
            if spot is None:
                hotspot.pop("stand", None)
                continue
            if (x, y) != spot:
                print(f"{room_id} hotspot {hotspot.get('id')} stand {x},{y} -> {spot[0]},{spot[1]}")
            hotspot["stand"] = [spot[0], spot[1]]

        actors = []
        for place in placements.get(room_id, []):
            entry = {
                "id": place["id"],
                "actor": place["actor"],
                "pos": place["pos"],
                "facing": "s",
            }
            if "anim" in place:
                entry["anim"] = place["anim"]
            if "occlude" in place:
                entry["occlude"] = place["occlude"]
            actors.append(entry)
        room["actors"] = actors

        props = []
        images = []
        cursor = 0
        height = 1
        for prop_id, image_path, pos, show_if, req in prop_specs.get(room_id, []):
            with Image.open(image_path) as raw:
                image = raw.convert("RGBA")
            images.append(image)
            height = max(height, image.height)
            prop = {
                "id": prop_id,
                "pos": pos,
                "showIf": show_if,
                "frame": {"x": cursor, "y": 0, "w": image.width, "h": image.height, "ax": 0, "ay": 0},
            }
            if req:
                prop["req"] = req
            props.append(prop)
            cursor += image.width
        room["props"] = props
        if images:
            atlas = Image.new("RGBA", (cursor, height), (0, 0, 0, 0))
            x = 0
            for image in images:
                atlas.paste(image, (x, 0))
                x += image.width
            atlas.save(room_dir / "props.png")
        else:
            props_png = room_dir / "props.png"
            if props_png.exists():
                props_png.unlink()

        if room_id == "bar":
            mask_img = Image.open(pixel / "props" / "sal_counter_occlusion_mask.png").convert("L")
            if mask_img.size != (W, H):
                raise SystemExit("sal occlusion mask is the wrong size")
            raw = mask_img.tobytes()
            out_mask = bytes(255 if b >= 128 else 0 for b in raw)
            Image.frombytes("L", (W, H), out_mask).save(room_dir / "occlude-counter.png")
        elif room_id == "club":
            mask_img = Image.new("L", (W, H), 0)
            px = mask_img.load()
            for y in range(110, 110 + 48):
                for x in range(110, 110 + 36):
                    px[x, y] = 255
            mask_img.save(room_dir / "occlude-counter.png")

        write_json(room_dir / "room.json", room)

    variants = {
        "street": {"door": "cs_street_door", "cosh": "cs_street_cosh"},
        "billiard": {"tied": "cs_billiard_tied"},
        "hallway": {"clear": "cs_hallway_clear"},
        "office": {"police": "cs_office_police", "dana": "cs_office_dana_tied"},
    }
    cut_root = pixel / "cutscenes"
    for room_id, names in variants.items():
        room_dir = out / "rooms" / room_id
        room = load_json(room_dir / "room.json")
        room["pictures"] = list(names)
        write_json(room_dir / "room.json", room)
        for name, cut_id in names.items():
            frames = sorted((cut_root / cut_id).glob("f*.png"))
            if not frames:
                raise SystemExit(f"no frames for {cut_id}")
            pick = frames[len(frames) // 2] if name == "cosh" else frames[0]
            shutil.copyfile(pick, room_dir / f"variant-{name}.png")

    engine_cuts = out / "cutscenes"
    engine_cuts.mkdir(parents=True, exist_ok=True)
    alley_json = load_json(engine_cuts / "alley-dana.json")
    alley_frames = sorted((cut_root / "cs_alley_dana_taken").glob("f*.png"))
    alley_dir = engine_cuts / "alley-dana"
    if alley_dir.exists():
        shutil.rmtree(alley_dir)
    alley_dir.mkdir()
    for index, frame in enumerate(alley_frames):
        shutil.copyfile(frame, alley_dir / f"f{index:03}.png")
    count = len(alley_frames)
    beats = alley_json["beats"]
    edges = [js_round(count * i / len(beats)) for i in range(len(beats))] + [count]
    for i, beat in enumerate(beats):
        beat["flipFrom"] = edges[i]
        beat["flipTo"] = edges[i + 1] - 1
        beat["flipTicks"] = 8
    write_json(engine_cuts / "alley-dana.json", alley_json)

    for folder in sorted(p for p in cut_root.iterdir() if p.is_dir()):
        if folder.name == "cs_alley_dana_taken":
            continue
        frames = sorted(folder.glob("f*.png"))
        if not frames:
            continue
        dest = engine_cuts / folder.name
        if dest.exists():
            shutil.rmtree(dest)
        dest.mkdir()
        for index, frame in enumerate(frames):
            shutil.copyfile(frame, dest / f"f{index:03}.png")
        write_json(
            engine_cuts / f"{folder.name}.json",
            {"id": folder.name, "beats": [], "frameTicks": [8]},
        )

    title = pixel / "stills" / "card_title.png"
    public = out.parent.parent / "web" / "public"
    if title.is_file() and public.is_dir():
        shutil.copyfile(title, public / "charlie-title.png")

    write_pixel_refs(out)
    print(f"pixel art imported into {out}")
    return 0


def frame_of(actor_dir: Path, costume: dict, anim: str) -> tuple[Image.Image, int, int]:
    for strip in costume["sheets"]:
        spec = strip["animations"].get(anim)
        if not spec:
            continue
        image = Image.open(actor_dir / strip["sheet"]).convert("RGBA")
        index = spec["frames"][0]
        fw, fh = strip["frameW"], strip["frameH"]
        crop = image.crop((index * fw, 0, (index + 1) * fw, fh))
        return crop, strip["ax"], strip["ay"]
    raise SystemExit(f"missing animation {anim} in {actor_dir}")


def write_pixel_refs(out: Path) -> None:
    pixel_dir = out / "tests" / "pixel"
    pixel_dir.mkdir(parents=True, exist_ok=True)
    hero = load_json(out / "actors" / "charlie" / "actor.json")
    hero_dir = out / "actors" / "charlie"
    for room_id in ("bar", "club", "hall", "kitchen"):
        room_dir = out / "rooms" / room_id
        room = load_json(room_dir / "room.json")
        plate = Image.open(room_dir / "picture.png").convert("RGBA")
        fb = bytearray(plate.tobytes())
        priority = [band_for(y) for y in range(H) for _ in range(W)]
        layers = []
        spawn = room["spawns"]["default"]
        sprite, ax, ay = frame_of(hero_dir, hero["costumes"]["default"], "stand_n")
        layers.append(("hero", band_for(spawn[1]), spawn[1], spawn[0] - ax, spawn[1] - ay, sprite, None))
        for place in room.get("actors") or []:
            actor = load_json(out / "actors" / place["actor"] / "actor.json")
            anim = place.get("anim") or "stand_s"
            sprite, ax, ay = frame_of(out / "actors" / place["actor"], actor["costumes"]["default"], anim)
            feet_x, feet_y = place["pos"]
            mask = None
            if place.get("occlude"):
                mask_img = Image.open(room_dir / f"occlude-{place['occlude']}.png").convert("L")
                mask = list(mask_img.tobytes())
            layers.append(
                (
                    f"actor:{place['id']}",
                    band_for(feet_y),
                    feet_y,
                    feet_x - ax,
                    feet_y - ay,
                    sprite,
                    mask,
                )
            )
        layers.sort(key=lambda item: (item[1], item[2], item[0]))
        scratch = bytearray(len(fb))
        for _id, priority_value, _y, dx, dy, sprite, mask in layers:
            blit(fb, priority, sprite, dx, dy, priority_value, scratch if mask else None, mask)
        Image.frombytes("RGBA", (W, H), bytes(fb)).save(pixel_dir / f"{room_id}.png")


if __name__ == "__main__":
    raise SystemExit(main())
