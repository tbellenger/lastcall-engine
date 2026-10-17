import init, { Engine, type InitOutput } from "./wasm/lastcall_wasm.js";

const WIDTH = 384;
const HEIGHT = 216;
const DB_NAME = "lastcall";
const STORE = "saves";

type HostEvent =
  | { type: "sfx"; id: string }
  | { type: "music"; id: string }
  | { type: "say"; text: string }
  | { type: "autosave" }
  | { type: "ui"; action: string; slot: string }
  | { type: "room_changed"; room: string }
  | { type: "warning"; message: string };

const canvas = document.querySelector<HTMLCanvasElement>("#screen");
const live = document.querySelector<HTMLElement>("#live");
const line = document.querySelector<HTMLInputElement>("#line");
const note = document.querySelector<HTMLElement>("#note");
if (!canvas || !live || !line || !note) {
  throw new Error("player markup is missing");
}
const screen = canvas;
const narration = live;
const command = line;
const status = note;
const ctx = screen.getContext("2d");
if (!ctx) {
  throw new Error("canvas is unavailable");
}
const context = ctx;

let audio: AudioContext | null = null;

function unlock(): void {
  try {
    if (!audio) {
      audio = new AudioContext();
    }
    if (audio.state === "suspended") {
      void audio.resume();
    }
  } catch {
    audio = null;
  }
}

function beep(): void {
  if (!audio) {
    return;
  }
  const osc = audio.createOscillator();
  const gain = audio.createGain();
  osc.frequency.value = 660;
  gain.gain.value = 0.05;
  osc.connect(gain);
  gain.connect(audio.destination);
  osc.start();
  osc.stop(audio.currentTime + 0.06);
}

function fit(): void {
  const sx = Math.floor((window.innerWidth - 24) / WIDTH);
  const sy = Math.floor((window.innerHeight - 96) / HEIGHT);
  const scale = Math.max(1, Math.min(sx, sy));
  screen.style.setProperty("--scale", String(scale));
}

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, 1);
    request.onupgradeneeded = () => {
      request.result.createObjectStore(STORE);
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

async function idbPut(slot: string, bytes: Uint8Array): Promise<void> {
  const db = await openDb();
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction(STORE, "readwrite");
    tx.objectStore(STORE).put(bytes, slot);
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });
  db.close();
}

async function idbGet(slot: string): Promise<Uint8Array | null> {
  const db = await openDb();
  const value = await new Promise<unknown>((resolve, reject) => {
    const tx = db.transaction(STORE, "readonly");
    const request = tx.objectStore(STORE).get(slot);
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
  db.close();
  return value instanceof Uint8Array ? value : null;
}

function pixels(engine: Engine, wasm: InitOutput): Uint8ClampedArray {
  const ptr = engine.framebuffer();
  const len = engine.framebuffer_len();
  return new Uint8ClampedArray(wasm.memory.buffer, ptr, len);
}

function paint(engine: Engine, wasm: InitOutput): void {
  const view = pixels(engine, wasm);
  const copy = new Uint8ClampedArray(view);
  context.putImageData(new ImageData(copy, WIDTH, HEIGHT), 0, 0);
  screen.dataset.frameHash = engine.frame_hash();
  screen.dataset.ready = "1";
}

async function boot(): Promise<void> {
  const wasm = await init();
  const response = await fetch("sample.advpak");
  if (!response.ok) {
    throw new Error("sample.advpak is missing");
  }
  return start(wasm, new Uint8Array(await response.arrayBuffer()));
}

async function start(wasm: InitOutput, pak: Uint8Array): Promise<void> {
  const engine = Engine.load(pak);
  fit();
  window.addEventListener("resize", fit);

  const send = (event: unknown): void => {
    engine.input(JSON.stringify(event));
  };

  const handle = async (): Promise<void> => {
    const events = JSON.parse(engine.drain_events()) as HostEvent[];
    for (const event of events) {
      if (event.type === "say") {
        narration.textContent = event.text;
      } else if (event.type === "sfx") {
        console.log("sfx", event.id);
        beep();
      } else if (event.type === "warning") {
        status.textContent = event.message;
      } else if (event.type === "autosave") {
        await idbPut("autosave", engine.save());
      } else if (event.type === "ui" && event.action === "save") {
        await idbPut(event.slot, engine.save());
        status.textContent = `Saved slot ${event.slot}.`;
      } else if (event.type === "ui" && event.action === "restore") {
        const bytes = await idbGet(event.slot);
        if (bytes) {
          engine.restore(bytes);
          status.textContent = `Restored slot ${event.slot}.`;
        } else {
          status.textContent = `Slot ${event.slot} is empty.`;
        }
      }
    }
  };

  document.addEventListener("pointerdown", (ev) => {
    unlock();
    const target = ev.target;
    if (!(target instanceof Element)) {
      return;
    }
    if (target.closest("button")) {
      return;
    }
    if (target === screen) {
      const rect = screen.getBoundingClientRect();
      const x = Math.floor(((ev.clientX - rect.left) * WIDTH) / rect.width);
      const y = Math.floor(((ev.clientY - rect.top) * HEIGHT) / rect.height);
      send({ type: "tap", x, y });
      void handle();
      return;
    }
    command.focus();
  });

  window.addEventListener("keydown", (ev) => {
    unlock();
    if (ev.repeat) {
      return;
    }
    const arrows: Record<string, string> = {
      ArrowUp: "n",
      ArrowDown: "s",
      ArrowLeft: "w",
      ArrowRight: "e",
    };
    const dir = arrows[ev.key];
    if (dir) {
      ev.preventDefault();
      send({ type: "key_down", key: dir });
      void handle();
      return;
    }
    if (ev.key === "Enter") {
      ev.preventDefault();
      send({ type: "set_line", text: command.value });
      send({ type: "submit" });
      command.value = "";
      void handle();
      return;
    }
    if (ev.key === "F5") {
      ev.preventDefault();
      void idbPut("1", engine.save()).then(() => {
        status.textContent = "Saved slot 1.";
      });
    }
    if (ev.key === "F7") {
      ev.preventDefault();
      void idbGet("1").then((bytes) => {
        if (bytes) {
          engine.restore(bytes);
          status.textContent = "Restored slot 1.";
        }
      });
    }
  });

  command.addEventListener("input", () => {
    send({ type: "set_line", text: command.value });
  });

  document.querySelector("#save")?.addEventListener("click", () => {
    void idbPut("1", engine.save()).then(() => {
      status.textContent = "Saved slot 1.";
    });
  });
  document.querySelector("#load")?.addEventListener("click", () => {
    void idbGet("1").then((bytes) => {
      if (!bytes) {
        status.textContent = "Slot 1 is empty.";
        return;
      }
      engine.restore(bytes);
      status.textContent = "Restored slot 1.";
      paint(engine, wasm);
    });
  });
  document.querySelector("#autosave")?.addEventListener("click", () => {
    void idbGet("autosave").then((bytes) => {
      if (!bytes) {
        status.textContent = "No autosave yet.";
        return;
      }
      engine.restore(bytes);
      status.textContent = "Restored autosave.";
      paint(engine, wasm);
    });
  });

  await handle();
  paint(engine, wasm);

  let acc = 0;
  let last = performance.now();
  const frame = (now: number): void => {
    let dt = now - last;
    last = now;
    if (dt > 250) {
      dt = 250;
    }
    acc += dt;
    let steps = 0;
    while (acc >= 1000 / 60 && steps < 5) {
      engine.step(1);
      acc -= 1000 / 60;
      steps += 1;
    }
    if (steps > 0) {
      void handle();
    }
    paint(engine, wasm);
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
}

boot().catch((err: unknown) => {
  status.textContent = err instanceof Error ? err.message : "The player failed to start.";
});
