// Turn the Charlie: Last Call room files into a lastcall-engine authoring folder.
// Usage: node tools/import-charlie.mjs <dw-root> <out-dir>
import fs from "fs";
import path from "path";
import zlib from "zlib";

const dw = process.argv[2];
const out = process.argv[3];
if (!dw || !out) {
  console.error("usage: node tools/import-charlie.mjs <dw-root> <out-dir>");
  process.exit(1);
}

const roomsDir = path.join(dw, "public/games/charlie/rooms");
const parserSrc = fs.readFileSync(path.join(dw, "lib/charlie/parser.ts"), "utf8");
const logicSrc = fs.readFileSync(path.join(dw, "lib/charlie/logic.ts"), "utf8");

const ROOMS = [
  "bar",
  "bathroom",
  "alley",
  "club",
  "rooftop",
  "street",
  "billiard",
  "deck",
  "hall",
  "kitchen",
  "hallway",
  "office",
];
const LAKE = new Set(["billiard", "deck", "hall", "kitchen", "hallway", "office"]);
const FREE = new Set([
  "look",
  "go",
  "wait",
  "inventory",
  "score",
  "help",
  "save",
  "restore",
  "quit",
  "restart",
  "drink",
  "wear",
  "drop",
  "shout",
  "insult",
  "burn",
  "no",
  "hold",
  "surface",
  "dive",
  "stand",
]);
const META = new Set(["save", "restore", "quit", "restart"]);
const VERB_DEFAULT = {
  look: "You see nothing special about that.",
  get: "You can't take that.",
  use: "That doesn't seem to do anything.",
  give: "That doesn't change any hands.",
  talk: "You get no answer worth repeating.",
  buy: "Nobody is selling that.",
  play: "That doesn't play.",
  put: "That doesn't fit.",
  wear: "You can't wear that.",
  open: "It doesn't open.",
  close: "It doesn't close.",
  search: "You find nothing you want.",
  pet: "You keep your hands to yourself.",
  drink: "You can't drink that.",
  wash: "You don't wash that.",
  drop: "You hang onto it.",
  show: "You have nothing to show, or nobody who cares.",
  attack: "Violence is a hobby for people with better insurance.",
  shout: "You shout. The city does not take your call.",
  jump: "You hop in place. The suit squelches.",
  insult: "You think better of it.",
  go: "You can't go that way.",
};

const CODE_SCORES = [
  ["witness-cutscene", 4],
  ["burn-rope", 5],
  ["get-ball", 3],
  ["bean-thug", 4],
  ["thug-knife", 2],
  ["thug-chips", 2],
  ["thug-hanky", 2],
  ["thug-clothes", 3],
  ["pool-breath", 5],
  ["get-chlorine", 3],
  ["make-bomb", 4],
  ["smoke-guard", 5],
  ["untie-dana", 4],
  ["call-police", 3],
  ["get-books", 4],
  ["read-books", 2],
  ["give-books", 3],
  ["give-plate", 5],
];

const SCRIPT_FLAGS = [
  "tied",
  "floorBook",
  "handsFree",
  "vicLeft",
  "vicEntered",
  "thugDown",
  "dry",
  "inPool",
  "poolDone",
  "wet",
  "hallClear",
  "danaFree",
  "policeCalled",
  "vicFled",
  "policeHere",
  "goodEnd",
  "badEnd",
  "ended",
  "quietEnter",
  "live",
  "heardThunder",
  "seenCutscene",
  "matchbookHere",
  "alleyChase",
  "clubChase",
];

const ITEM_LABEL = {
  twenty: ["a twenty-dollar bill", "twenty-dollar bills"],
  ten: ["a ten-dollar bill", "ten-dollar bills"],
  five: ["a five-dollar bill", "five-dollar bills"],
  dollar: ["a one-dollar bill", "one-dollar bills"],
  quarters: ["a roll of quarters", "rolls of quarters"],
  beer: ["a cold beer", "cold beers"],
  cologne: ["a bottle of Midnight Stallion", "bottles of Midnight Stallion"],
  iron: ["a tyre iron", "tyre irons"],
  umbrella: ["a hooked black umbrella", "hooked black umbrellas"],
  matchbook: ["a Purple Rhino matchbook", "Purple Rhino matchbooks"],
  pick: ["a black hair pick", "black hair picks"],
  note: ["a folded licence-plate note", "folded notes"],
  soap: ["a slimy bar of soap", "slimy bars of soap"],
  ball: ["a billiard ball", "billiard balls"],
  knife: ["a short knife", "short knives"],
  chips: ["a few casino chips", "piles of casino chips"],
  handkerchief: ["a handkerchief", "handkerchiefs"],
  chlorine: ["a tin of chlorine tablets", "tins of chlorine tablets"],
  bomb: ["a smoke bomb", "smoke bombs"],
  books: ["a set of account books", "sets of account books"],
};

function parseAdds(src, which) {
  const re = new RegExp(`add\\(${which}, "([^"]+)", \\[([^\\]]*)\\]\\)`, "g");
  const map = {};
  const groups = new Map();
  let match;
  while ((match = re.exec(src))) {
    const id = match[1];
    const words = [...match[2].matchAll(/"([^"]+)"/g)].map((found) => found[1]);
    if (!groups.has(id)) groups.set(id, []);
    const list = groups.get(id);
    for (const word of words) {
      for (const other of groups.values()) {
        const index = other.indexOf(word);
        if (index >= 0) other.splice(index, 1);
      }
      map[word] = id;
      list.push(word);
    }
  }
  for (const [id, words] of [...groups]) {
    if (words.length === 0) groups.delete(id);
  }
  return { map, groups };
}

const verbs = parseAdds(parserSrc, "VERBS");
const nouns = parseAdds(parserSrc, "NOUNS");
const songMatch = parserSrc.match(/const SONGS = \[([^\]]+)\]/);
for (const song of [...songMatch[1].matchAll(/"([^"]+)"/g)].map((found) => found[1])) {
  if (!nouns.map[song]) {
    nouns.map[song] = song;
    nouns.groups.set(song, [song]);
  }
}

function canonVerb(word) {
  return verbs.map[word] || word;
}
function canonNoun(word) {
  if (!word) return "";
  return nouns.map[word] || word;
}

function asList(value) {
  if (value == null) return null;
  return Array.isArray(value) ? value : [value];
}

function ensureNoun(id) {
  if (!id) return;
  for (const words of nouns.groups.values()) {
    if (nouns.map[id] === undefined && words.includes(id)) return;
  }
  if ([...nouns.groups.values()].some((words) => words.includes(id)) && nouns.map[id] !== id) {
    const privateWord = `${id}Word`;
    if (!nouns.map[privateWord]) {
      nouns.map[privateWord] = id;
      const existing = [...nouns.groups.entries()].find(([, words]) => false);
      void existing;
      if (![...nouns.groups.keys()].includes(id)) nouns.groups.set(id, [privateWord]);
    }
    return;
  }
  if (!nouns.groups.has(id)) {
    if (nouns.map[id] && nouns.map[id] !== id) {
      const privateWord = `${id}Word`;
      nouns.map[privateWord] = id;
      nouns.groups.set(id, [privateWord]);
    } else {
      nouns.map[id] = id;
      nouns.groups.set(id, [id]);
    }
  }
}

function quoteConst(name, text) {
  return `const ${name} = ${JSON.stringify(text)};\n`;
}

const CLUB_TIMEOUT = logicSrc.match(/const CLUB_TIMEOUT =\n([\s\S]*?);/)[1].replace(/\s*\+\s*/g, "").replace(/^"|"$/gm, "").replace(/\n/g, "");
const ALLEY_TIMEOUT = logicSrc.match(/const ALLEY_TIMEOUT =\n([\s\S]*?);/)[1].replace(/\s*\+\s*/g, "").replace(/^"|"$/gm, "").replace(/\n/g, "");

function unquoteTs(block) {
  return block
    .split("\n")
    .map((line) => line.trim().replace(/^"/, "").replace(/"$/, "").replace(/"\s*\+\s*$/, "").replace(/^\+\s*"/, ""))
    .join("")
    .replace(/"\s*\+\s*"/g, "");
}

function grabConst(name) {
  const match = logicSrc.match(new RegExp(`const ${name} =\\n([\\s\\S]*?);`));
  if (!match) throw new Error(`missing ${name}`);
  return unquoteTs(match[1]);
}

const clubTimeout = grabConst("CLUB_TIMEOUT");
const alleyTimeout = grabConst("ALLEY_TIMEOUT");
const prompt = "Restore, restart, or quit?";

const usedItems = new Set(Object.keys(ITEM_LABEL));
const usedFlags = new Set(SCRIPT_FLAGS);
const scores = new Map(CODE_SCORES);
const ruleIds = new Set();

function effectsFor(action, roomId) {
  if (action.death) {
    const lines = action.texts?.length ? action.texts.slice() : [action.text || "That was a bad idea."];
    lines.push(prompt);
    let restore = "autosave";
    if (action.timerDeath && action.restoreEarly) restore = "club-chase";
    else if (action.timerDeath) restore = roomId === "alley" ? "alley-chase" : "club-chase";
    const effects = lines.map((text) => ({ do: "say", text }));
    effects.push({ do: "setDead", restoreTo: restore });
    return effects;
  }
  const effects = [];
  for (const item of asList(action.take) || []) {
    effects.push({ do: "give", id: item });
    usedItems.add(item);
  }
  for (const item of asList(action.give) || []) {
    effects.push({ do: "take", id: item });
    usedItems.add(item);
  }
  for (const flag of asList(action.set) || []) {
    effects.push({ do: "set", flag });
    usedFlags.add(flag);
  }
  for (const flag of asList(action.clear) || []) {
    effects.push({ do: "clear", flag });
    usedFlags.add(flag);
  }
  if (action.once) {
    const flag = `did_${String(action.id).replace(/[^A-Za-z0-9_]/g, "_")}`;
    effects.push({ do: "set", flag });
    usedFlags.add(flag);
  }
  for (const score of action.score || []) {
    effects.push({ do: "score", id: score.id, points: score.points });
    const prev = scores.get(score.id);
    if (prev != null && prev !== score.points) {
      throw new Error(`score ${score.id} is ${prev} and ${score.points}`);
    }
    scores.set(score.id, score.points);
  }
  if (action.sfx) effects.push({ do: "sfx", id: action.sfx });
  else if (action.score?.length) effects.push({ do: "sfx", id: "score" });
  const lines = action.texts?.length ? action.texts.slice() : action.text ? [action.text] : [];
  if (!lines.length && !action.goto) lines.push("Okay.");
  for (const text of lines) effects.push({ do: "say", text });
  if (action.goto) effects.push({ do: "goto", room: action.goto, spawn: action.spawn || "default" });
  if (action.checkpoint) {
    let name = action.id;
    if (String(action.id).startsWith("show-guard")) name = "club-chase";
    if (action.id === "leave-slipped") name = "alley-chase";
    effects.push({ do: "checkpoint", name });
  }
  return effects;
}

function expand(action, roomId) {
  const verbWords = asList(action.verb) || [];
  const nounWords = asList(action.noun);
  const secondWords = asList(action.second);
  const verbIds = [...new Set(verbWords.map(canonVerb))];
  const nounIds = nounWords == null ? [null] : [...new Set(nounWords.map((word) => (word === "" ? "" : canonNoun(word))))];
  const secondIds = secondWords == null ? [null] : [...new Set(secondWords.map((word) => (word === "" ? "" : canonNoun(word))))];
  for (const id of nounIds) if (id) ensureNoun(id);
  for (const id of secondIds) if (id) ensureNoun(id);
  const req = (action.req || []).slice();
  if (action.once) req.push(`!flag:did_${String(action.id).replace(/[^A-Za-z0-9_]/g, "_")}`);
  for (const cond of req) {
    const body = cond.replace(/^!/, "");
    if (body.startsWith("flag:")) usedFlags.add(body.slice(5));
    if (body.startsWith("has:")) usedItems.add(body.slice(4));
  }
  const effects = effectsFor(action, roomId);
  const rules = [];
  let n = 0;
  for (const verb of verbIds) {
    for (const noun of nounIds) {
      for (const second of secondIds) {
        const rule = { verb, effects, req };
        if (noun == null) rule.anyNoun = true;
        else if (noun !== "") rule.noun = noun;
        if (second == null) rule.anySecond = true;
        else if (second !== "") rule.second = second;
        let id = `${roomId}:${action.id}`;
        if (n > 0) id += `:${n}`;
        n += 1;
        while (ruleIds.has(id)) id += "x";
        ruleIds.add(id);
        rule.id = id;
        rules.push(rule);
      }
    }
  }
  return rules;
}

function loadRoom(id) {
  return JSON.parse(fs.readFileSync(path.join(roomsDir, `${id}.json`), "utf8"));
}

const roomDocs = Object.fromEntries(ROOMS.map((id) => [id, loadRoom(id)]));
const globalDoc = loadRoom("global");

function roomMap(id) {
  const map = {};
  map.man = id === "alley" ? "professor" : id === "club" || id === "billiard" ? "guard" : id === "street" ? "doorman" : "sal";
  if (id !== "rooftop") map.tank = "bouncer";
  if (id !== "street") map.doorman = "bouncer";
  if (id === "club") map.sal = "mixologist";
  if (id !== "bathroom") map.machine = "jukebox";
  if (LAKE.has(id)) {
    map.hallway = "corridor";
    map.stairs = "rope";
    map.vip = "rope";
    map.ropes = "rope";
  } else {
    map.rope = "stairs";
  }
  return map;
}

ensureNoun("rope");
ensureNoun("drink");
ensureNoun("around");
ensureNoun("room");
ensureNoun("down");
ensureNoun("off");
ensureNoun("self");
ensureNoun("breath");
ensureNoun("no");
ensureNoun("help");
ensureNoun("exit");
ensureNoun("back");
ensureNoun("out");
ensureNoun("change");

const phrases = [
  ["pick up", "get"],
  ["look at", "look"],
  ["look in", "search"],
  ["talk to", "talk"],
  ["speak to", "talk"],
  ["speak with", "talk"],
  ["chat with", "talk"],
  ["ask for", "ask"],
  ["ask about", "ask"],
  ["brooklyn moon", "b7"],
  ["tire iron", "iron"],
  ["tyre iron", "iron"],
  ["fire escape", "ladder"],
  ["tip jar", "jar"],
  ["old man", "earl"],
  ["homeless man", "professor"],
  ["bar man", "sal"],
  ["midnight stallion", "cologne"],
  ["bar of soap", "soap"],
  ["vending machine", "machine"],
  ["cologne machine", "machine"],
  ["dollar bill", "dollar"],
  ["put on", "wear"],
  ["license plate", "note"],
  ["licence plate", "note"],
  ["domino brothers", "regulars"],
  ["purple rhino", "club"],
  ["hair pick", "pick"],
  ["bobby pin", "pick"],
  ["water tank", "tank"],
  ["velvet sign", "sign"],
  ["rhino sign", "sign"],
  ["mirror ball", "ball"],
  ["dance floor", "floor"],
  ["private door", "private"],
  ["brass doors", "door"],
  ["brass door", "door"],
  ["vip stairs", "stairs"],
  ["billiard ball", "ball"],
  ["cue ball", "ball"],
  ["pool ball", "ball"],
  ["golden cow", "chips"],
  ["chlorine tablets", "chlorine"],
  ["smoke bomb", "bomb"],
  ["account books", "books"],
  ["desk phone", "phone"],
  ["stand up", "stand"],
  ["get up", "stand"],
  ["hold your breath", "hold breath"],
  ["hold his breath", "hold breath"],
  ["hold my breath", "hold breath"],
  ["hold breath", "hold breath"],
  ["climb out", "surface"],
  ["get into the water", "dive pool"],
  ["get into the pool", "dive pool"],
  ["get in the water", "dive pool"],
  ["get in the pool", "dive pool"],
  ["get into water", "dive pool"],
  ["get into pool", "dive pool"],
  ["get in water", "dive pool"],
  ["get in pool", "dive pool"],
  ["thugs clothes", "clothes"],
  ["goons clothes", "clothes"],
  ["dry clothes", "clothes"],
];

const verbList = [...verbs.groups.entries()].map(([id, words]) => {
  const verb = { id, words };
  if (FREE.has(id)) verb.ranged = true;
  if (META.has(id)) verb.meta = true;
  if (VERB_DEFAULT[id]) verb.default = VERB_DEFAULT[id];
  if (id === "tip") verb.targets = { bar: "sal", club: "dancer" };
  if (id === "play") verb.targets = { bar: "jukebox" };
  if (id === "buy") verb.targets = { bar: "sal" };
  return verb;
});

function hotspotNouns(words) {
  const ids = [];
  for (const word of words || []) {
    const id = canonNoun(word);
    ensureNoun(id);
    if (!ids.includes(id)) ids.push(id);
  }
  return ids;
}

function clamp(n, lo, hi) {
  return Math.max(lo, Math.min(hi, n | 0));
}

function standOf(hotspot) {
  const point = hotspot.interact || hotspot.approach;
  if (!point) return null;
  return [clamp(point[0], 2, 381), clamp(point[1], 2, 213)];
}

const COLORS = {
  bar: [42, 28, 24],
  bathroom: [48, 58, 62],
  alley: [24, 28, 36],
  club: [48, 18, 42],
  rooftop: [28, 32, 40],
  street: [36, 32, 28],
  billiard: [36, 28, 22],
  deck: [18, 36, 42],
  hall: [40, 32, 26],
  kitchen: [44, 36, 28],
  hallway: [32, 30, 28],
  office: [30, 26, 22],
};

function png(w, h, rgba) {
  const raw = Buffer.alloc((w * 4 + 1) * h);
  for (let y = 0; y < h; y++) {
    const row = y * (w * 4 + 1);
    raw[row] = 0;
    rgba.copy(raw, row + 1, y * w * 4, (y + 1) * w * 4);
  }
  const sig = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  return Buffer.concat([
    sig,
    chunk("IHDR", ihdr),
    chunk("IDAT", zlib.deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}
function chunk(type, data) {
  const head = Buffer.alloc(8);
  head.writeUInt32BE(data.length, 0);
  head.write(type, 4);
  const crc = zlib.crc32(Buffer.concat([Buffer.from(type), data]));
  const tail = Buffer.alloc(4);
  tail.writeUInt32BE(crc >>> 0, 0);
  return Buffer.concat([head, data, tail]);
}

function fill(w, h, rgb) {
  const rgba = Buffer.alloc(w * h * 4);
  for (let i = 0; i < w * h; i++) {
    rgba[i * 4] = rgb[0];
    rgba[i * 4 + 1] = rgb[1];
    rgba[i * 4 + 2] = rgb[2];
    rgba[i * 4 + 3] = 255;
  }
  return rgba;
}

function paintHero(rgbSuit) {
  const fw = 16;
  const fh = 24;
  const frames = 12;
  const rgba = Buffer.alloc(fw * frames * fh * 4);
  const put = (frame, x, y, rgb) => {
    if (x < 0 || y < 0 || x >= fw || y >= fh) return;
    const i = ((y * fw * frames + frame * fw + x) * 4);
    rgba[i] = rgb[0];
    rgba[i + 1] = rgb[1];
    rgba[i + 2] = rgb[2];
    rgba[i + 3] = 255;
  };
  const skin = [232, 196, 150];
  const hair = [214, 176, 64];
  const shoe = [40, 32, 28];
  for (let frame = 0; frame < frames; frame++) {
    const bob = frame % 3 === 1 ? 1 : 0;
    for (let y = 2; y < 6; y++) for (let x = 5; x < 11; x++) put(frame, x, y + bob, hair);
    for (let y = 5; y < 9; y++) for (let x = 6; x < 10; x++) put(frame, x, y + bob, skin);
    put(frame, 7, 7 + bob, [40, 70, 120]);
    put(frame, 9, 7 + bob, [40, 70, 120]);
    for (let y = 9; y < 16; y++) for (let x = 5; x < 11; x++) put(frame, x, y + bob, rgbSuit);
    const step = frame % 3;
    for (let y = 16; y < 22; y++) {
      put(frame, 6, y, rgbSuit);
      put(frame, 9, y, rgbSuit);
    }
    put(frame, 6 + (step === 2 ? 1 : 0), 22, shoe);
    put(frame, 9 - (step === 1 ? 1 : 0), 22, shoe);
    put(frame, 4, 11 + bob, rgbSuit);
    put(frame, 11, 11 + bob, rgbSuit);
  }
  return png(fw * frames, fh, rgba);
}

function roomPicture(id) {
  const src = path.join(dw, "public/games/charlie/pixel/rooms", `${id}.png`);
  if (fs.existsSync(src)) {
    const bytes = fs.readFileSync(src);
    if (bytes.readUInt32BE(16) === 384 && bytes.readUInt32BE(20) === 216) return bytes;
  }
  const rgba = fill(384, 216, COLORS[id] || [20, 20, 20]);
  const floor = [Math.min(255, (COLORS[id][0] || 20) + 28), Math.min(255, (COLORS[id][1] || 20) + 24), Math.min(255, (COLORS[id][2] || 20) + 18)];
  for (let y = 150; y < 216; y++) {
    for (let x = 0; x < 384; x++) {
      const i = (y * 384 + x) * 4;
      rgba[i] = floor[0];
      rgba[i + 1] = floor[1];
      rgba[i + 2] = floor[2];
    }
  }
  return png(384, 216, rgba);
}

function dynamicLook(docs) {
  const alley = JSON.stringify(docs.alley.look + " A soggy matchbook sits in a puddle by the Purple Rhino door.");
  const roofPick = JSON.stringify(docs.rooftop.look + " The hair pick in Charlie's pocket was made for that lock.");
  const roofEmpty = JSON.stringify(docs.rooftop.look + " The access door is locked. A hair pick would open it. Empty hands will not.");
  return `fn dynamic_look() {
    if verb() != "look" {
        return false;
    }
    if noun() != "" && noun() != "around" && noun() != "room" {
        return false;
    }
    if room() == "alley" && flag("matchbookHere") && !has("matchbook") {
        say(${alley});
        handled();
        return true;
    }
    if room() == "rooftop" {
        if has("pick") {
            say(${roofPick});
        } else {
            say(${roofEmpty});
        }
        handled();
        return true;
    }
    false
}
`;
}

fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(out, { recursive: true });

const gameRules = [];
for (const action of globalDoc.actions || []) gameRules.push(...expand(action, "global"));

const roomRules = {};
for (const id of ROOMS) {
  roomRules[id] = [];
  for (const action of roomDocs[id].actions || []) roomRules[id].push(...expand(action, id));
  for (const hotspot of roomDocs[id].hotspots || []) {
    for (const word of hotspot.nouns || []) hotspotNouns([word]);
  }
}

const total = [...scores.values()].reduce((sum, points) => sum + points, 0);
if (total !== 168) {
  console.error(`score table sums to ${total}, expected 168`);
  for (const [id, points] of [...scores.entries()].sort()) console.error(`  ${id} ${points}`);
  process.exit(1);
}

const game = {
  id: "charlie",
  title: "Charlie: Last Call",
  version: "1.0.0",
  formatVersion: "0.2.0",
  engineMin: "0.2.0",
  resolution: [384, 216],
  startRoom: "bar",
  startSpawn: "default",
  hero: "charlie",
  scoreMax: 168,
  rngSeed: 1,
  reach: 42,
  intro: [],
  carrying: ["twenty", "dollar", "dollar", "dollar"],
  deathPrompt: prompt,
  hooks: {
    early: "on_early",
    command: "on_command",
    enter: "on_enter",
    cutsceneCommand: "on_cutscene_command",
    after: "on_after",
  },
  timers: [
    {
      id: "clubChase",
      when: ["room:club", "flag:clubChase"],
      limit: 4,
      var: "clockClub",
      effects: [
        { do: "say", text: clubTimeout },
        { do: "say", text: prompt },
        { do: "setDead", restoreTo: "club-chase" },
      ],
    },
    {
      id: "alleyChase",
      when: ["room:alley", "flag:alleyChase"],
      limit: 4,
      var: "clockAlley",
      effects: [
        { do: "say", text: alleyTimeout },
        { do: "say", text: prompt },
        { do: "setDead", restoreTo: "alley-chase" },
      ],
    },
  ],
  declarations: {
    flags: [...usedFlags].sort(),
    vars: ["thug", "police", "billiardMoves", "clockClub", "clockAlley"],
    scores: [...scores.entries()].sort().map(([id, points]) => ({ id, points })),
    endings: ["good", "bad"],
  },
  rules: gameRules,
};

const vocab = {
  ignore: ["a", "an", "the"],
  prepositions: ["to", "on", "in", "into", "with", "for", "at", "about", "from", "by"],
  phrases: phrases.map(([from, to]) => ({ from, to })),
  rewrites: [
    { pattern: "\\$20", replace: " twenty " },
    { pattern: "\\$10", replace: " ten " },
    { pattern: "\\$5", replace: " five " },
    { pattern: "\\$1", replace: " dollar " },
  ],
  verbs: verbList,
  nouns: [...nouns.groups.entries()].map(([id, words]) => ({ id, words })),
  rooms: Object.fromEntries(ROOMS.map((id) => [id, roomMap(id)])),
  unknownWord: "I don't know the word '{word}'.",
  unknownCommand: "I don't understand that.",
  bare: { go: "out" },
};

const items = [...usedItems].sort().map((id) => {
  const labels = ITEM_LABEL[id] || [id, id];
  return { id, name: labels[0], plural: labels[1] };
});

fs.writeFileSync(path.join(out, "game.json"), JSON.stringify(game, null, 2) + "\n");
fs.writeFileSync(path.join(out, "vocab.json"), JSON.stringify(vocab, null, 2) + "\n");
fs.writeFileSync(path.join(out, "items.json"), JSON.stringify(items, null, 2) + "\n");

for (const id of ROOMS) {
  const doc = roomDocs[id];
  const dir = path.join(out, "rooms", id);
  fs.mkdirSync(dir, { recursive: true });
  const spawns = {};
  for (const [name, point] of Object.entries(doc.spawn || { default: [176, 180] })) {
    spawns[name] = [clamp(point[0], 2, 381), clamp(point[1], 2, 213)];
  }
  if (!spawns.default) spawns.default = [176, 180];
  const hotspots = [];
  for (const hotspot of doc.hotspots || []) {
    const nounsHere = hotspotNouns(hotspot.nouns);
    if (!nounsHere.length) continue;
    const entry = {
      id: hotspot.id,
      rect: hotspot.rect,
      nouns: nounsHere,
    };
    if (hotspot.look) entry.look = hotspot.look;
    const stand = standOf(hotspot);
    if (stand) entry.stand = stand;
    hotspots.push(entry);
  }
  const room = {
    id,
    name: doc.name,
    look: doc.look || doc.name,
    spawns,
    exits: [],
    hotspots,
    actors: [],
    props: [],
    rules: roomRules[id],
  };
  fs.writeFileSync(path.join(dir, "room.json"), JSON.stringify(room, null, 2) + "\n");
  fs.writeFileSync(
    path.join(dir, "walk.json"),
    JSON.stringify({ polygons: [{ points: [[1, 1], [382, 1], [382, 214], [1, 214]] }] }, null, 2) + "\n",
  );
  fs.writeFileSync(path.join(dir, "priority.json"), JSON.stringify({ bands: [{ y: 0, priority: 0 }] }, null, 2) + "\n");
  fs.writeFileSync(path.join(dir, "picture.png"), roomPicture(id));
}

const actorDir = path.join(out, "actors", "charlie");
fs.mkdirSync(actorDir, { recursive: true });
const grey = [92, 98, 108];
const brown = [122, 84, 52];
fs.writeFileSync(path.join(actorDir, "sheet.png"), paintHero(grey));
fs.writeFileSync(path.join(actorDir, "sheet-dry.png"), paintHero(brown));
const anim = (frames, fps) => ({ fps, frames });
const animations = {
  stand_s: anim([0], 1),
  walk_s: anim([1, 2], 8),
  stand_n: anim([3], 1),
  walk_n: anim([4, 5], 8),
  stand_e: anim([6], 1),
  walk_e: anim([7, 8], 8),
  stand_w: anim([9], 1),
  walk_w: anim([10, 11], 8),
};
fs.writeFileSync(
  path.join(actorDir, "actor.json"),
  JSON.stringify(
    {
      id: "charlie",
      costumes: {
        default: { sheet: "sheet.png", frameW: 16, frameH: 24, ax: 8, ay: 23, animations },
        dry: { sheet: "sheet-dry.png", frameW: 16, frameH: 24, ax: 8, ay: 23, animations },
      },
    },
    null,
    2,
  ) + "\n",
);

const scriptDir = path.join(out, "scripts");
fs.mkdirSync(scriptDir, { recursive: true });
let script = fs.readFileSync(path.join(path.dirname(new URL(import.meta.url).pathname), "charlie.rhai"), "utf8");
script = script.replace(/fn dynamic_look\(\) \{\n    false\n\}\n/, dynamicLook(roomDocs));
fs.writeFileSync(path.join(scriptDir, "game.rhai"), script);

const cutDir = path.join(out, "cutscenes");
fs.mkdirSync(cutDir, { recursive: true });
const dana = fs.readFileSync(path.join(dw, "lib/charlie/types.ts"), "utf8");
function tsConst(name) {
  const match = dana.match(new RegExp(`export const ${name} =\\n([\\s\\S]*?);`));
  return unquoteTs(match[1]);
}
fs.writeFileSync(
  path.join(cutDir, "alley-dana.json"),
  JSON.stringify(
    {
      id: "alley-dana",
      beats: [
        { say: [tsConst("CUTSCENE_STORM")] },
        { say: [tsConst("CUTSCENE_DANA")] },
        { say: [tsConst("CUTSCENE_CHOICE")] },
        { say: [tsConst("CUTSCENE_DROP")] },
      ],
      onEnd: [
        { do: "set", flag: "seenCutscene" },
        { do: "set", flag: "matchbookHere" },
        { do: "score", id: "witness-cutscene", points: 4 },
        { do: "sfx", id: "score" },
      ],
    },
    null,
    2,
  ) + "\n",
);

fs.mkdirSync(path.join(out, "tests"), { recursive: true });
fs.writeFileSync(
  path.join(out, "tests", "boot.walk"),
  "score\n@dismiss\n@expect score 0\n@expect room bar\n",
);

console.log(`imported charlie → ${out}`);
console.log(`rules ${ruleIds.size}, scores ${scores.size}, total ${total}`);
