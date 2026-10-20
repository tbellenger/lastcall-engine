import init, { ProjectFiles, Studio } from "./wasm/lastcall_studio.js";

const W = 384;
const H = 216;

type Pt = [number, number];
type Rect = [number, number, number, number];
type Polygon = { points: Pt[]; holes?: Pt[][] };
type Hotspot = {
  id: string;
  rect: Rect;
  nouns?: string[];
  look?: string;
  stand?: Pt | null;
  face?: string | null;
};
type Exit = {
  id: string;
  trigger: Rect;
  to: string;
  spawn: string;
  rect?: Rect | null;
  command?: string | null;
  blockedIf?: string | null;
  blockedText?: string | null;
  approach?: Pt | null;
};
type Band = { y: number; priority: number };
type Room = {
  id: string;
  name: string;
  polygons: Polygon[];
  bands: Band[];
  hotspots: Hotspot[];
  exits: Exit[];
  spawns: Record<string, Pt>;
  polygonsLive: boolean;
};
type LayerName = "walk" | "priority" | "hotspots" | "stands" | "exits" | "spawns";
type Tool = "select" | "polygon" | "vertex" | "hotspot" | "exit" | "band" | "spawn";
type Sel =
  | { t: "vertex"; poly: number; hole: number; index: number }
  | { t: "poly"; index: number }
  | { t: "hotspot"; index: number }
  | { t: "exit"; index: number; part: "trigger" | "rect" }
  | { t: "band"; index: number }
  | { t: "spawn"; name: string }
  | { t: "stand"; index: number };

const layers: Record<LayerName, boolean> = {
  walk: true,
  priority: true,
  hotspots: true,
  stands: true,
  exits: true,
  spawns: true,
};

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const titleEl = $("title");
const roomsEl = $("rooms");
const inspectorEl = $("inspector");
const statusEl = $("status");
const view = $<HTMLCanvasElement>("view");
const hoverEl = $("hover");
const ctx = view.getContext("2d")!;
const off = document.createElement("canvas");
off.width = W;
off.height = H;
const octx = off.getContext("2d")!;
const heroLayer = document.createElement("canvas");
heroLayer.width = W;
heroLayer.height = H;
const hctx = heroLayer.getContext("2d")!;

let studio: Studio | null = null;
let room: Room | null = null;
let currentId = "";
let picture: ImageBitmap | null = null;
let mask = new Uint8Array(W * H);
let priority = new Uint8Array(W * H);
let maskCanvas: HTMLCanvasElement | null = null;
let priCanvas: HTMLCanvasElement | null = null;
let tool: Tool = "select";
let sel: Sel | null = null;
let draft: Pt[] = [];
let scale = 2;
let dirHandle: DirectoryHandle | null = null;
let history: string[] = [];
let cursor = -1;
const histories = new Map<string, { stack: string[]; cursor: number }>();
let inspectorKey = "";
let feet: Pt | null = null;
let path: Pt[] = [];
let walking = 0;
let drag: Drag | null = null;
let hover = "";

type Drag =
  | { kind: "vertex"; poly: number; hole: number; index: number }
  | { kind: "move-rect"; rect: Rect; dx: number; dy: number; write: (rect: Rect) => void }
  | { kind: "resize-rect"; rect: Rect; handle: Handle; write: (rect: Rect) => void }
  | { kind: "band"; index: number }
  | { kind: "point"; write: (x: number, y: number) => void };
type Handle = "nw" | "ne" | "sw" | "se";

type DirectoryHandle = {
  getDirectoryHandle(name: string, opts?: { create?: boolean }): Promise<DirectoryHandle>;
  getFileHandle(name: string, opts?: { create?: boolean }): Promise<FileHandle>;
  removeEntry(name: string, opts?: { recursive?: boolean }): Promise<void>;
};
type FileHandle = {
  getFile(): Promise<File>;
  createWritable(): Promise<{ write(data: BufferSource): Promise<void>; close(): Promise<void> }>;
};

await init();

$("open").onclick = () => $<HTMLInputElement>("file").click();
$("open-folder").onclick = () => {
  void openFolder();
};
$("undo").onclick = () => undo();
$("redo").onclick = () => redo();
$("ghost").onclick = () => {
  const btn = $("ghost");
  btn.classList.toggle("on");
  if (!btn.classList.contains("on")) {
    feet = null;
    path = [];
    window.clearTimeout(walking);
    draw();
  }
};
$("save").onclick = () => void save(false);
$("download-pak").onclick = () => void save(true);
$("del").onclick = () => deleteSelection();

for (const button of document.querySelectorAll<HTMLButtonElement>(".layer")) {
  button.onclick = () => {
    const name = button.dataset.layer as LayerName;
    layers[name] = !layers[name];
    button.classList.toggle("on", layers[name]);
    draw();
  };
}
for (const button of document.querySelectorAll<HTMLButtonElement>(".tool")) {
  button.onclick = () => setTool(button.dataset.tool as Tool);
}

$<HTMLInputElement>("file").onchange = () => {
  const file = $<HTMLInputElement>("file").files?.[0];
  if (file) void openBlob(file);
};
const dirInput = $<HTMLInputElement>("dir");
dirInput.multiple = true;
dirInput.setAttribute("webkitdirectory", "");
dirInput.onchange = () => {
  const files = [...(dirInput.files ?? [])];
  if (files.length) void openFileList(files);
};

window.addEventListener("dragover", (event) => event.preventDefault());
window.addEventListener("drop", (event) => {
  event.preventDefault();
  void openDrop(event.dataTransfer);
});
window.addEventListener("keydown", onKey);
window.addEventListener("resize", () => {
  fit();
  draw();
});

view.addEventListener("pointerdown", onDown);
view.addEventListener("pointermove", onMove);
view.addEventListener("pointerup", onUp);
view.addEventListener("pointerleave", () => {
  hoverEl.hidden = true;
});
view.addEventListener("dblclick", (event) => {
  if (tool === "polygon" && draft.length >= 3) {
    event.preventDefault();
    closeDraft();
  }
});

fit();
draw();

function setTool(next: Tool) {
  tool = next;
  draft = [];
  for (const button of document.querySelectorAll<HTMLButtonElement>(".tool")) {
    button.classList.toggle("on", button.dataset.tool === tool);
  }
  draw();
}

function setStatus(text: string, error = false) {
  statusEl.textContent = text;
  statusEl.classList.toggle("error", error);
}

async function openBlob(file: File) {
  setStatus(`Opening ${file.name}…`);
  try {
    const bytes = new Uint8Array(await file.arrayBuffer());
    adopt(Studio.open(bytes), null);
  } catch (error) {
    setStatus(String(error), true);
  }
}

async function openFileList(files: File[]) {
  if (files.length === 1 && /\.(advpak|zip)$/i.test(files[0].name)) {
    await openBlob(files[0]);
    return;
  }
  setStatus(`Opening ${files.length} files…`);
  try {
    const project = new ProjectFiles();
    for (const file of files) {
      const rel = file.webkitRelativePath || file.name;
      project.add(rel, new Uint8Array(await file.arrayBuffer()));
    }
    adopt(project.finish(), null);
  } catch (error) {
    setStatus(String(error), true);
  }
}

async function openFolder() {
  const picker = (
    window as unknown as {
      showDirectoryPicker?: (opts?: { mode?: string }) => Promise<DirectoryHandle>;
    }
  ).showDirectoryPicker;
  if (!picker) {
    dirInput.click();
    return;
  }
  try {
    const handle = await picker({ mode: "readwrite" });
    setStatus("Reading folder…");
    const project = new ProjectFiles();
    await walkHandle(handle, "", project);
    adopt(project.finish(), handle);
  } catch (error) {
    if (error instanceof DOMException && error.name === "AbortError") return;
    setStatus(String(error), true);
  }
}

async function walkHandle(handle: DirectoryHandle, prefix: string, project: ProjectFiles) {
  const directory = handle as DirectoryHandle & {
    entries(): AsyncIterable<[string, { kind: string; getFile?: () => Promise<File> }]>;
  };
  for await (const [name, child] of directory.entries()) {
    const path = prefix ? `${prefix}/${name}` : name;
    if (child.kind === "file" && child.getFile) {
      const file = await child.getFile();
      project.add(path, new Uint8Array(await file.arrayBuffer()));
    } else {
      await walkHandle(child as unknown as DirectoryHandle, path, project);
    }
  }
}

async function openDrop(transfer: DataTransfer | null) {
  if (!transfer) return;
  const items = [...transfer.items];
  const entries = items
    .map((item) =>
      (item as DataTransferItem & { webkitGetAsEntry?: () => FileSystemEntry | null }).webkitGetAsEntry?.(),
    )
    .filter((entry): entry is FileSystemEntry => !!entry);
  if (entries.length) {
    setStatus("Reading drop…");
    try {
      const project = new ProjectFiles();
      let openedPak = false;
      for (const entry of entries) {
        if (entry.isFile && /\.(advpak|zip)$/i.test(entry.name) && entries.length === 1) {
          const file = await fileOf(entry as FileSystemFileEntry);
          await openBlob(file);
          openedPak = true;
          break;
        }
        await addEntry(entry, "", project);
      }
      if (!openedPak) adopt(project.finish(), null);
    } catch (error) {
      setStatus(String(error), true);
    }
    return;
  }
  const files = [...transfer.files];
  if (files.length) await openFileList(files);
}

async function addEntry(entry: FileSystemEntry, prefix: string, project: ProjectFiles) {
  const path = prefix ? `${prefix}/${entry.name}` : entry.name;
  if (entry.isFile) {
    const file = await fileOf(entry as FileSystemFileEntry);
    project.add(path, new Uint8Array(await file.arrayBuffer()));
    return;
  }
  const reader = (entry as FileSystemDirectoryEntry).createReader();
  const children: FileSystemEntry[] = [];
  for (;;) {
    const batch = await new Promise<FileSystemEntry[]>((resolve, reject) => reader.readEntries(resolve, reject));
    if (!batch.length) break;
    children.push(...batch);
  }
  for (const child of children) await addEntry(child, path, project);
}

function fileOf(entry: FileSystemFileEntry) {
  return new Promise<File>((resolve, reject) => entry.file(resolve, reject));
}

function adopt(next: Studio, folder: DirectoryHandle | null) {
  studio = next;
  dirHandle = folder;
  titleEl.textContent = `${next.title()} (${next.game_id()})`;
  feet = null;
  path = [];
  sel = null;
  draft = [];
  const rooms = JSON.parse(next.rooms()) as { id: string; name: string }[];
  const preferred = rooms.find((item) => item.id === "bar") ?? rooms[0];
  $<HTMLButtonElement>("save").disabled = false;
  $<HTMLButtonElement>("download-pak").disabled = false;
  if (!preferred) {
    setStatus("This game has no rooms.", true);
    return;
  }
  selectRoom(preferred.id);
  setStatus(
    next.is_pak()
      ? "Package open. Save downloads a new .advpak. Unedited saves keep the original bytes."
      : folder
        ? "Project folder open. Save writes the changed JSON back into that folder."
        : "Project open. Save downloads a zip of the authoring files.",
  );
}

function selectRoom(id: string) {
  if (!studio) return;
  if (currentId && history.length) histories.set(currentId, { stack: history, cursor });
  currentId = id;
  room = JSON.parse(studio.room(id)) as Room;
  const saved = histories.get(id);
  history = saved?.stack ?? [JSON.stringify(room)];
  cursor = saved?.cursor ?? 0;
  sel = null;
  inspectorKey = "";
  draft = [];
  feet = null;
  path = [];
  void loadPicture();
  refreshRasters();
  renderRooms();
  renderInspector(true);
  updateUndo();
  fit();
  draw();
}

async function loadPicture() {
  if (!studio || !room) return;
  const bytes = studio.picture(room.id);
  const copy = new Uint8Array(bytes);
  const blob = new Blob([copy], { type: "image/png" });
  picture = await createImageBitmap(blob);
  draw();
}

function refreshRasters() {
  if (!studio || !room) return;
  mask = new Uint8Array(studio.mask(room.id));
  priority = new Uint8Array(studio.priority(room.id));
  maskCanvas = tint(mask, (value) => (value ? [80, 180, 90, 255] : [0, 0, 0, 0]));
  priCanvas = tint(priority, (value) => {
    if (!value) return [0, 0, 0, 0];
    const hue = (value * 17) % 360;
    return [...hsl(hue), 255];
  });
}

function tint(bytes: Uint8Array, color: (value: number) => number[]) {
  const canvas = document.createElement("canvas");
  canvas.width = W;
  canvas.height = H;
  const image = new ImageData(W, H);
  for (let i = 0; i < W * H; i++) {
    const [r, g, b, a] = color(bytes[i] ?? 0);
    image.data[i * 4] = r;
    image.data[i * 4 + 1] = g;
    image.data[i * 4 + 2] = b;
    image.data[i * 4 + 3] = a;
  }
  canvas.getContext("2d")!.putImageData(image, 0, 0);
  return canvas;
}

function hsl(hue: number): [number, number, number] {
  const s = 0.45;
  const l = 0.55;
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const x = c * (1 - Math.abs(((hue / 60) % 2) - 1));
  const m = l - c / 2;
  let r = 0;
  let g = 0;
  let b = 0;
  if (hue < 60) [r, g, b] = [c, x, 0];
  else if (hue < 120) [r, g, b] = [x, c, 0];
  else if (hue < 180) [r, g, b] = [0, c, x];
  else if (hue < 240) [r, g, b] = [0, x, c];
  else if (hue < 300) [r, g, b] = [x, 0, c];
  else [r, g, b] = [c, 0, x];
  return [Math.round((r + m) * 255), Math.round((g + m) * 255), Math.round((b + m) * 255)];
}

function renderRooms() {
  if (!studio) return;
  const rooms = JSON.parse(studio.rooms()) as { id: string; name: string }[];
  roomsEl.replaceChildren();
  for (const item of rooms) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "room" + (item.id === currentId ? " on" : "");
    button.dataset.room = item.id;
    button.innerHTML = `${escapeHtml(item.name)}<small>${escapeHtml(item.id)}</small>`;
    button.onclick = () => selectRoom(item.id);
    roomsEl.append(button);
  }
}

function renderInspector(force = false) {
  const key = sel ? JSON.stringify(sel) : "room";
  if (!force && key === inspectorKey) return;
  inspectorKey = key;
  inspectorEl.replaceChildren();
  if (!room) {
    inspectorEl.innerHTML = `<p class="empty">Open a game to edit a room.</p>`;
    return;
  }
  const head = document.createElement("h2");
  head.textContent = sel ? selTitle(sel) : room.name;
  inspectorEl.append(head);
  if (!sel) {
    inspectorEl.append(field("Name", room.name, (value) => {
      room!.name = value;
      changed();
    }));
    inspectorEl.append(field("Id", room.id, (value) => {
      room!.id = value;
      changed();
      renderRooms();
    }, "change"));
    const note = document.createElement("p");
    note.textContent = room.polygonsLive
      ? "Polygons rasterise to the walk mask. The ghost walks that mask with the engine's A*."
      : "Showing the packed walk mask. Draw or drag a polygon to replace it.";
    inspectorEl.append(note);
    return;
  }
  if (sel.t === "hotspot") fillHotspot(room.hotspots[sel.index]);
  else if (sel.t === "exit") fillExit(room.exits[sel.index]);
  else if (sel.t === "band") fillBand(room.bands[sel.index]);
  else if (sel.t === "spawn") fillSpawn(sel.name);
  else if (sel.t === "stand") fillStand(room.hotspots[sel.index]);
  else if (sel.t === "vertex") {
    const pt = vertexAt(sel);
    if (!pt) return;
    const row = document.createElement("div");
    row.className = "row";
    row.append(num("X", pt[0], (value) => moveVertex(sel as { t: "vertex"; poly: number; hole: number; index: number }, value, pt[1])));
    row.append(num("Y", pt[1], (value) => moveVertex(sel as { t: "vertex"; poly: number; hole: number; index: number }, pt[0], value)));
    inspectorEl.append(row);
  } else if (sel.t === "poly") {
    const poly = room.polygons[sel.index];
    const note = document.createElement("p");
    note.textContent = `${poly?.points.length ?? 0} vertices. Drag them on the picture. Delete removes the polygon.`;
    inspectorEl.append(note);
  }
}

function fillHotspot(hotspot: Hotspot | undefined) {
  if (!hotspot || !room) return;
  inspectorEl.append(field("Id", hotspot.id, (value) => {
    hotspot.id = value;
    changed();
  }));
  inspectorEl.append(field("Nouns", (hotspot.nouns ?? []).join(", "), (value) => {
    hotspot.nouns = value.split(",").map((part) => part.trim()).filter(Boolean);
    changed();
  }));
  inspectorEl.append(area("Look", hotspot.look ?? "", (value) => {
    hotspot.look = value;
    changed();
  }));
  inspectorEl.append(rectFields("Rect", hotspot.rect, (rect) => {
    hotspot.rect = rect;
    changed();
  }));
}

function fillExit(exit: Exit | undefined) {
  if (!exit) return;
  inspectorEl.append(field("Id", exit.id, (value) => {
    exit.id = value;
    changed();
  }));
  inspectorEl.append(field("To room", exit.to, (value) => {
    exit.to = value;
    changed();
  }));
  inspectorEl.append(field("Spawn", exit.spawn, (value) => {
    exit.spawn = value;
    changed();
  }));
  inspectorEl.append(field("Command", exit.command ?? "", (value) => {
    exit.command = value || null;
    changed();
  }));
  inspectorEl.append(field("Blocked if", exit.blockedIf ?? "", (value) => {
    exit.blockedIf = value || null;
    changed();
  }));
  inspectorEl.append(area("Blocked text", exit.blockedText ?? "", (value) => {
    exit.blockedText = value || null;
    changed();
  }));
  inspectorEl.append(rectFields("Trigger", exit.trigger, (rect) => {
    exit.trigger = rect;
    changed();
  }));
  if (exit.rect) {
    inspectorEl.append(rectFields("Picture rect", exit.rect, (rect) => {
      exit.rect = rect;
      changed();
    }));
  }
}

function fillBand(band: Band | undefined) {
  if (!band) return;
  const row = document.createElement("div");
  row.className = "row";
  row.append(num("Y", band.y, (value) => {
    band.y = clamp(value, 0, H);
    changed();
  }));
  row.append(num("Priority", band.priority, (value) => {
    band.priority = clamp(value, 0, 255);
    changed();
  }));
  inspectorEl.append(row);
}

function fillSpawn(name: string) {
  if (!room) return;
  const pt = room.spawns[name];
  if (!pt) return;
  inspectorEl.append(field("Name", name, (value) => {
    if (!value || value === name) return;
    room!.spawns[value] = room!.spawns[name];
    delete room!.spawns[name];
    sel = { t: "spawn", name: value };
    changed();
    renderInspector(true);
  }, "change"));
  const row = document.createElement("div");
  row.className = "row";
  row.append(num("X", pt[0], (value) => {
    pt[0] = snap(value);
    changed();
  }));
  row.append(num("Y", pt[1], (value) => {
    pt[1] = snap(value);
    changed();
  }));
  inspectorEl.append(row);
}

function fillStand(hotspot: Hotspot | undefined) {
  const stand = hotspot?.stand;
  if (!stand) return;
  const row = document.createElement("div");
  row.className = "row";
  row.append(num("Stand X", stand[0], (value) => {
    stand[0] = snap(value);
    changed();
  }));
  row.append(num("Stand Y", stand[1], (value) => {
    stand[1] = snap(value);
    changed();
  }));
  inspectorEl.append(row);
}

function field(label: string, value: string, on: (value: string) => void, event: "input" | "change" = "input") {
  const wrap = document.createElement("label");
  wrap.textContent = label;
  const input = document.createElement("input");
  input.value = value;
  input.addEventListener(event, () => on(input.value));
  wrap.append(input);
  return wrap;
}

function area(label: string, value: string, on: (value: string) => void) {
  const wrap = document.createElement("label");
  wrap.textContent = label;
  const input = document.createElement("textarea");
  input.value = value;
  input.addEventListener("input", () => on(input.value));
  wrap.append(input);
  return wrap;
}

function num(label: string, value: number, on: (value: number) => void) {
  const wrap = document.createElement("label");
  wrap.textContent = label;
  const input = document.createElement("input");
  input.type = "number";
  input.value = String(value);
  input.addEventListener("change", () => on(Number(input.value)));
  wrap.append(input);
  return wrap;
}

function rectFields(label: string, rect: Rect, on: (rect: Rect) => void) {
  const wrap = document.createElement("div");
  const caption = document.createElement("div");
  caption.className = "label";
  caption.textContent = label;
  wrap.append(caption);
  const row = document.createElement("div");
  row.className = "row";
  const names = ["X", "Y", "W", "H"] as const;
  names.forEach((name, index) => {
    row.append(num(name, rect[index], (value) => {
      rect[index] = snap(value);
      if (index >= 2) rect[index] = Math.max(1, rect[index]);
      on(rect);
    }));
  });
  wrap.append(row);
  return wrap;
}

function changed() {
  try {
    sync();
    commit();
    draw();
  } catch (error) {
    setStatus(String(error), true);
  }
}

function sync() {
  if (!studio || !room) return;
  const prev = currentId;
  studio.set_room(prev, JSON.stringify(room));
  currentId = room.id;
  refreshRasters();
  if (feet) paintHero();
}

function commit() {
  if (!room) return;
  const json = JSON.stringify(room);
  if (history[cursor] === json) return;
  history = history.slice(0, cursor + 1);
  history.push(json);
  cursor = history.length - 1;
  updateUndo();
}

function undo() {
  if (cursor <= 0 || !studio) return;
  cursor -= 1;
  restore(history[cursor]);
}

function redo() {
  if (cursor + 1 >= history.length || !studio) return;
  cursor += 1;
  restore(history[cursor]);
}

function restore(json: string) {
  if (!studio || !room) return;
  const prev = currentId;
  studio.set_room(prev, json);
  room = JSON.parse(json) as Room;
  currentId = room.id;
  room = JSON.parse(studio.room(currentId)) as Room;
  refreshRasters();
  renderRooms();
  renderInspector(true);
  updateUndo();
  draw();
}

function updateUndo() {
  $<HTMLButtonElement>("undo").disabled = cursor <= 0;
  $<HTMLButtonElement>("redo").disabled = cursor + 1 >= history.length;
  if (studio) $("save").textContent = studio.dirty() ? "Save" : "Save";
}

function fit() {
  const stage = $("stage");
  const availW = Math.max(1, stage.clientWidth - 24);
  const availH = Math.max(1, stage.clientHeight - 24);
  scale = Math.max(1, Math.min(Math.floor(availW / W), Math.floor(availH / H)));
  view.dataset.scale = String(scale);
}

function draw() {
  fit();
  octx.imageSmoothingEnabled = false;
  octx.clearRect(0, 0, W, H);
  if (picture) octx.drawImage(picture, 0, 0);
  if (layers.priority && priCanvas) {
    octx.globalAlpha = 0.28;
    octx.drawImage(priCanvas, 0, 0);
    octx.globalAlpha = 1;
  }
  if (layers.walk && maskCanvas) {
    octx.globalAlpha = 0.38;
    octx.drawImage(maskCanvas, 0, 0);
    octx.globalAlpha = 1;
  }
  if (feet && $("ghost").classList.contains("on")) octx.drawImage(heroLayer, 0, 0);
  view.width = W * scale;
  view.height = H * scale;
  ctx.imageSmoothingEnabled = false;
  ctx.drawImage(off, 0, 0, view.width, view.height);
  if (!room) return;
  if (layers.walk) strokePolys();
  if (layers.priority) strokeBands();
  if (layers.hotspots) strokeHotspots();
  if (layers.exits) strokeExits();
  if (layers.spawns || layers.stands) strokePoints();
  if (draft.length) strokeDraft();
  if (path.length) strokePath();
}

function strokePolys() {
  if (!room) return;
  ctx.lineWidth = 1;
  room.polygons.forEach((poly, index) => {
    traceRing(poly.points, sel?.t === "poly" && sel.index === index ? "#f2d65c" : "#d7ffe2");
    for (const hole of poly.holes ?? []) traceRing(hole, "#ffb4b0");
    poly.points.forEach((pt, vi) => dot(pt, vertexSelected(index, -1, vi)));
    (poly.holes ?? []).forEach((hole, hi) => hole.forEach((pt, vi) => dot(pt, vertexSelected(index, hi, vi))));
  });
}

function traceRing(points: Pt[], color: string) {
  if (points.length < 2) return;
  ctx.beginPath();
  points.forEach((pt, index) => {
    const [x, y] = px(pt);
    if (index === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  });
  ctx.closePath();
  ctx.strokeStyle = color;
  ctx.stroke();
}

function strokeBands() {
  if (!room) return;
  ctx.font = "12px sans-serif";
  for (const [index, band] of room.bands.entries()) {
    const y = band.y * scale + 0.5;
    ctx.strokeStyle = sel?.t === "band" && sel.index === index ? "#f2d65c" : "rgba(180, 200, 255, 0.9)";
    ctx.beginPath();
    ctx.moveTo(0, y);
    ctx.lineTo(view.width, y);
    ctx.stroke();
    ctx.fillStyle = "#d5def8";
    ctx.fillText(String(band.priority), 6, Math.max(12, y - 2));
  }
}

function strokeHotspots() {
  if (!room) return;
  room.hotspots.forEach((hotspot, index) => {
    box(hotspot.rect, sel?.t === "hotspot" && sel.index === index ? "#f2d65c" : "#ff9a4a");
  });
}

function strokeExits() {
  if (!room) return;
  room.exits.forEach((exit, index) => {
    box(exit.trigger, sel?.t === "exit" && sel.index === index && sel.part === "trigger" ? "#f2d65c" : "#7ee0ff");
    if (exit.rect) box(exit.rect, sel?.t === "exit" && sel.index === index && sel.part === "rect" ? "#f2d65c" : "#2f6f86");
  });
}

function strokePoints() {
  if (!room) return;
  if (layers.spawns) {
    for (const [name, pt] of Object.entries(room.spawns)) {
      diamond(pt, sel?.t === "spawn" && sel.name === name ? "#f2d65c" : "#ff7ad9");
      const [x, y] = px(pt);
      ctx.fillStyle = "#ffd6f2";
      ctx.font = "12px sans-serif";
      ctx.fillText(name, x + 6, y - 4);
    }
  }
  if (layers.stands) {
    room.hotspots.forEach((hotspot, index) => {
      if (hotspot.stand) dot(hotspot.stand, sel?.t === "stand" && sel.index === index, "#ffe27a");
    });
  }
}

function strokeDraft() {
  traceRing(draft, "#f2d65c");
  for (const pt of draft) dot(pt, false);
}

function strokePath() {
  ctx.beginPath();
  path.forEach((pt, index) => {
    const [x, y] = px(pt);
    if (index === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  });
  ctx.strokeStyle = "#f2d65c";
  ctx.lineWidth = 2;
  ctx.stroke();
  ctx.lineWidth = 1;
}

function box(rect: Rect, color: string) {
  const [x, y, w, h] = rect;
  ctx.strokeStyle = color;
  ctx.strokeRect(x * scale + 0.5, y * scale + 0.5, w * scale, h * scale);
}

function dot(pt: Pt, on: boolean, color = "#ffffff") {
  const [x, y] = px(pt);
  ctx.fillStyle = on ? "#f2d65c" : color;
  ctx.fillRect(x - 2, y - 2, 5, 5);
}

function diamond(pt: Pt, color: string) {
  const [x, y] = px(pt);
  ctx.beginPath();
  ctx.moveTo(x, y - 5);
  ctx.lineTo(x + 5, y);
  ctx.lineTo(x, y + 5);
  ctx.lineTo(x - 5, y);
  ctx.closePath();
  ctx.fillStyle = color;
  ctx.fill();
}

function px(pt: Pt): Pt {
  return [pt[0] * scale + scale / 2, pt[1] * scale + scale / 2];
}

function vertexSelected(poly: number, hole: number, index: number) {
  return sel?.t === "vertex" && sel.poly === poly && sel.hole === hole && sel.index === index;
}

function onDown(event: PointerEvent) {
  if (!room || !studio) return;
  view.setPointerCapture(event.pointerId);
  const [x, y] = gamePoint(event);
  if ($("ghost").classList.contains("on") && tool === "select" && !event.shiftKey) {
    placeGhost(x, y);
    return;
  }
  if (tool === "polygon") {
    draft.push([x, y]);
    draw();
    return;
  }
  if (tool === "vertex") {
    insertVertex(x, y);
    return;
  }
  if (tool === "hotspot") {
    addHotspot(x, y);
    return;
  }
  if (tool === "exit") {
    addExit(x, y);
    return;
  }
  if (tool === "band") {
    room.bands.push({ y, priority: 8 });
    sel = { t: "band", index: room.bands.length - 1 };
    changed();
    renderInspector(true);
    return;
  }
  if (tool === "spawn") {
    const name = unique("spot", Object.keys(room.spawns));
    room.spawns[name] = [x, y];
    sel = { t: "spawn", name };
    changed();
    renderInspector(true);
    return;
  }
  const hit = hitTest(x, y);
  sel = hit.sel;
  drag = hit.drag;
  renderInspector(true);
  draw();
}

function onMove(event: PointerEvent) {
  const [x, y] = gamePoint(event);
  if (drag && room) {
    applyDrag(x, y);
    try {
      sync();
    } catch (error) {
      setStatus(String(error), true);
    }
    draw();
  }
  const name = hitTest(x, y).label;
  hover = name ? `${name}  ${x}, ${y}` : `${x}, ${y}`;
  hoverEl.hidden = false;
  hoverEl.textContent = hover;
  const stage = $("stage").getBoundingClientRect();
  hoverEl.style.left = `${event.clientX - stage.left + 12}px`;
  hoverEl.style.top = `${event.clientY - stage.top + 12}px`;
}

function onUp() {
  if (drag) {
    drag = null;
    commit();
    renderInspector(true);
  }
}

function placeGhost(x: number, y: number) {
  if (!studio || !room) return;
  const onMask = mask[y * W + x] !== 0;
  const from = feet;
  const fromOk = !!from && mask[from[1] * W + from[0]] !== 0;
  if (!onMask || !fromOk) {
    feet = [x, y];
    path = [];
    paintHero();
    draw();
    setStatus(onMask ? `Hero at ${x}, ${y}. Click a walkable spot to path.` : `Hero at ${x}, ${y} (off the walk mask).`);
    return;
  }
  path = JSON.parse(studio.plan_path(room.id, from![0], from![1], x, y)) as Pt[];
  if (path.length < 2) {
    feet = [x, y];
    paintHero();
    draw();
    setStatus("No path on the edited walk mask.");
    return;
  }
  window.clearTimeout(walking);
  let step = 0;
  const tick = () => {
    feet = path[step];
    paintHero();
    draw();
    step += 1;
    if (step < path.length) walking = window.setTimeout(tick, 16);
  };
  tick();
  setStatus(`Walking ${path.length} steps on the edited mask.`);
}

function paintHero() {
  if (!studio || !room || !feet) return;
  const rgba = studio.hero_overlay(room.id, feet[0], feet[1]);
  const image = new ImageData(new Uint8ClampedArray(rgba), W, H);
  hctx.putImageData(image, 0, 0);
}

function applyDrag(x: number, y: number) {
  if (!drag || !room) return;
  if (drag.kind === "vertex") {
    moveVertex(drag, x, y);
    return;
  }
  if (drag.kind === "band") {
    room.bands[drag.index].y = clamp(y, 0, H);
    return;
  }
  if (drag.kind === "point") {
    drag.write(x, y);
    return;
  }
  if (drag.kind === "move-rect") {
    const [rx, ry, rw, rh] = drag.rect;
    drag.write([snap(x - drag.dx), snap(y - drag.dy), rw, rh]);
    return;
  }
  const [rx, ry, rw, rh] = drag.rect;
  const right = rx + rw;
  const bottom = ry + rh;
  let next: Rect = [rx, ry, rw, rh];
  if (drag.handle === "nw") next = [x, y, Math.max(1, right - x), Math.max(1, bottom - y)];
  if (drag.handle === "ne") next = [rx, y, Math.max(1, x - rx), Math.max(1, bottom - y)];
  if (drag.handle === "sw") next = [x, ry, Math.max(1, right - x), Math.max(1, y - ry)];
  if (drag.handle === "se") next = [rx, ry, Math.max(1, x - rx), Math.max(1, y - ry)];
  drag.write(next.map((value) => snap(value)) as Rect);
}

function moveVertex(which: { poly: number; hole: number; index: number }, x: number, y: number) {
  const poly = room?.polygons[which.poly];
  if (!poly) return;
  const ring = which.hole < 0 ? poly.points : poly.holes?.[which.hole];
  if (!ring || !ring[which.index]) return;
  ring[which.index] = [snap(x), snap(y)];
}

function onKey(event: KeyboardEvent) {
  const target = event.target as HTMLElement | null;
  if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA")) return;
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "z") {
    event.preventDefault();
    if (event.shiftKey) redo();
    else undo();
    return;
  }
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "y") {
    event.preventDefault();
    redo();
    return;
  }
  if (event.key === "Escape") {
    draft = [];
    draw();
    return;
  }
  if (event.key === "Enter" && tool === "polygon") {
    closeDraft();
    return;
  }
  if (event.key === "Delete" || event.key === "Backspace") {
    deleteSelection();
    return;
  }
  const nudge: Record<string, Pt> = {
    ArrowLeft: [-1, 0],
    ArrowRight: [1, 0],
    ArrowUp: [0, -1],
    ArrowDown: [0, 1],
  };
  const delta = nudge[event.key];
  if (delta && sel) {
    event.preventDefault();
    nudgeSel(delta[0], delta[1]);
  }
}

function nudgeSel(dx: number, dy: number) {
  if (!room || !sel) return;
  if (sel.t === "vertex") {
    const pt = vertexAt(sel);
    if (pt) moveVertex(sel, pt[0] + dx, pt[1] + dy);
  } else if (sel.t === "spawn") {
    const pt = room.spawns[sel.name];
    if (pt) {
      pt[0] += dx;
      pt[1] += dy;
    }
  } else if (sel.t === "stand") {
    const pt = room.hotspots[sel.index]?.stand;
    if (pt) {
      pt[0] += dx;
      pt[1] += dy;
    }
  } else if (sel.t === "band") room.bands[sel.index].y = clamp(room.bands[sel.index].y + dy, 0, H);
  else if (sel.t === "hotspot") shiftRect(room.hotspots[sel.index].rect, dx, dy);
  else if (sel.t === "exit") {
    const rect = sel.part === "rect" ? room.exits[sel.index].rect : room.exits[sel.index].trigger;
    if (rect) shiftRect(rect, dx, dy);
  }
  changed();
}

function shiftRect(rect: Rect, dx: number, dy: number) {
  rect[0] += dx;
  rect[1] += dy;
}

function deleteSelection() {
  if (!room || !sel) return;
  if (sel.t === "vertex") {
    const poly = room.polygons[sel.poly];
    const ring = sel.hole < 0 ? poly?.points : poly?.holes?.[sel.hole];
    if (ring && ring.length > 3) ring.splice(sel.index, 1);
    else setStatus("A ring needs at least 3 vertices.", true);
  } else if (sel.t === "poly") room.polygons.splice(sel.index, 1);
  else if (sel.t === "hotspot" || sel.t === "stand") room.hotspots.splice(sel.index, 1);
  else if (sel.t === "exit") room.exits.splice(sel.index, 1);
  else if (sel.t === "band") room.bands.splice(sel.index, 1);
  else if (sel.t === "spawn") delete room.spawns[sel.name];
  sel = null;
  changed();
  renderInspector(true);
}

function closeDraft() {
  if (!room || draft.length < 3) return;
  room.polygons.push({ points: draft.map((pt) => [pt[0], pt[1]]), holes: [] });
  sel = { t: "poly", index: room.polygons.length - 1 };
  draft = [];
  setTool("select");
  changed();
  renderInspector(true);
}

function insertVertex(x: number, y: number) {
  if (!room) return;
  const found: { best: { poly: number; hole: number; index: number; dist: number } | null } = { best: null };
  room.polygons.forEach((poly, polyIndex) => {
    consider(poly.points, polyIndex, -1);
    (poly.holes ?? []).forEach((hole, holeIndex) => consider(hole, polyIndex, holeIndex));
  });
  function consider(ring: Pt[], polyIndex: number, hole: number) {
    for (let i = 0; i < ring.length; i++) {
      const a = ring[i];
      const b = ring[(i + 1) % ring.length];
      const dist = segmentDistance(x, y, a, b);
      if (!found.best || dist < found.best.dist) found.best = { poly: polyIndex, hole, index: i + 1, dist };
    }
  }
  const best = found.best;
  if (!best || best.dist > 8) {
    setStatus("Click an edge to insert a vertex.", true);
    return;
  }
  const poly = room.polygons[best.poly];
  const ring = best.hole < 0 ? poly.points : poly.holes?.[best.hole];
  ring?.splice(best.index, 0, [x, y]);
  sel = { t: "vertex", poly: best.poly, hole: best.hole, index: best.index };
  changed();
  renderInspector(true);
}

function addHotspot(x: number, y: number) {
  if (!room) return;
  const id = unique("hotspot", room.hotspots.map((item) => item.id));
  room.hotspots.push({ id, rect: [x, y, 24, 24], nouns: [], look: "", stand: [x + 12, Math.min(H - 1, y + 28)] });
  sel = { t: "hotspot", index: room.hotspots.length - 1 };
  setTool("select");
  changed();
  renderInspector(true);
}

function addExit(x: number, y: number) {
  if (!room || !studio) return;
  const rooms = JSON.parse(studio.rooms()) as { id: string }[];
  const dest = rooms.find((item) => item.id !== room!.id)?.id ?? room.id;
  const id = unique("exit", room.exits.map((item) => item.id));
  room.exits.push({
    id,
    trigger: [x, y, 20, 12],
    to: dest,
    spawn: "default",
    rect: [x, Math.max(0, y - 36), 20, 36],
  });
  sel = { t: "exit", index: room.exits.length - 1, part: "trigger" };
  setTool("select");
  changed();
  renderInspector(true);
}

function hitTest(x: number, y: number): { sel: Sel | null; drag: Drag | null; label: string } {
  if (!room) return { sel: null, drag: null, label: "" };
  if (layers.walk) {
    for (let p = room.polygons.length - 1; p >= 0; p--) {
      const poly = room.polygons[p];
      const rings = [poly.points, ...(poly.holes ?? [])];
      for (let h = 0; h < rings.length; h++) {
        const hole = h === 0 ? -1 : h - 1;
        for (let i = 0; i < rings[h].length; i++) {
          const pt = rings[h][i];
          if (Math.abs(pt[0] - x) <= 3 && Math.abs(pt[1] - y) <= 3) {
            return {
              sel: { t: "vertex", poly: p, hole, index: i },
              drag: { kind: "vertex", poly: p, hole, index: i },
              label: `vertex ${i}`,
            };
          }
        }
      }
    }
  }
  if (layers.stands) {
    for (let i = room.hotspots.length - 1; i >= 0; i--) {
      const stand = room.hotspots[i].stand;
      if (stand && Math.abs(stand[0] - x) <= 3 && Math.abs(stand[1] - y) <= 3) {
        return {
          sel: { t: "stand", index: i },
          drag: { kind: "point", write: (px, py) => (room!.hotspots[i].stand = [px, py]) },
          label: `${room.hotspots[i].id} stand`,
        };
      }
    }
  }
  if (layers.spawns) {
    for (const [name, pt] of Object.entries(room.spawns)) {
      if (Math.abs(pt[0] - x) <= 4 && Math.abs(pt[1] - y) <= 4) {
        return {
          sel: { t: "spawn", name },
          drag: { kind: "point", write: (px, py) => (room!.spawns[name] = [px, py]) },
          label: `spawn ${name}`,
        };
      }
    }
  }
  if (layers.hotspots) {
    for (let i = room.hotspots.length - 1; i >= 0; i--) {
      const handle = rectHit(room.hotspots[i].rect, x, y);
      if (handle) {
        const hotspot = room.hotspots[i];
        return {
          sel: { t: "hotspot", index: i },
          drag: rectDrag(hotspot.rect, x, y, handle, (rect) => (hotspot.rect = rect)),
          label: hotspot.id,
        };
      }
    }
  }
  if (layers.exits) {
    for (let i = room.exits.length - 1; i >= 0; i--) {
      const exit = room.exits[i];
      if (exit.rect) {
        const handle = rectHit(exit.rect, x, y);
        if (handle) {
          return {
            sel: { t: "exit", index: i, part: "rect" },
            drag: rectDrag(exit.rect, x, y, handle, (rect) => (exit.rect = rect)),
            label: `${exit.id} picture`,
          };
        }
      }
      const handle = rectHit(exit.trigger, x, y);
      if (handle) {
        return {
          sel: { t: "exit", index: i, part: "trigger" },
          drag: rectDrag(exit.trigger, x, y, handle, (rect) => (exit.trigger = rect)),
          label: `${exit.id} → ${exit.to}`,
        };
      }
    }
  }
  if (layers.priority) {
    for (let i = room.bands.length - 1; i >= 0; i--) {
      if (Math.abs(room.bands[i].y - y) <= 2) {
        return {
          sel: { t: "band", index: i },
          drag: { kind: "band", index: i },
          label: `priority ${room.bands[i].priority}`,
        };
      }
    }
  }
  return { sel: null, drag: null, label: "" };
}

function rectHit(rect: Rect, x: number, y: number): Handle | "move" | null {
  const [rx, ry, rw, rh] = rect;
  const near = (px: number, py: number) => Math.abs(px - x) <= 3 && Math.abs(py - y) <= 3;
  if (near(rx, ry)) return "nw";
  if (near(rx + rw, ry)) return "ne";
  if (near(rx, ry + rh)) return "sw";
  if (near(rx + rw, ry + rh)) return "se";
  if (x >= rx && y >= ry && x < rx + rw && y < ry + rh) return "move";
  return null;
}

function rectDrag(rect: Rect, x: number, y: number, handle: Handle | "move", write: (rect: Rect) => void): Drag {
  if (handle === "move") {
    return { kind: "move-rect", rect: [...rect], dx: x - rect[0], dy: y - rect[1], write };
  }
  return { kind: "resize-rect", rect: [...rect], handle, write };
}

function vertexAt(sel: { poly: number; hole: number; index: number }) {
  const poly = room?.polygons[sel.poly];
  if (!poly) return null;
  const ring = sel.hole < 0 ? poly.points : poly.holes?.[sel.hole];
  return ring?.[sel.index] ?? null;
}

function selTitle(sel: Sel) {
  if (!room) return "Selection";
  if (sel.t === "hotspot") return room.hotspots[sel.index]?.id ?? "Hotspot";
  if (sel.t === "exit") return room.exits[sel.index]?.id ?? "Exit";
  if (sel.t === "spawn") return sel.name;
  if (sel.t === "band") return "Priority band";
  if (sel.t === "vertex") return "Vertex";
  if (sel.t === "stand") return "Stand point";
  return "Polygon";
}

function gamePoint(event: PointerEvent): Pt {
  const bounds = view.getBoundingClientRect();
  const x = Math.floor(((event.clientX - bounds.left) / bounds.width) * W);
  const y = Math.floor(((event.clientY - bounds.top) / bounds.height) * H);
  return [clamp(x, 0, W - 1), clamp(y, 0, H - 1)];
}

function segmentDistance(x: number, y: number, a: Pt, b: Pt) {
  const dx = b[0] - a[0];
  const dy = b[1] - a[1];
  const len = dx * dx + dy * dy || 1;
  let t = ((x - a[0]) * dx + (y - a[1]) * dy) / len;
  t = Math.max(0, Math.min(1, t));
  const px = a[0] + t * dx;
  const py = a[1] + t * dy;
  return Math.hypot(x - px, y - py);
}

function unique(prefix: string, used: string[]) {
  let n = 1;
  while (used.includes(`${prefix}${n}`)) n += 1;
  return `${prefix}${n}`;
}

function snap(value: number) {
  return Math.round(value);
}

function clamp(value: number, min: number, max: number) {
  return Math.max(min, Math.min(max, value));
}

function escapeHtml(text: string) {
  return text.replace(/[&<>"]/g, (ch) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[ch]!);
}

async function save(forcePak: boolean) {
  if (!studio) return;
  setStatus("Checking the game…");
  try {
    if (forcePak || studio.is_pak()) {
      const bytes = studio.export_pak();
      download(bytes, `${studio.game_id()}.advpak`);
      setStatus(studio.dirty() ? "Downloaded a rebuilt package." : "Downloaded the original package bytes.");
      return;
    }
    if (dirHandle) {
      const exported = studio.export_changes();
      const paths = JSON.parse(exported.paths()) as string[];
      const deleted = JSON.parse(exported.deleted()) as string[];
      for (const path of paths) await writeProjectFile(dirHandle, path, exported.bytes(path));
      for (const path of deleted) await removeProjectFile(dirHandle, path);
      setStatus(paths.length || deleted.length ? `Wrote ${paths.length} files.` : "No changes to write.");
      return;
    }
    download(studio.export_zip(), `${studio.game_id()}-project.zip`);
    setStatus("Downloaded the project zip.");
  } catch (error) {
    setStatus(String(error), true);
  }
}

async function writeProjectFile(root: DirectoryHandle, path: string, bytes: Uint8Array) {
  const parts = path.split("/");
  let dir = root;
  for (const part of parts.slice(0, -1)) dir = await dir.getDirectoryHandle(part, { create: true });
  const handle = await dir.getFileHandle(parts[parts.length - 1], { create: true });
  const writable = await handle.createWritable();
  const copy = new Uint8Array(bytes);
  await writable.write(copy);
  await writable.close();
}

async function removeProjectFile(root: DirectoryHandle, path: string) {
  const parts = path.split("/");
  let dir = root;
  for (const part of parts.slice(0, -1)) dir = await dir.getDirectoryHandle(part);
  await dir.removeEntry(parts[parts.length - 1]);
}

function download(bytes: Uint8Array, name: string) {
  const copy = new Uint8Array(bytes);
  const blob = new Blob([copy]);
  const link = document.createElement("a");
  link.href = URL.createObjectURL(blob);
  link.download = name;
  link.click();
  URL.revokeObjectURL(link.href);
}
