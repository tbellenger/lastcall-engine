import init, { Engine, type InitOutput } from "./wasm/lastcall_wasm.js";
import { CharlieAudio } from "./audio";

const WIDTH = 384;
const HEIGHT = 216;
const STORE = "saves";

type HostEvent =
  | { type: "sfx"; id: string }
  | { type: "music"; id: string }
  | { type: "say"; text: string }
  | { type: "autosave" }
  | { type: "ui"; action: string; slot: string }
  | { type: "room_changed"; room: string }
  | { type: "warning"; message: string };

export type MountOptions = {
  /** Package stem. `charlie` loads `charlie.advpak`. */
  game?: string;
  /** Directory that holds the package, the title card, and this bundle. */
  basePath?: string;
  /** IndexedDB database name. Charlie defaults to `lastcall-charlie-next`. */
  storageKey?: string;
  /** Capture keyboard and pointer for the whole page. Use this for a full-page mount. */
  page?: boolean;
};

export type PlayerHandle = {
  dispose: () => void;
};

export function mountPlayer(el: HTMLElement, options: MountOptions = {}): Promise<PlayerHandle> {
  const game = options.game ?? "charlie";
  const storageKey = options.storageKey ?? defaultStorageKey(game);
  const base = resolveBase(options.basePath);
  const page = options.page === true;

  el.classList.add("lastcall-player");
  el.dataset.storageKey = storageKey;
  el.replaceChildren();
  el.insertAdjacentHTML("afterbegin", markup());

  const screen = el.querySelector<HTMLCanvasElement>("#screen");
  const narration = el.querySelector<HTMLElement>("#live");
  const command = el.querySelector<HTMLInputElement>("#line");
  const status = el.querySelector<HTMLElement>("#note");
  const titleCard = el.querySelector<HTMLImageElement>("#title");
  if (!screen || !narration || !command || !status || !titleCard) {
    throw new Error("player markup is missing");
  }
  const context = screen.getContext("2d");
  if (!context) {
    throw new Error("canvas is unavailable");
  }
  const canvasContext = context;
  const canvas = screen;
  const title = titleCard;
  const commandLine = command;

  const charlieAudio = new CharlieAudio();
  const cleanups: Array<() => void> = [];
  let frameId = 0;
  let disposed = false;

  const handle: PlayerHandle = {
    dispose: () => {
      disposed = true;
      cancelAnimationFrame(frameId);
      for (const cleanup of cleanups) {
        cleanup();
      }
    },
  };

  const fail = (err: unknown): PlayerHandle => {
    status.textContent = err instanceof Error ? err.message : "The player failed to start.";
    return handle;
  };

  function unlock(): void {
    try {
      charlieAudio.unlock();
    } catch {
      /* the browser may still be blocking audio */
    }
  }

  function focusLine(): void {
    commandLine.focus({ preventScroll: true });
  }

  function dismissTitle(): void {
    title.hidden = true;
    focusLine();
  }

  function hostOwnsKeys(): boolean {
    const active = document.activeElement;
    if (!(active instanceof HTMLElement)) {
      return false;
    }
    if (active === command || active === document.body) {
      return false;
    }
    const field = active.closest("input, textarea, select, button, a, [contenteditable='true']");
    return field !== null && !el.contains(field);
  }

  function isPrintable(ev: KeyboardEvent): boolean {
    return ev.key.length === 1 && !ev.ctrlKey && !ev.metaKey && !ev.altKey;
  }

  function fit(): void {
    const sx = Math.floor((window.innerWidth - 24) / WIDTH);
    const sy = Math.floor((window.innerHeight - 96) / HEIGHT);
    const scale = Math.max(1, Math.min(sx, sy));
    canvas.style.setProperty("--scale", String(scale));
  }

  function openDb(): Promise<IDBDatabase> {
    return new Promise((resolve, reject) => {
      const request = indexedDB.open(storageKey, 1);
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
    canvasContext.putImageData(new ImageData(copy, WIDTH, HEIGHT), 0, 0);
    canvas.dataset.frameHash = engine.frame_hash();
    canvas.dataset.ready = "1";
  }

  function asset(name: string): string {
    return new URL(name, base).href;
  }

  return (async () => {
    try {
      const wasm = await init();
      if (disposed) {
        return handle;
      }
      const response = await fetch(asset(`${game}.advpak`));
      if (!response.ok) {
        throw new Error(`${game}.advpak is missing`);
      }
      if (game === "charlie") {
        titleCard.src = asset("charlie-title.png");
        titleCard.hidden = false;
      }
      const engine = Engine.load(new Uint8Array(await response.arrayBuffer()));
      if (disposed) {
        return handle;
      }
      fit();
      const onResize = (): void => fit();
      window.addEventListener("resize", onResize);
      cleanups.push(() => window.removeEventListener("resize", onResize));

      const send = (event: unknown): void => {
        engine.input(JSON.stringify(event));
      };

      const pump = async (): Promise<void> => {
        const events = JSON.parse(engine.drain_events()) as HostEvent[];
        for (const event of events) {
          if (event.type === "say") {
            narration.textContent = event.text;
          } else if (event.type === "sfx") {
            charlieAudio.sfx(event.id);
          } else if (event.type === "room_changed") {
            charlieAudio.setRoom(event.room);
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

      const onPointerDown = (ev: Event): void => {
        unlock();
        const target = ev.target;
        if (!(target instanceof Element)) {
          return;
        }
        if (!page && !el.contains(target)) {
          return;
        }
        if (target.closest("button")) {
          return;
        }
        const foreign = target.closest("input, textarea, select, [contenteditable='true']");
        if (foreign && foreign !== command) {
          return;
        }
        if (!title.hidden) {
          ev.preventDefault();
          dismissTitle();
          queueMicrotask(() => focusLine());
          return;
        }
        if (target === canvas) {
          ev.preventDefault();
          const pointer = ev as PointerEvent;
          const rect = canvas.getBoundingClientRect();
          const x = Math.floor(((pointer.clientX - rect.left) * WIDTH) / rect.width);
          const y = Math.floor(((pointer.clientY - rect.top) * HEIGHT) / rect.height);
          send({ type: "tap", x, y });
          void pump();
        } else if (target !== command) {
          ev.preventDefault();
        }
        queueMicrotask(() => focusLine());
      };

      const onPointerUp = (ev: Event): void => {
        const target = ev.target;
        if (!(target instanceof Element)) {
          return;
        }
        if (!page && !el.contains(target)) {
          return;
        }
        if (target.closest("button")) {
          return;
        }
        const foreign = target.closest("input, textarea, select, [contenteditable='true']");
        if (foreign && foreign !== command) {
          return;
        }
        if (title.hidden) {
          focusLine();
        }
      };

      const onKey = (ev: Event): void => {
        const keyEvent = ev as KeyboardEvent;
        if (hostOwnsKeys()) {
          return;
        }
        unlock();
        if (!title.hidden && (isPrintable(keyEvent) || keyEvent.key === "Enter" || keyEvent.key === "Backspace")) {
          dismissTitle();
        }
        if (keyEvent.repeat && keyEvent.key !== "Backspace") {
          return;
        }
        const arrows: Record<string, string> = {
          ArrowUp: "n",
          ArrowDown: "s",
          ArrowLeft: "w",
          ArrowRight: "e",
        };
        const dir = arrows[keyEvent.key];
        if (dir) {
          keyEvent.preventDefault();
          send({ type: "key_down", key: dir });
          void pump();
          return;
        }
        if (keyEvent.key === "Enter") {
          keyEvent.preventDefault();
          focusLine();
          send({ type: "set_line", text: command.value });
          send({ type: "submit" });
          command.value = "";
          send({ type: "set_line", text: "" });
          void pump();
          return;
        }
        if (isPrintable(keyEvent) && document.activeElement !== command) {
          keyEvent.preventDefault();
          focusLine();
          command.value += keyEvent.key;
          send({ type: "set_line", text: command.value });
          void pump();
          return;
        }
        if (keyEvent.key === "Backspace" && document.activeElement !== command) {
          keyEvent.preventDefault();
          focusLine();
          command.value = command.value.slice(0, -1);
          send({ type: "set_line", text: command.value });
          void pump();
          return;
        }
        if (keyEvent.key === "F5") {
          keyEvent.preventDefault();
          void idbPut("1", engine.save()).then(() => {
            status.textContent = "Saved slot 1.";
          });
        }
        if (keyEvent.key === "F7") {
          keyEvent.preventDefault();
          void idbGet("1").then((bytes) => {
            if (bytes) {
              engine.restore(bytes);
              status.textContent = "Restored slot 1.";
            }
          });
        }
      };

      const pointerTarget: EventTarget = page ? document : el;
      const keyTarget: EventTarget = page ? window : el;
      pointerTarget.addEventListener("pointerdown", onPointerDown);
      pointerTarget.addEventListener("pointerup", onPointerUp);
      keyTarget.addEventListener("keydown", onKey);
      cleanups.push(() => {
        pointerTarget.removeEventListener("pointerdown", onPointerDown);
        pointerTarget.removeEventListener("pointerup", onPointerUp);
        keyTarget.removeEventListener("keydown", onKey);
      });

      const onInput = (): void => {
        send({ type: "set_line", text: command.value });
      };
      command.addEventListener("input", onInput);
      cleanups.push(() => command.removeEventListener("input", onInput));

      const saveSlot = (slot: string): void => {
        void idbPut(slot, engine.save()).then(() => {
          status.textContent = `Saved slot ${slot}.`;
        });
      };
      const loadSlot = (slot: string): void => {
        void idbGet(slot).then((bytes) => {
          if (!bytes) {
            status.textContent = `Slot ${slot} is empty.`;
            return;
          }
          engine.restore(bytes);
          status.textContent = `Restored slot ${slot}.`;
          paint(engine, wasm);
        });
      };
      el.querySelectorAll<HTMLButtonElement>("[data-save]").forEach((button) => {
        const click = (): void => saveSlot(button.dataset.save ?? "1");
        button.addEventListener("click", click);
        cleanups.push(() => button.removeEventListener("click", click));
      });
      el.querySelectorAll<HTMLButtonElement>("[data-load]").forEach((button) => {
        const click = (): void => loadSlot(button.dataset.load ?? "1");
        button.addEventListener("click", click);
        cleanups.push(() => button.removeEventListener("click", click));
      });
      const autosave = el.querySelector<HTMLButtonElement>("#autosave");
      if (autosave) {
        const labeled = (): void => {
          void idbGet("autosave").then((bytes) => {
            if (!bytes) {
              status.textContent = "No autosave yet.";
              return;
            }
            engine.restore(bytes);
            status.textContent = "Restored autosave.";
            paint(engine, wasm);
          });
        };
        autosave.addEventListener("click", labeled);
        cleanups.push(() => autosave.removeEventListener("click", labeled));
      }

      await pump();
      paint(engine, wasm);
      if (title.hidden) {
        focusLine();
      }

      let acc = 0;
      let last = performance.now();
      const frame = (now: number): void => {
        if (disposed) {
          return;
        }
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
          void pump();
        }
        paint(engine, wasm);
        frameId = requestAnimationFrame(frame);
      };
      frameId = requestAnimationFrame(frame);
      return handle;
    } catch (err) {
      return fail(err);
    }
  })();
}

function defaultStorageKey(game: string): string {
  if (game === "sample") {
    return "lastcall";
  }
  if (game === "charlie") {
    return "lastcall-charlie-next";
  }
  return `lastcall-${game}-next`;
}

function resolveBase(basePath: string | undefined): string {
  if (!basePath) {
    // Keep import.meta.url indirect so the bundler does not treat "." as an asset.
    const moduleUrl = import.meta.url;
    return new URL(".", moduleUrl).href;
  }
  const withSlash = basePath.endsWith("/") ? basePath : `${basePath}/`;
  return new URL(withSlash, window.location.href).href;
}

function markup(): string {
  const slots = [1, 2, 3, 4, 5]
    .map(
      (slot) =>
        `<button type="button" data-save="${slot}">Save ${slot}</button><button type="button" data-load="${slot}">Load ${slot}</button>`,
    )
    .join("");
  return `<div class="stage"><canvas id="screen" width="384" height="216" role="img" aria-label="Game picture"></canvas><img id="title" alt="Charlie: Last Call" hidden /></div><input id="line" autocomplete="off" autocapitalize="off" spellcheck="false" aria-label="Command line" /><p id="live" aria-live="polite"></p><div class="bar">${slots}<button type="button" id="autosave">Load autosave</button><span id="note"></span></div>`;
}
