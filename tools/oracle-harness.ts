import fs from "fs";
import path from "path";
import {
  applyCommand as dispatch,
  cloneState,
  completeCutscene,
  initialState,
  worldFrom,
} from "@/lib/charlie/logic";
import { Runtime } from "@/lib/charlie/runtime";
import type { CommandResult, GameState, World } from "@/lib/charlie/types";

type Step = {
  cmd?: string;
  interrupt?: string;
  hold?: boolean;
  lines: string[];
  score: number;
  room: string;
  flags: string[];
  items: Record<string, number>;
  vars: Record<string, number>;
  dead?: boolean;
  ending?: string | null;
  shot?: string;
};

const outDir = process.argv[2];
if (!outDir) {
  console.error("usage: node harness.cjs <parity-dir>");
  process.exit(1);
}

function loadWorld(): World {
  const dir = path.join("/tmp/dw/public/games/charlie/rooms");
  const read = (name: string) => JSON.parse(fs.readFileSync(path.join(dir, name), "utf8"));
  return worldFrom(
    read("bar.json"),
    read("bathroom.json"),
    read("alley.json"),
    read("global.json"),
    read("club.json"),
    read("rooftop.json"),
    read("street.json"),
    read("billiard.json"),
    read("deck.json"),
    read("hall.json"),
    read("kitchen.json"),
    read("hallway.json"),
    read("office.json"),
  );
}

function counts(items: string[]) {
  const out: Record<string, number> = {};
  for (const item of items) out[item] = (out[item] ?? 0) + 1;
  return out;
}

function snap(state: GameState, dead = false): Omit<Step, "cmd" | "lines"> {
  const flags = Object.entries(state.flags)
    .filter(([, on]) => on === true)
    .map(([name]) => name)
    .filter((name) => !name.startsWith("did:") && name !== "live" && name !== "vicEntered")
    .sort();
  return {
    score: state.score,
    room: state.room,
    flags,
    items: counts(state.inventory),
    vars: {
      clockClub: state.clocks?.club ?? 0,
      clockAlley: state.clocks?.alley ?? 0,
      thug: state.thugTurn ?? 0,
      police: state.policeMoves ?? 0,
    },
    dead,
    ending: state.flags.goodEnd ? "good" : state.flags.badEnd ? "bad" : null,
  };
}

const SHOTS: Record<string, string> = {
  "order beer#1": "bar",
  "drink beer#1": "bar-beer",
  "talk to earl#1": "earl",
  "play b7#1": "jukebox",
  "tip sal#1": "tip",
  "go bathroom#1": "bathroom",
  "look mirror#1": "mirror",
  "get soap#1": "soap",
  "wear cologne#1": "cologne",
  "go alley#1": "alley",
  "get matchbook#1": "matchbook",
  "give five to professor#1": "professor",
  "show matchbook to bouncer#1": "bouncer",
  "tip dancer#1": "dancer",
  "show matchbook to guard#1": "club",
  "climb ladder#1": "ladder",
  "pick lock#1": "street",
  "look#1": "billiard",
  "use matchbook#1": "rope",
  "throw ball#1": "untied",
  "dive#1": "deck",
  "get chlorine#1": "pool",
  "take clothes#1": "dry",
  "make bomb#1": "bomb",
  "throw bomb#1": "hallway",
  "go office#1": "office",
  "untie dana#1": "dana",
  "give note to police#1": "ending",
};

function shotFor(cmd: string, seen: Record<string, number>) {
  seen[cmd] = (seen[cmd] ?? 0) + 1;
  return SHOTS[`${cmd}#${seen[cmd]}`];
}

function playTracked(world: World, lines: string[], shots = false) {
  let state = initialState(world);
  let autosave = cloneState(state);
  let early: GameState | null = null;
  let checkpoint: GameState | null = null;
  const steps: Step[] = [];
  const seen: Record<string, number> = {};
  for (const line of lines) {
    let result = dispatch(world, state, line);
    if (result.closer) {
      state = { ...state, pos: [result.closer[0], result.closer[1]], flags: { ...state.flags }, inventory: state.inventory.slice(), scoredIds: state.scoredIds.slice(), clocks: { ...state.clocks } };
      result = dispatch(world, state, line);
    }
    if (result.roomChanged) autosave = cloneState(result.state);
    if (result.checkpoint) {
      const saved = cloneState(result.state);
      if (!early) early = saved;
      checkpoint = saved;
    }
    let said = result.lines.slice();
    state = result.state;
    if (result.cutscene) {
      const cut = completeCutscene(state);
      state = cut.state;
      said = said.concat(cut.lines);
    }
    const step: Step = { cmd: line, lines: said, ...snap(state, Boolean(result.death)) };
    if (shots) {
      const name = shotFor(line, seen);
      if (name) step.shot = name;
    }
    steps.push(step);
    if (result.death) {
      const back = result.timerDeath && result.restoreEarly && early
        ? early
        : result.timerDeath && checkpoint
          ? checkpoint
          : autosave;
      steps.push({ cmd: "restore", lines: [], ...snap(cloneState(back), false) });
      break;
    }
  }
  return { state, steps, autosave, early, checkpoint };
}

function dismissAll(rt: Runtime) {
  let guard = 0;
  while (rt.line && guard++ < 40) rt.dismiss();
}

function sayRt(rt: Runtime, text: string) {
  const before = rt.transcript.length;
  rt.command(text);
  rt.completeApproach();
  dismissAll(rt);
  rt.changingUntil = 0;
  return rt.transcript.slice(before);
}

const PREFACE = [
  "order beer",
  "drink beer",
  "talk to earl",
  "buy drink for earl",
  "play b7",
  "tip sal",
  "go bathroom",
  "look mirror",
  "get soap",
  "put quarters in machine",
  "wear cologne",
  "go bar",
  "go alley",
  "get matchbook",
  "search car",
  "get umbrella",
  "give five to professor",
  "show matchbook to bouncer",
  "talk to bouncer",
  "tip dancer",
  "get pick",
  "show matchbook to guard",
  "drop soap",
  "go alley",
  "climb dumpster",
  "pull ladder with umbrella",
  "climb ladder",
  "pick lock",
];

const LAKE = [
  "look",
  "use matchbook",
  "get ball",
  "throw ball",
  "get knife",
  "get chips",
  "get handkerchief",
  "go outside",
  "dive",
  "hold breath",
  "get chlorine",
  "go inside",
  "take clothes",
  "go hall",
  "go kitchen",
  "open pantry",
  "make bomb",
  "go hall",
  "go hallway",
  "throw bomb",
  "go office",
  "untie dana",
  "call police",
  "search desk",
  "read books",
  "give books to police",
  "give note to police",
];

function untilWindow(rt: Runtime, steps: Step[], seen: Record<string, number>) {
  let guard = 0;
  while ((((rt.state.thugTurn ?? 0) % 3) + 3) % 3 !== 2 && guard++ < 6) {
    const lines = sayRt(rt, "wait");
    const step: Step = { cmd: "wait", lines, ...snap(rt.state) };
    const name = shotFor("wait", seen);
    if (name) step.shot = name;
    steps.push(step);
  }
}

function lakeFrom(world: World, state: GameState, commands: string[], shots = false, window = true) {
  const rt = new Runtime(world, state, true);
  const steps: Step[] = [];
  const seen: Record<string, number> = {};
  const before = rt.transcript.length;
  rt.command("talk doorman");
  let guard = 0;
  while (rt.cutStep !== "lake" && rt.scene !== "ending" && guard++ < 40) {
    if (rt.cutStep === "knockout") {
      rt.present(rt.knockoutUntil);
      continue;
    }
    rt.dismiss();
  }
  dismissAll(rt);
  steps.push({ cmd: "talk doorman", lines: rt.transcript.slice(before), ...snap(rt.state) });
  const arrival = cloneState(rt.state);
  const rest = commands[0] === "look" ? commands : ["look", ...commands];
  for (const line of rest) {
    const needWindow = line === "use matchbook" || line === "get matchbook" || (line === "get ball" && rt.state.flags.thugDown !== true);
    if (window && needWindow) untilWindow(rt, steps, seen);
    const lines = sayRt(rt, line);
    const step: Step = { cmd: line, lines, ...snap(rt.state, rt.scene === "death") };
    if (shots) {
      const name = shotFor(line, seen);
      if (name) step.shot = name;
    }
    steps.push(step);
    if (rt.scene === "death") {
      steps.push({ cmd: "restore", lines: [], ...snap(arrival, false) });
      break;
    }
    if (rt.scene === "ending") break;
  }
  return { rt, steps };
}

function write(name: string, steps: Step[]) {
  fs.mkdirSync(outDir, { recursive: true });
  fs.writeFileSync(path.join(outDir, name), JSON.stringify({ steps }, null, 2) + "\n");
  console.log(`${name} ${steps.length} steps`);
}

const world = loadWorld();
const goldenPlay = playTracked(world, PREFACE, true);
const goldenLake = lakeFrom(world, goldenPlay.state, LAKE, true);
write("golden.json", goldenPlay.steps.concat(goldenLake.steps));

write("death-tank.json", playTracked(world, ["go alley", "attack bouncer"]).steps);
write("death-ladder.json", playTracked(world, ["go alley", "jump ladder"]).steps);

{
  const rt = new Runtime(world, initialState(world), true);
  const steps: Step[] = [];
  rt.command("go alley");
  rt.completeApproach();
  steps.push({ cmd: "go alley", hold: true, lines: rt.line ? [rt.line] : [], ...snap(rt.state) });
  const before = rt.transcript.length;
  rt.command("shout");
  dismissAll(rt);
  const lines = rt.transcript.slice(before);
  steps.push({ interrupt: "shout", lines, ...snap(rt.state, true) });
  steps.push({ cmd: "restore", lines: [], ...snap(rt.state, false) });
  write("death-cutscene.json", steps);
}

const clubPath = [
  "order beer", "drink beer", "talk to earl", "buy drink for earl", "play b7", "tip sal",
  "go bathroom", "get soap", "put quarters in machine", "wear cologne", "go bar", "go alley",
  "get matchbook", "talk to bouncer", "show matchbook to guard", "look", "look", "look", "look", "look",
];
write("death-club.json", playTracked(world, clubPath).steps);

const alleyPath = [
  "order beer", "drink beer", "talk to earl", "buy drink for earl", "play b7", "tip sal",
  "go bathroom", "get soap", "put quarters in machine", "wear cologne", "go bar", "go alley",
  "get umbrella", "get matchbook", "talk to bouncer", "show matchbook to guard", "drop soap", "go alley",
  "look", "look", "look", "look", "look",
];
write("death-alley.json", playTracked(world, alleyPath).steps);

const dryPath = [
  "order beer", "drink beer", "talk to earl", "buy drink for earl", "play b7", "tip sal",
  "go bathroom", "get soap", "put quarters in machine", "wear cologne", "go bar", "go alley",
  "get matchbook", "talk to bouncer", "show matchbook to guard", "go out",
];
write("death-leave.json", playTracked(world, dryPath).steps);

const dancerPath = [
  "order beer", "drink beer", "talk to earl", "buy drink for earl", "play b7", "tip sal",
  "go bathroom", "get soap", "put quarters in machine", "wear cologne", "go bar", "go alley",
  "get matchbook", "talk to bouncer", "touch dancer",
];
write("death-dancer.json", playTracked(world, dancerPath).steps);

const roofPath = [
  "order beer", "drink beer", "talk to earl", "buy drink for earl", "play b7", "tip sal",
  "go bathroom", "get soap", "put quarters in machine", "wear cologne", "go bar", "go alley",
  "get umbrella", "get matchbook", "talk to bouncer", "show matchbook to guard", "drop soap", "go alley",
  "climb dumpster", "pull ladder with umbrella", "climb ladder", "open door",
];
write("death-roof.json", playTracked(world, roofPath).steps);

function lakeDeath(name: string, extra: string[], window = true) {
  const played = playTracked(world, PREFACE);
  const lake = lakeFrom(world, played.state, extra, false, window);
  write(name, played.steps.concat(lake.steps));
}

lakeDeath("death-rope.json", ["look", "use matchbook"], false);
lakeDeath("death-beam.json", ["look", "use matchbook", "get ball", "throw ball", "go outside", "wait"]);
lakeDeath("death-surface.json", ["look", "use matchbook", "get ball", "throw ball", "go outside", "dive", "surface"]);
lakeDeath("death-hall.json", ["look", "use matchbook", "get ball", "throw ball", "go hall"]);
lakeDeath("death-wet.json", ["look", "use matchbook", "get ball", "throw ball", "go outside", "dive", "hold breath", "go inside", "go hall"]);
lakeDeath("death-office.json", [
  "look", "use matchbook", "get ball", "throw ball", "go outside", "dive", "hold breath", "get chlorine",
  "go inside", "take clothes", "go hall", "go hallway", "go office",
]);
lakeDeath("ending-bad-no.json", LAKE.slice(0, -1).concat(["no"]));
lakeDeath("ending-bad-wait.json", LAKE.slice(0, -1).concat(["wait", "wait", "wait", "wait", "wait"]));

void (0 as unknown as CommandResult);
