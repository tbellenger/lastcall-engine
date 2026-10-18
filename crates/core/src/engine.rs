use crate::cond::{self, CondWorld};
use crate::error::{Error, Result};
use crate::geom::{self, astar, in_mask, nearest_walkable, point_in_rect};
use crate::imageutil::SpriteFrame;
use crate::model::{Dir, Effect, Rule, SCREEN_H, SCREEN_W};
use crate::pak::{self, Package};
use crate::parser::Parsed;
use crate::render::{self, Layer};
use crate::rng::Pcg32;
use crate::save::{self, Checkpoint, World};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, VecDeque};

const SPEED: i32 = 160;
const DIAG_NUM: i32 = 181;
const DIAG_DEN: i32 = 256;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Input {
    KeyDown { key: String },
    KeyUp { key: String },
    Char { ch: String },
    Backspace,
    Submit,
    SetLine { text: String },
    Dismiss,
    Repeat,
    Tap { x: i32, y: i32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostEvent {
    Sfx { id: String },
    Music { id: String },
    Say { text: String },
    Autosave,
    Ui { action: String, slot: String },
    RoomChanged { room: String },
    Warning { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Active {
    Idle,
    Walking,
    Waiting { left: u32 },
    Saying,
    Dead,
    Ended,
}

struct Route {
    points: Vec<(i32, i32)>,
    index: usize,
    stuck: u32,
}

enum RoutePlan {
    AlreadyThere,
    Path(Vec<(i32, i32)>),
    Unreachable,
}

pub struct Engine {
    pkg: Package,
    world: World,
    rng: Pcg32,
    checkpoints: BTreeMap<String, Checkpoint>,
    checkpoint_stack: Vec<String>,
    queue: VecDeque<Effect>,
    active: Active,
    current_text: Option<String>,
    said: Vec<String>,
    input_line: String,
    last_command: String,
    walk_dir: Option<Dir>,
    forced_dir: Dir,
    forced_ticks: u32,
    route: Option<Route>,
    route_is_effect: bool,
    overlap: Option<String>,
    moved: bool,
    pending: Option<Parsed>,
    hook_verb: String,
    hook_noun: String,
    hook_second: String,
    /// Set by `keepAutosave` so later room changes leave the arrival snapshot alone.
    autosave_locked: bool,
    /// A room change wants a snapshot after the entry effects that run before the first line.
    autosave_pending: bool,
    events: Vec<HostEvent>,
    fb: Vec<u8>,
}

impl Engine {
    pub fn load(bytes: &[u8]) -> Result<Self> {
        let pkg = pak::load_pak(bytes)?;
        let mut engine = Self::from_package(pkg)?;
        engine.boot()?;
        Ok(engine)
    }

    pub fn from_package(pkg: Package) -> Result<Self> {
        let rng = Pcg32::new(pkg.game.rng_seed);
        let mut world = World::new(0);
        world.costume = "default".into();
        Ok(Self {
            pkg,
            world,
            rng,
            checkpoints: BTreeMap::new(),
            checkpoint_stack: Vec::new(),
            queue: VecDeque::new(),
            active: Active::Idle,
            current_text: None,
            said: Vec::new(),
            input_line: String::new(),
            last_command: String::new(),
            walk_dir: None,
            forced_dir: Dir::S,
            forced_ticks: 0,
            route: None,
            route_is_effect: false,
            overlap: None,
            moved: false,
            pending: None,
            hook_verb: String::new(),
            hook_noun: String::new(),
            hook_second: String::new(),
            autosave_locked: false,
            autosave_pending: false,
            events: Vec::new(),
            fb: vec![0; (SCREEN_W * SCREEN_H * 4) as usize],
        })
    }

    fn boot(&mut self) -> Result<()> {
        if self.pkg.newer_minor {
            self.events.push(HostEvent::Warning {
                message: format!(
                    "package format {} is newer than this engine; some data may be ignored",
                    self.pkg.game.format_version
                ),
            });
        }
        for id in self.pkg.game.carrying.clone() {
            *self.world.inventory.entry(id).or_insert(0) += 1;
        }
        let room = self.pkg.game.start_room.clone();
        let spawn = self.pkg.game.start_spawn.clone();
        self.place(&room, &spawn)?;
        self.take_checkpoint("start");
        for line in self.pkg.game.intro.clone() {
            self.queue.push_back(Effect::Say { text: line });
        }
        self.queue_on_enter();
        self.pump();
        // The boot snapshot includes entry flags. Later rooms replace it on arrival.
        self.remember_autosave();
        self.render();
        Ok(())
    }

    pub fn input(&mut self, event: Input) {
        match event {
            Input::KeyDown { key } => self.key_down(&key),
            Input::KeyUp { .. } => {}
            Input::Char { ch } if !self.locked() => {
                for c in ch.chars().filter(|c| !c.is_control()) {
                    self.input_line.push(c);
                }
            }
            Input::Char { .. } => {}
            Input::Backspace if !self.locked() => {
                self.input_line.pop();
            }
            Input::Backspace => {}
            Input::SetLine { text } if !self.locked() => self.input_line = text,
            Input::SetLine { .. } => {}
            Input::Submit => self.submit(),
            Input::Dismiss => {
                if matches!(self.active, Active::Saying) {
                    let _ = self.dismiss_narration();
                    return;
                }
            }
            Input::Repeat => {
                let line = self.last_command.clone();
                if !line.is_empty() && !self.locked() {
                    self.input_line = line;
                    self.submit();
                    return;
                }
            }
            Input::Tap { x, y } => self.tap(x, y),
        }
        self.render();
    }

    /// Run a command while a cutscene line is up, without advancing the cutscene.
    pub fn interrupt_line(&mut self, line: &str) {
        self.queue.clear();
        self.current_text = None;
        self.active = Active::Idle;
        self.run_command(line);
    }

    pub fn submit_line(&mut self, line: &str) {
        self.input_line = line.to_string();
        self.submit();
        self.render();
    }

    pub fn dismiss_narration(&mut self) -> std::result::Result<(), String> {
        if !matches!(self.active, Active::Saying) {
            return Err("nothing to dismiss".into());
        }
        self.current_text = None;
        self.active = Active::Idle;
        self.pump();
        self.advance_cutscene_if_idle();
        self.render();
        Ok(())
    }

    pub fn step(&mut self, ticks: u32) {
        for _ in 0..ticks {
            self.world.tick = self.world.tick.saturating_add(1);
            self.pump();
            self.moved = false;
            match self.active {
                Active::Walking => self.advance_route(),
                Active::Waiting { left } => {
                    if left <= 1 {
                        self.active = Active::Idle;
                        if self.autoplay_cutscene() {
                            self.advance_autoplay();
                        }
                        self.pump();
                    } else {
                        self.active = Active::Waiting { left: left - 1 };
                    }
                }
                Active::Idle => self.free_move(),
                Active::Saying | Active::Dead | Active::Ended => {}
            }
            if self.moved && !self.route_is_effect {
                self.check_exits();
            }
            self.tick_flipbook();
        }
        self.render();
    }

    /// Place the hero at a room's default spawn, facing away, with no narration.
    /// Used to compare a room plate against the pixel-mode reference.
    pub fn preview_room(&mut self, room: &str) -> Result<()> {
        self.world.cutscene = None;
        self.world.cut_beat = 0;
        self.world.cut_frame = 0;
        self.world.cut_flip = 0;
        self.world.picture.clear();
        self.current_text = None;
        self.input_line.clear();
        self.queue.clear();
        self.active = Active::Idle;
        self.route = None;
        self.walk_dir = None;
        self.forced_ticks = 0;
        self.world.actors.remove(&self.pkg.game.hero);
        self.place(room, "default")?;
        self.world.facing = Dir::N;
        self.autosave_pending = false;
        self.render();
        Ok(())
    }

    pub fn framebuffer(&self) -> &[u8] {
        &self.fb
    }

    pub fn width(&self) -> u32 {
        SCREEN_W
    }

    pub fn height(&self) -> u32 {
        SCREEN_H
    }

    pub fn frame_hash(&self) -> String {
        hex(&Sha256::digest(&self.fb))
    }

    pub fn drain_events(&mut self) -> Vec<HostEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn save(&self) -> Result<Vec<u8>> {
        save::encode_save(
            &self.pkg,
            &self.world,
            self.rng,
            &self.checkpoints,
            &self.checkpoint_stack,
            self.autosave_locked,
        )
    }

    pub fn restore(&mut self, bytes: &[u8]) -> Result<()> {
        let decoded = save::decode_save(&self.pkg, bytes)?;
        if decoded.pack_mismatch {
            self.events.push(HostEvent::Warning {
                message: "save is from a different package build".into(),
            });
        }
        self.world = decoded.world;
        self.rng = decoded.rng;
        self.checkpoints = decoded.checkpoints;
        self.checkpoint_stack = decoded.stack;
        self.autosave_locked = decoded.autosave_locked;
        self.autosave_pending = false;
        self.reset_transient();
        self.events.push(HostEvent::RoomChanged {
            room: self.world.room.clone(),
        });
        self.render();
        Ok(())
    }

    pub fn restore_last_checkpoint(&mut self) -> Result<()> {
        let name = self
            .checkpoint_stack
            .last()
            .cloned()
            .ok_or_else(|| Error::msg("there is no checkpoint to restore"))?;
        self.restore_checkpoint(&name)
    }

    pub fn room_id(&self) -> &str {
        &self.world.room
    }

    pub fn score(&self) -> i32 {
        self.world
            .scored
            .iter()
            .map(|id| self.pkg.score_table.get(id).copied().unwrap_or(0))
            .sum()
    }

    pub fn score_max(&self) -> i32 {
        self.pkg.game.score_max
    }

    pub fn has_flag(&self, name: &str) -> bool {
        self.world.flags.contains(name)
    }

    pub fn var(&self, name: &str) -> i32 {
        self.world.vars.get(name).copied().unwrap_or(0)
    }

    pub fn item_count(&self, id: &str) -> i32 {
        self.world.inventory.get(id).copied().unwrap_or(0)
    }

    pub fn is_dead(&self) -> bool {
        self.world.dead
    }

    pub fn ending(&self) -> Option<&str> {
        self.world.ending.as_deref()
    }

    pub fn said(&self) -> &[String] {
        &self.said
    }

    pub fn flags(&self) -> Vec<String> {
        self.world.flags.iter().cloned().collect()
    }

    pub fn vars(&self) -> Vec<(String, i32)> {
        self.world
            .vars
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect()
    }

    pub fn inventory(&self) -> Vec<(String, i32)> {
        self.world
            .inventory
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect()
    }

    /// True while a line, a walk, or a wait still has to finish.
    pub fn presenting(&self) -> bool {
        self.route.is_some()
            || !self.queue.is_empty()
            || matches!(
                self.active,
                Active::Saying | Active::Walking | Active::Waiting { .. }
            )
    }

    pub fn hero_px(&self) -> (i32, i32) {
        (self.world.hero_x >> 8, self.world.hero_y >> 8)
    }

    pub fn is_locked(&self) -> bool {
        !matches!(self.active, Active::Idle)
    }

    pub fn route_active(&self) -> bool {
        self.route.is_some()
    }

    pub fn script_walk_to(&mut self, x: i32, y: i32) -> std::result::Result<(), String> {
        if self.is_locked() {
            return Err("input is locked; dismiss the narrator first".into());
        }
        match self.plan_route(x, y) {
            RoutePlan::AlreadyThere => Ok(()),
            RoutePlan::Unreachable => Err(format!("no path to {x},{y}")),
            RoutePlan::Path(points) => {
                self.walk_dir = None;
                self.route = Some(Route {
                    points,
                    index: 0,
                    stuck: 0,
                });
                self.route_is_effect = false;
                Ok(())
            }
        }
    }

    pub fn force_walk(&mut self, dir: Dir, ticks: u32) {
        self.walk_dir = None;
        if !self.route_is_effect {
            self.route = None;
        }
        self.forced_dir = dir;
        self.forced_ticks = ticks;
    }

    pub fn hotspot_stand(&self, id: &str) -> Option<(i32, i32)> {
        self.pkg.rooms[&self.world.room]
            .def
            .hotspots
            .iter()
            .find(|hs| hs.id == id)
            .and_then(|hs| hs.stand)
            .map(|p| (p[0], p[1]))
    }

    pub fn exit_target(&self, id: &str) -> Option<(i32, i32)> {
        let room = &self.pkg.rooms[&self.world.room];
        let exit = room.def.exits.iter().find(|ex| ex.id == id)?;
        if let Some(approach) = exit.approach {
            return Some((approach[0], approach[1]));
        }
        let w = SCREEN_W as i32;
        let h = SCREEN_H as i32;
        let mut best: Option<(i64, i32, i32)> = None;
        let cx = exit.trigger[0] + exit.trigger[2] / 2;
        let cy = exit.trigger[1] + exit.trigger[3] / 2;
        for y in exit.trigger[1]..exit.trigger[1] + exit.trigger[3] {
            for x in exit.trigger[0]..exit.trigger[0] + exit.trigger[2] {
                if !in_mask(&room.walk, w, h, x, y) {
                    continue;
                }
                let dx = (x - cx) as i64;
                let dy = (y - cy) as i64;
                let dist = dx * dx + dy * dy;
                let replace = match best {
                    None => true,
                    Some((bd, by, bx)) => {
                        dist < bd || (dist == bd && (y < by || (y == by && x < bx)))
                    }
                };
                if replace {
                    best = Some((dist, y, x));
                }
            }
        }
        best.map(|(_, y, x)| (x, y))
    }

    fn submit(&mut self) {
        if matches!(self.active, Active::Saying) {
            self.current_text = None;
            self.active = Active::Idle;
            self.pump();
            return;
        }
        if matches!(self.active, Active::Ended) {
            let line = self.input_line.trim().to_string();
            self.input_line.clear();
            if line.is_empty() {
                return;
            }
            self.active = Active::Idle;
            self.run_command(&line);
            return;
        }
        if matches!(self.active, Active::Dead) {
            let line = self.input_line.trim().to_string();
            self.input_line.clear();
            if line.is_empty() {
                return;
            }
            self.last_command = line.clone();
            let parsed = self.pkg.vocab.parse(&self.world.room, &line);
            if parsed.verb.as_deref() == Some("restore") {
                if let Err(err) = self.restore_after_death() {
                    self.force_say(err.to_string());
                }
            } else if self.pkg.game.death_prompt.is_some()
                && matches!(parsed.verb.as_deref(), Some("restart" | "quit"))
            {
                self.events.push(HostEvent::Ui {
                    action: parsed.verb.unwrap_or_default(),
                    slot: String::new(),
                });
            } else if let Some(prompt) = self.pkg.game.death_prompt.clone() {
                self.force_say(prompt);
            } else {
                self.force_say("Only restore will help now.".into());
            }
            return;
        }
        if self.is_locked() {
            return;
        }
        let line = self.input_line.trim().to_string();
        self.input_line.clear();
        if line.is_empty() {
            return;
        }
        self.run_command(&line);
    }

    fn run_command(&mut self, line: &str) {
        self.last_command = line.to_string();
        let parsed = self.pkg.vocab.parse(&self.world.room, line);
        if let Some(word) = &parsed.unknown {
            let message = self.pkg.vocab.unknown_message(word);
            self.force_say(message);
            return;
        }
        let Some(verb) = parsed.verb.clone() else {
            let message = self.pkg.vocab.unknown_command.clone();
            self.force_say(message);
            return;
        };
        self.hook_verb = verb.clone();
        self.hook_noun = parsed.noun.clone().unwrap_or_default();
        self.hook_second = parsed.second.clone().unwrap_or_default();
        if self.world.cutscene.is_some() {
            self.cutscene_command(&parsed);
            return;
        }
        if self.hook_handles("early") {
            self.pump();
            return;
        }
        if self.arm_timers(&verb) {
            return;
        }
        let prefix = self.reach_prefix(&parsed);
        if !prefix.is_empty() {
            self.pending = Some(parsed);
            self.queue.extend(prefix);
            self.pump();
            return;
        }
        self.finish_command(&parsed);
    }

    fn finish_command(&mut self, parsed: &Parsed) {
        let Some(verb) = parsed.verb.clone() else {
            return;
        };
        let scripted = self.hook_handles("command");
        if !scripted {
            if let Some(rule) = self.match_rule(parsed) {
                self.enqueue_rule(&rule, parsed);
            } else if !self.builtin(&verb, parsed) {
                if let Some(reply) = self.pkg.vocab.verb_default.get(&verb).cloned() {
                    self.queue.push_back(Effect::Say { text: reply });
                } else {
                    let message = self.pkg.vocab.unknown_command.clone();
                    self.queue.push_back(Effect::Say { text: message });
                }
            }
        }
        if !self
            .queue
            .iter()
            .any(|effect| matches!(effect, Effect::SetDead { .. }))
        {
            let _ = self.hook_handles("after");
        }
        self.pump();
    }

    fn builtin(&mut self, verb: &str, parsed: &Parsed) -> bool {
        match verb {
            "score" => {
                let text = format!("Your score is {} of {}.", self.score(), self.score_max());
                self.force_say(text);
                true
            }
            "inventory" => {
                self.force_say(self.inventory_text());
                true
            }
            "help" => {
                let verbs: Vec<&str> = self
                    .pkg
                    .vocab
                    .verb_meta
                    .keys()
                    .map(String::as_str)
                    .collect();
                self.force_say(format!("Verbs: {}.", verbs.join(", ")));
                true
            }
            "save" => {
                self.events.push(HostEvent::Ui {
                    action: "save".into(),
                    slot: "1".into(),
                });
                self.force_say("Saved.".into());
                true
            }
            "restore" => {
                self.events.push(HostEvent::Ui {
                    action: "restore".into(),
                    slot: "1".into(),
                });
                true
            }
            "look" => {
                if parsed.noun.is_none() {
                    let look = self.pkg.rooms[&self.world.room].def.look.clone();
                    let name = self.pkg.rooms[&self.world.room].def.name.clone();
                    self.force_say(if look.is_empty() { name } else { look });
                    return true;
                }
                let noun = parsed.noun.clone().unwrap_or_default();
                if self.item_count(&noun) > 0 {
                    if let Some(item) = self.pkg.item_index.get(&noun) {
                        if !item.look.is_empty() {
                            self.force_say(item.look.clone());
                            return true;
                        }
                    }
                }
                if let Some(text) = self.pkg.rooms[&self.world.room]
                    .def
                    .hotspots
                    .iter()
                    .find(|hs| hs.nouns.iter().any(|n| n == &noun))
                    .map(|hs| hs.look.clone())
                {
                    if !text.is_empty() {
                        self.force_say(text);
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    fn inventory_text(&self) -> String {
        let mut parts = Vec::new();
        for (id, count) in &self.world.inventory {
            if *count <= 0 {
                continue;
            }
            let item = self.pkg.item_index.get(id);
            let name = if *count > 1 {
                item.and_then(|it| {
                    if it.plural.is_empty() {
                        None
                    } else {
                        Some(it.plural.clone())
                    }
                })
                .unwrap_or_else(|| id.clone())
            } else {
                item.map(|it| it.name.clone()).unwrap_or_else(|| id.clone())
            };
            if *count > 1 {
                parts.push(format!("{count} {name}"));
            } else {
                parts.push(format!("a {name}"));
            }
        }
        if parts.is_empty() {
            "You are carrying nothing.".into()
        } else {
            format!("You are carrying {}.", parts.join(", "))
        }
    }

    fn match_rule(&self, parsed: &Parsed) -> Option<Rule> {
        let mut rules = self.pkg.rooms[&self.world.room].def.rules.clone();
        rules.extend(self.pkg.game.rules.clone());
        let mut best: Option<(u8, Rule)> = None;
        for rule in rules {
            if !self.rule_ok(&rule, parsed) {
                continue;
            }
            let spec = specificity(&rule);
            if best.as_ref().map(|(s, _)| spec > *s).unwrap_or(true) {
                best = Some((spec, rule));
            }
        }
        best.map(|(_, rule)| rule)
    }

    fn rule_ok(&self, rule: &Rule, parsed: &Parsed) -> bool {
        if rule.verb.is_none() || rule.verb != parsed.verb {
            return false;
        }
        if !rule.any_noun && !opt_eq(&rule.noun, &parsed.noun) {
            return false;
        }
        if !rule.any_second && !opt_eq(&rule.second, &parsed.second) {
            return false;
        }
        if rule.once && self.world.fired.contains(&rule.id) {
            return false;
        }
        self.reqs_ok(&rule.req)
    }

    fn enqueue_rule(&mut self, rule: &Rule, parsed: &Parsed) {
        let mut prefix = Vec::new();
        if let Some((stand, face)) = self.approach_stand(parsed) {
            let (hx, hy) = self.hero_px();
            let dx = hx as i64 - stand.0 as i64;
            let dy = hy as i64 - stand.1 as i64;
            let reach = self.pkg.game.reach as i64;
            if dx * dx + dy * dy > reach * reach {
                prefix.push(Effect::WalkTo {
                    x: stand.0,
                    y: stand.1,
                });
            }
            if let Some(dir) = face {
                prefix.push(Effect::Face { dir });
            }
        }
        prefix.extend(rule.effects.clone());
        self.queue.extend(prefix);
    }

    fn reach_prefix(&self, parsed: &Parsed) -> Vec<Effect> {
        let Some((stand, _)) = self.approach_stand(parsed) else {
            return Vec::new();
        };
        let (hx, hy) = self.hero_px();
        let dx = hx as i64 - stand.0 as i64;
        let dy = hy as i64 - stand.1 as i64;
        let reach = self.pkg.game.reach as i64;
        if dx * dx + dy * dy > reach * reach {
            vec![Effect::WalkTo {
                x: stand.0,
                y: stand.1,
            }]
        } else {
            Vec::new()
        }
    }

    fn approach_stand(&self, parsed: &Parsed) -> Option<((i32, i32), Option<Dir>)> {
        if self.world.held {
            return None;
        }
        let verb = parsed.verb.as_deref().unwrap_or("");
        if self.pkg.vocab.verb_meta.get(verb).copied().unwrap_or(false)
            || self
                .pkg
                .vocab
                .verb_ranged
                .get(verb)
                .copied()
                .unwrap_or(false)
        {
            return None;
        }
        if let Some(second) = &parsed.second {
            if let Some(stand) = self.stand_for_noun(second) {
                return Some(stand);
            }
        }
        if let Some(noun) = &parsed.noun {
            if let Some(stand) = self.stand_for_noun(noun) {
                return Some(stand);
            }
        }
        let fallback = self
            .pkg
            .vocab
            .verb_targets
            .get(verb)
            .and_then(|rooms| rooms.get(&self.world.room))
            .cloned();
        fallback.and_then(|noun| self.stand_for_noun(&noun))
    }

    fn stand_for_noun(&self, noun: &str) -> Option<((i32, i32), Option<Dir>)> {
        let hs = self.pkg.rooms[&self.world.room]
            .def
            .hotspots
            .iter()
            .find(|hs| hs.nouns.iter().any(|n| n == noun))?;
        let stand = hs.stand?;
        Some(((stand[0], stand[1]), hs.face))
    }

    fn reqs_ok(&self, reqs: &[String]) -> bool {
        let view = self.view();
        reqs.iter()
            .all(|cond| cond::eval_str(&view, cond).unwrap_or(false))
    }

    fn view(&self) -> CondWorld<'_> {
        CondWorld {
            room: &self.world.room,
            flags: &self.world.flags,
            vars: &self.world.vars,
            inventory: &self.world.inventory,
        }
    }

    fn force_say(&mut self, text: String) {
        self.queue.push_back(Effect::Say { text });
        self.pump();
    }

    fn pump(&mut self) {
        while matches!(self.active, Active::Idle) {
            let Some(effect) = self.queue.pop_front() else {
                self.flush_autosave();
                break;
            };
            if self.apply(effect) {
                break;
            }
        }
        self.settle();
    }

    fn flush_autosave(&mut self) {
        if !self.autosave_pending {
            return;
        }
        self.autosave_pending = false;
        self.remember_autosave();
        self.events.push(HostEvent::Autosave);
    }

    fn settle(&mut self) {
        if matches!(self.active, Active::Idle) {
            if self.world.dead {
                self.active = Active::Dead;
            } else if self.world.ending.is_some() {
                self.active = Active::Ended;
            }
        }
    }

    /// Returns true when the effect blocks the queue.
    fn apply(&mut self, effect: Effect) -> bool {
        match effect {
            Effect::Say { text } => {
                self.flush_autosave();
                self.said.push(text.clone());
                self.current_text = Some(text.clone());
                self.events.push(HostEvent::Say { text });
                self.active = Active::Saying;
                true
            }
            Effect::Wait { ticks } => {
                if ticks == 0 {
                    false
                } else {
                    self.active = Active::Waiting { left: ticks };
                    true
                }
            }
            Effect::WalkTo { x, y } => match self.plan_route(x, y) {
                RoutePlan::AlreadyThere => false,
                RoutePlan::Path(points) => {
                    self.route = Some(Route {
                        points,
                        index: 0,
                        stuck: 0,
                    });
                    self.route_is_effect = true;
                    self.walk_dir = None;
                    self.active = Active::Walking;
                    true
                }
                RoutePlan::Unreachable => {
                    self.queue.clear();
                    self.route = None;
                    self.route_is_effect = false;
                    let text = "You can't get there from here.".to_string();
                    self.said.push(text.clone());
                    self.current_text = Some(text.clone());
                    self.events.push(HostEvent::Say { text });
                    self.active = Active::Saying;
                    true
                }
            },
            Effect::Take { id } => {
                *self.world.inventory.entry(id).or_insert(0) += 1;
                false
            }
            Effect::Give { id } => {
                if let Some(count) = self.world.inventory.get_mut(&id) {
                    *count -= 1;
                    if *count <= 0 {
                        self.world.inventory.remove(&id);
                    }
                }
                false
            }
            Effect::Set { flag } => {
                self.world.flags.insert(flag);
                false
            }
            Effect::Clear { flag } => {
                self.world.flags.remove(&flag);
                false
            }
            Effect::Inc { var, by } => {
                *self.world.vars.entry(var).or_insert(0) += by;
                false
            }
            Effect::SetVar { var, value } => {
                self.world.vars.insert(var, value);
                false
            }
            Effect::Score { id, .. } => {
                self.world.scored.insert(id);
                false
            }
            Effect::Goto { room, spawn } => {
                if self.place(&room, &spawn).is_ok() {
                    // A checkpoint parked on the goto records the room Charlie
                    // just entered, before the entry line is spoken.
                    while matches!(self.queue.front(), Some(Effect::Checkpoint { .. })) {
                        if let Some(Effect::Checkpoint { name }) = self.queue.pop_front() {
                            self.take_checkpoint(&name);
                        }
                    }
                    self.queue_on_enter();
                } else {
                    self.queue.push_front(Effect::Say {
                        text: "You can't go that way.".into(),
                    });
                }
                false
            }
            Effect::SetDead { restore_to } => {
                self.world.dead = true;
                self.world.death_restore = restore_to;
                false
            }
            Effect::SetEnding { id } => {
                self.world.ending = Some(id);
                false
            }
            Effect::Checkpoint { name } => {
                self.take_checkpoint(&name);
                false
            }
            Effect::KeepAutosave => {
                self.autosave_locked = true;
                false
            }
            Effect::Sfx { id } => {
                self.events.push(HostEvent::Sfx { id });
                false
            }
            Effect::Face { dir } => {
                self.world.facing = dir;
                false
            }
            Effect::MarkOnce { id } => {
                self.world.fired.insert(id);
                false
            }
            Effect::Call { name } => {
                let _ = self.hook_handles_named(&name);
                false
            }
            Effect::Cutscene { id } => {
                if !self.pkg.cutscenes.contains_key(&id) {
                    self.queue.push_front(Effect::Say {
                        text: format!("The cutscene '{id}' is missing."),
                    });
                    return false;
                }
                self.world.cutscene = Some(id);
                self.world.cut_beat = 0;
                self.world.cut_frame = 0;
                self.world.cut_flip = 0;
                self.enter_cut_beat();
                false
            }
            Effect::Costume { id } => {
                self.world.costume = id;
                false
            }
            Effect::Teleport { x, y } => {
                self.world.hero_x = x * 256 + 128;
                self.world.hero_y = y * 256 + 128;
                false
            }
            Effect::Hold { locked } => {
                self.world.held = locked;
                if locked {
                    self.walk_dir = None;
                    if !self.route_is_effect {
                        self.route = None;
                    }
                }
                false
            }
            Effect::Picture { name } => {
                self.world.picture = name;
                false
            }
            Effect::Pose {
                actor,
                anim,
                x,
                y,
                hidden,
            } => {
                self.world
                    .actors
                    .insert(actor, pose_token(hidden, anim, x, y));
                false
            }
        }
    }

    fn place(&mut self, room: &str, spawn: &str) -> Result<()> {
        let loaded = self
            .pkg
            .rooms
            .get(room)
            .ok_or_else(|| Error::msg(format!("unknown room {room}")))?;
        let spot = loaded
            .def
            .spawns
            .get(spawn)
            .copied()
            .ok_or_else(|| Error::msg(format!("room {room} has no spawn {spawn}")))?;
        self.world.room = room.to_string();
        self.world.hero_x = spot[0] * 256 + 128;
        self.world.hero_y = spot[1] * 256 + 128;
        self.route = None;
        self.route_is_effect = false;
        self.overlap = None;
        self.walk_dir = None;
        self.world.picture.clear();
        if matches!(self.active, Active::Walking) {
            self.active = Active::Idle;
        }
        if !self.autosave_locked {
            // Snapshot after entry effects, so a cleared pose flag is not saved as still set.
            self.autosave_pending = true;
        }
        self.events.push(HostEvent::RoomChanged {
            room: room.to_string(),
        });
        Ok(())
    }

    fn queue_on_enter(&mut self) {
        let hooks = self.pkg.rooms[&self.world.room].def.on_enter.clone();
        let mut effects = Vec::new();
        for rule in &hooks {
            if self.reqs_ok(&rule.req) {
                effects.extend(rule.effects.clone());
            }
        }
        for effect in effects.into_iter().rev() {
            self.queue.push_front(effect);
        }
        if let Some(name) = self.pkg.game.hooks.enter.clone() {
            let _ = self.hook_handles_named(&name);
        }
    }

    fn take_checkpoint(&mut self, name: &str) {
        self.checkpoints.insert(
            name.to_string(),
            Checkpoint {
                world: self.world.clone(),
                rng_state: self.rng.state(),
                rng_inc: self.rng.inc(),
            },
        );
        self.checkpoint_stack.retain(|entry| entry != name);
        self.checkpoint_stack.push(name.to_string());
    }

    fn restore_checkpoint(&mut self, name: &str) -> Result<()> {
        self.restore_named(name, true)
    }

    fn restore_named(&mut self, name: &str, replay_enter: bool) -> Result<()> {
        let snap = self
            .checkpoints
            .get(name)
            .cloned()
            .ok_or_else(|| Error::msg(format!("missing checkpoint {name}")))?;
        let checkpoints = self.checkpoints.clone();
        let stack = self.checkpoint_stack.clone();
        self.world = snap.world;
        self.rng = Pcg32::from_parts(snap.rng_state, snap.rng_inc);
        self.checkpoints = checkpoints;
        self.checkpoint_stack = stack;
        self.reset_transient();
        if replay_enter {
            self.queue_on_enter();
            self.pump();
        }
        self.events.push(HostEvent::RoomChanged {
            room: self.world.room.clone(),
        });
        self.render();
        Ok(())
    }

    pub fn restore_after_death(&mut self) -> Result<()> {
        if let Some(name) = self.world.death_restore.clone() {
            // Charlie adopts the snapshot as it was saved. Entry lines do not replay.
            self.restore_named(&name, false)
        } else {
            self.restore_last_checkpoint()
        }
    }

    fn reset_transient(&mut self) {
        self.queue.clear();
        self.active = Active::Idle;
        self.current_text = None;
        self.said.clear();
        self.input_line.clear();
        self.route = None;
        self.route_is_effect = false;
        self.overlap = None;
        self.walk_dir = None;
        self.forced_ticks = 0;
        self.pending = None;
        self.settle();
    }

    fn locked(&self) -> bool {
        self.is_locked()
    }

    fn key_down(&mut self, key: &str) {
        let Some(dir) = arrow_dir(key) else {
            return;
        };
        if self.locked() || self.world.held {
            return;
        }
        self.forced_ticks = 0;
        if !self.route_is_effect {
            self.route = None;
        }
        if self.walk_dir == Some(dir) {
            self.walk_dir = None;
        } else {
            self.walk_dir = Some(dir);
        }
    }

    fn tap(&mut self, x: i32, y: i32) {
        if self.locked() || self.world.held {
            return;
        }
        let w = SCREEN_W as i32;
        let h = SCREEN_H as i32;
        let mask = &self.pkg.rooms[&self.world.room].walk;
        let Some(goal) = nearest_walkable(mask, w, h, x, y, 24) else {
            return;
        };
        if let RoutePlan::Path(points) = self.plan_route(goal.0, goal.1) {
            self.walk_dir = None;
            self.route = Some(Route {
                points,
                index: 0,
                stuck: 0,
            });
            self.route_is_effect = false;
        }
    }

    fn free_move(&mut self) {
        if self.forced_ticks > 0 {
            let dir = self.forced_dir;
            self.forced_ticks -= 1;
            self.step_dir(dir);
            return;
        }
        if let Some(dir) = self.walk_dir {
            self.route = None;
            self.step_dir(dir);
            return;
        }
        if self.route.is_some() {
            self.advance_route();
        }
    }

    fn step_dir(&mut self, dir: Dir) {
        let (dx, dy) = dir.delta();
        let (sx, sy) = if dx != 0 && dy != 0 {
            (
                dx * SPEED * DIAG_NUM / DIAG_DEN,
                dy * SPEED * DIAG_NUM / DIAG_DEN,
            )
        } else {
            (dx * SPEED, dy * SPEED)
        };
        self.world.facing = dir;
        self.try_move(sx, sy);
    }

    fn try_move(&mut self, dx: i32, dy: i32) {
        let nx = self.world.hero_x + dx;
        let ny = self.world.hero_y + dy;
        if self.walkable_fixed(nx, ny) {
            self.world.hero_x = nx;
            self.world.hero_y = ny;
            self.moved = true;
            return;
        }
        if dx != 0 && self.walkable_fixed(self.world.hero_x + dx, self.world.hero_y) {
            self.world.hero_x += dx;
            self.moved = true;
            return;
        }
        if dy != 0 && self.walkable_fixed(self.world.hero_x, self.world.hero_y + dy) {
            self.world.hero_y += dy;
            self.moved = true;
        }
    }

    fn advance_route(&mut self) {
        let Some(route) = self.route.as_ref() else {
            self.finish_route();
            return;
        };
        if route.index >= route.points.len() {
            self.finish_route();
            return;
        }
        let (tx, ty) = route.points[route.index];
        let target_x = tx * 256 + 128;
        let target_y = ty * 256 + 128;
        let dx = target_x - self.world.hero_x;
        let dy = target_y - self.world.hero_y;
        let mut route = Route {
            points: route.points.clone(),
            index: route.index,
            stuck: route.stuck,
        };
        if dx == 0 && dy == 0 {
            route.index += 1;
            route.stuck = 0;
            self.route = Some(route);
            if self
                .route
                .as_ref()
                .is_some_and(|r| r.index >= r.points.len())
            {
                self.finish_route();
            }
            return;
        }
        if let Some(dir) = Dir::from_delta(dx, dy) {
            self.world.facing = dir;
        }
        let dist = geom::isqrt(dx as i64 * dx as i64 + dy as i64 * dy as i64) as i32;
        let (sx, sy) = if dist <= SPEED {
            (dx, dy)
        } else {
            (dx * SPEED / dist.max(1), dy * SPEED / dist.max(1))
        };
        let ox = self.world.hero_x;
        let oy = self.world.hero_y;
        if dist <= SPEED && self.walkable_fixed(target_x, target_y) {
            self.world.hero_x = target_x;
            self.world.hero_y = target_y;
            self.moved = true;
        } else {
            self.try_move(sx, sy);
        }
        if self.world.hero_x == target_x && self.world.hero_y == target_y {
            route.index += 1;
            route.stuck = 0;
        } else if self.world.hero_x == ox && self.world.hero_y == oy {
            route.stuck += 1;
        } else {
            route.stuck = 0;
        }
        let stuck = route.stuck;
        let done = route.index >= route.points.len();
        self.route = Some(route);
        if stuck > 45 {
            self.fail_route();
        } else if done {
            self.finish_route();
        }
    }

    fn finish_route(&mut self) {
        let was_effect = self.route_is_effect;
        self.route = None;
        self.route_is_effect = false;
        if was_effect {
            self.active = Active::Idle;
            self.pump();
        }
        if self.pending.is_some() && self.queue.is_empty() && matches!(self.active, Active::Idle) {
            let parsed = self.pending.take().unwrap();
            self.finish_command(&parsed);
        }
    }

    fn fail_route(&mut self) {
        self.route = None;
        self.route_is_effect = false;
        self.pending = None;
        self.queue.clear();
        self.active = Active::Idle;
        self.force_say("You can't get there from here.".into());
    }

    fn plan_route(&self, x: i32, y: i32) -> RoutePlan {
        let w = SCREEN_W as i32;
        let h = SCREEN_H as i32;
        let mask = &self.pkg.rooms[&self.world.room].walk;
        let start = self.hero_px();
        let Some(goal) = nearest_walkable(mask, w, h, x, y, 16) else {
            return RoutePlan::Unreachable;
        };
        if start == goal {
            return RoutePlan::AlreadyThere;
        }
        match astar(mask, w, h, start, goal) {
            Some(mut path) => {
                if path.first().copied() == Some(start) {
                    path.remove(0);
                }
                if path.is_empty() {
                    RoutePlan::AlreadyThere
                } else {
                    RoutePlan::Path(path)
                }
            }
            None => RoutePlan::Unreachable,
        }
    }

    fn check_exits(&mut self) {
        let (px, py) = self.hero_px();
        let exits = self.pkg.rooms[&self.world.room].def.exits.clone();
        let inside = exits
            .into_iter()
            .find(|exit| point_in_rect(px, py, exit.trigger));
        if let Some(exit) = inside {
            if self.overlap.as_deref() == Some(exit.id.as_str()) {
                return;
            }
            self.overlap = Some(exit.id);
            if let Some(cond) = &exit.blocked_if {
                if self.reqs_ok(std::slice::from_ref(cond)) {
                    let text = exit
                        .blocked_text
                        .clone()
                        .unwrap_or_else(|| "You can't go that way.".into());
                    self.force_say(text);
                    return;
                }
            }
            let room = exit.to.clone();
            let spawn = exit.spawn.clone();
            if self.place(&room, &spawn).is_ok() {
                self.queue_on_enter();
                self.pump();
            }
        } else {
            self.overlap = None;
        }
    }

    fn walkable_fixed(&self, x: i32, y: i32) -> bool {
        self.walkable_px(x >> 8, y >> 8)
    }

    fn walkable_px(&self, x: i32, y: i32) -> bool {
        let mask = &self.pkg.rooms[&self.world.room].walk;
        in_mask(mask, SCREEN_W as i32, SCREEN_H as i32, x, y)
    }

    fn render(&mut self) {
        let left = self.pkg.rooms[&self.world.room].def.name.clone();
        let right = format!("Score: {} of {}", self.score(), self.score_max());
        let narrator = self.current_text.clone();
        let input = self.input_line.clone();
        let layers = if self.cutscene_covers() {
            Vec::new()
        } else {
            self.collect_layers()
        };
        let picture = self.backdrop();
        let priority = self.pkg.rooms[&self.world.room].priority.clone();
        render::composite(
            &mut self.fb,
            SCREEN_W as i32,
            SCREEN_H as i32,
            &picture,
            &priority,
            layers,
            &left,
            &right,
            narrator.as_deref(),
            &input,
        );
    }

    fn collect_layers(&self) -> Vec<Layer> {
        let room = &self.pkg.rooms[&self.world.room];
        let mut layers = Vec::new();
        for (prop, frame) in room.def.props.iter().zip(room.prop_frames.iter()) {
            if let Some(cond) = &prop.show_if {
                if !self.reqs_ok(std::slice::from_ref(cond)) {
                    continue;
                }
            }
            if !prop.req.is_empty() && !self.reqs_ok(&prop.req) {
                continue;
            }
            let dest_x = prop.pos[0];
            let dest_y = prop.pos[1];
            let foot_y = dest_y + frame.h as i32 - 1;
            let priority = prop
                .priority
                .unwrap_or_else(|| render::band_for_y(&room.def.bands, foot_y));
            layers.push(Layer {
                id: format!("prop:{}", prop.id),
                priority,
                sort_y: foot_y,
                dest_x,
                dest_y,
                sprite: frame.clone(),
                occlude: None,
            });
        }
        if let Some(layer) = self.hero_layer() {
            layers.push(layer);
        }
        for place in &room.def.actors {
            if let Some(layer) = self.actor_layer(place) {
                layers.push(layer);
            }
        }
        layers
    }

    fn hero_layer(&self) -> Option<Layer> {
        let (hidden, anim_override, _pos) = decode_pose(self.world.actors.get(&self.pkg.game.hero));
        if hidden {
            return None;
        }
        let forced = anim_override.filter(|name| name != "idle");
        let moving = forced.is_none()
            && (self.walk_dir.is_some() || self.route.is_some() || self.forced_ticks > 0);
        let (frame, _fps) = self.actor_frame(
            &self.pkg.game.hero,
            &self.world.costume,
            self.world.facing,
            moving,
            forced.as_deref(),
        )?;
        let (fx, fy) = self.hero_px();
        let room = &self.pkg.rooms[&self.world.room];
        let priority = render::band_for_y(&room.def.bands, fy);
        Some(Layer {
            id: "hero".into(),
            priority,
            sort_y: fy,
            dest_x: fx - frame.ax,
            dest_y: fy - frame.ay,
            sprite: frame,
            occlude: None,
        })
    }

    fn actor_layer(&self, place: &crate::model::ActorPlace) -> Option<Layer> {
        let (hidden, anim_override, pos_override) = decode_pose(self.world.actors.get(&place.id));
        if hidden {
            return None;
        }
        let facing = place.facing.unwrap_or(Dir::S);
        let costume = place.costume.clone().unwrap_or_else(|| "default".into());
        let state_anim = place
            .states
            .iter()
            .find(|state| state.if_conds.is_empty() || self.reqs_ok(&state.if_conds))
            .map(|state| state.anim.clone())
            .or_else(|| place.anim.clone());
        let anim = anim_override.or(state_anim);
        let (frame, _) =
            self.actor_frame(&place.actor, &costume, facing, false, anim.as_deref())?;
        let pos = pos_override.unwrap_or(place.pos);
        let room = &self.pkg.rooms[&self.world.room];
        let priority = place
            .priority_override
            .unwrap_or_else(|| render::band_for_y(&room.def.bands, pos[1]));
        let occlude = place
            .occlude
            .as_ref()
            .and_then(|name| room.occluders.get(name).cloned());
        Some(Layer {
            id: format!("actor:{}", place.id),
            priority,
            sort_y: pos[1],
            dest_x: pos[0] - frame.ax,
            dest_y: pos[1] - frame.ay,
            sprite: frame,
            occlude,
        })
    }

    fn actor_frame(
        &self,
        actor_id: &str,
        costume: &str,
        facing: Dir,
        moving: bool,
        forced_anim: Option<&str>,
    ) -> Option<(SpriteFrame, u32)> {
        let actor = self.pkg.actors.get(actor_id)?;
        let costume_frames = actor
            .frames
            .get(costume)
            .or_else(|| actor.frames.get("default"))?;
        let anims = &actor
            .def
            .costumes
            .get(costume)
            .or_else(|| actor.def.costumes.get("default"))?
            .animations;
        let kind = if moving { "walk" } else { "stand" };
        let mut names = Vec::new();
        if let Some(name) = forced_anim {
            names.push(name.to_string());
        }
        names.push(format!("{kind}_{}", facing.name()));
        names.push(format!("{kind}_{}", facing.cardinal().name()));
        names.push(format!("stand_{}", facing.cardinal().name()));
        names.push("stand_s".into());
        for name in names {
            if let Some(frames) = costume_frames.get(&name) {
                if frames.is_empty() {
                    continue;
                }
                let anim = anims.get(&name);
                let fps = anim.map(|a| a.fps.max(1)).unwrap_or(1);
                let looped = anim.map(|a| a.looped).unwrap_or(true);
                let tpf = (60 / fps).max(1);
                let index = (self.world.tick / tpf as u64) as usize;
                let index = if looped {
                    index % frames.len()
                } else {
                    index.min(frames.len() - 1)
                };
                return Some((frames[index].clone(), fps));
            }
        }
        None
    }

    fn remember_autosave(&mut self) {
        self.checkpoints.insert(
            "autosave".into(),
            Checkpoint {
                world: self.world.clone(),
                rng_state: self.rng.state(),
                rng_inc: self.rng.inc(),
            },
        );
    }

    fn hook_handles(&mut self, which: &str) -> bool {
        let name = match which {
            "early" => self.pkg.game.hooks.early.clone(),
            "command" => self.pkg.game.hooks.command.clone(),
            "enter" => self.pkg.game.hooks.enter.clone(),
            "cutscene_end" => self.pkg.game.hooks.cutscene_end.clone(),
            "cutscene_command" => self.pkg.game.hooks.cutscene_command.clone(),
            "after" => self.pkg.game.hooks.after.clone(),
            _ => None,
        };
        match name {
            Some(name) => self.hook_handles_named(&name),
            None => false,
        }
    }

    fn hook_handles_named(&mut self, name: &str) -> bool {
        let Some(script) = self.pkg.script.as_ref() else {
            return false;
        };
        if !script.has_fn(name) {
            return false;
        }
        let world = self.world.clone();
        let score = self.score();
        let result = script.call(
            &world,
            score,
            &self.hook_verb,
            &self.hook_noun,
            &self.hook_second,
            name,
        );
        match result {
            Ok(out) => {
                if !out.effects.is_empty() {
                    self.queue.extend(out.effects);
                }
                out.handled
            }
            Err(err) => {
                self.events.push(HostEvent::Warning {
                    message: err.clone(),
                });
                self.queue.push_back(Effect::Say { text: err });
                true
            }
        }
    }

    fn arm_timers(&mut self, verb: &str) -> bool {
        if self.pkg.vocab.verb_meta.get(verb).copied().unwrap_or(false) {
            return false;
        }
        let timers = self.pkg.game.timers.clone();
        for timer in timers {
            if timer.skip_verbs.iter().any(|skip| skip == verb) {
                continue;
            }
            if !self.reqs_ok(&timer.when) {
                continue;
            }
            let current = self.world.vars.get(&timer.var).copied().unwrap_or(0);
            if current + 1 > timer.limit {
                self.queue.extend(timer.effects);
                self.pump();
                return true;
            }
            self.world.vars.insert(timer.var, current + 1);
        }
        false
    }

    fn cutscene_command(&mut self, parsed: &Parsed) {
        let id = self.world.cutscene.clone().unwrap_or_default();
        let rules = self
            .pkg
            .cutscenes
            .get(&id)
            .map(|cut| cut.def.on_command.clone())
            .unwrap_or_default();
        if let Some(rule) = rules.into_iter().find(|rule| self.rule_ok(rule, parsed)) {
            self.queue.extend(rule.effects);
            self.pump();
            return;
        }
        if self.hook_handles("cutscene_command") {
            self.pump();
        }
    }

    fn enter_cut_beat(&mut self) {
        let Some(id) = self.world.cutscene.clone() else {
            return;
        };
        let Some(cut) = self.pkg.cutscenes.get(&id) else {
            return;
        };
        if cut.def.beats.is_empty() {
            if cut.frames.is_empty() {
                self.end_cutscene();
                return;
            }
            self.world.cut_frame = 0;
            let ticks = cut.def.frame_ticks.first().copied().unwrap_or(8).max(1);
            self.queue.push_front(Effect::Wait { ticks });
            return;
        }
        let index = self.world.cut_beat as usize;
        if index >= cut.def.beats.len() {
            self.end_cutscene();
            return;
        }
        let beat = cut.def.beats[index].clone();
        self.world.cut_frame = beat.flip_from.unwrap_or(beat.frame);
        self.world.cut_flip = 0;
        let mut front = Vec::new();
        front.extend(beat.effects);
        for line in beat.say {
            front.push(Effect::Say { text: line });
        }
        for effect in front.into_iter().rev() {
            self.queue.push_front(effect);
        }
    }

    fn advance_cutscene_if_idle(&mut self) {
        if self.world.cutscene.is_none() {
            return;
        }
        if !self.queue.is_empty() || !matches!(self.active, Active::Idle) {
            return;
        }
        if self.autoplay_cutscene() {
            return;
        }
        self.world.cut_beat = self.world.cut_beat.saturating_add(1);
        self.enter_cut_beat();
        self.pump();
    }

    fn autoplay_cutscene(&self) -> bool {
        let Some(id) = &self.world.cutscene else {
            return false;
        };
        self.pkg
            .cutscenes
            .get(id)
            .is_some_and(|cut| cut.def.beats.is_empty() && !cut.frames.is_empty())
    }

    fn advance_autoplay(&mut self) {
        let Some(id) = self.world.cutscene.clone() else {
            return;
        };
        let Some(cut) = self.pkg.cutscenes.get(&id) else {
            return;
        };
        let next = self.world.cut_frame.saturating_add(1);
        if next >= cut.frames.len() as u32 {
            self.end_cutscene();
            return;
        }
        self.world.cut_frame = next;
        let ticks = cut
            .def
            .frame_ticks
            .get(next as usize)
            .copied()
            .unwrap_or(8)
            .max(1);
        self.queue.push_front(Effect::Wait { ticks });
    }

    fn end_cutscene(&mut self) {
        let id = self.world.cutscene.take();
        self.world.cut_beat = 0;
        self.world.cut_frame = 0;
        self.world.cut_flip = 0;
        if let Some(id) = id {
            if let Some(cut) = self.pkg.cutscenes.get(&id) {
                let effects = cut.def.on_end.clone();
                self.queue.extend(effects);
            }
        }
        if let Some(name) = self.pkg.game.hooks.cutscene_end.clone() {
            let _ = self.hook_handles_named(&name);
        }
    }

    fn tick_flipbook(&mut self) {
        let Some(id) = self.world.cutscene.clone() else {
            return;
        };
        let Some(cut) = self.pkg.cutscenes.get(&id) else {
            return;
        };
        let Some(beat) = cut.def.beats.get(self.world.cut_beat as usize) else {
            return;
        };
        let (Some(from), Some(to)) = (beat.flip_from, beat.flip_to) else {
            return;
        };
        let ticks = beat.flip_ticks.max(1);
        self.world.cut_flip = self.world.cut_flip.saturating_add(1);
        if self.world.cut_flip < ticks {
            return;
        }
        self.world.cut_flip = 0;
        let span = to.saturating_sub(from).saturating_add(1).max(1);
        let step = self.world.cut_frame.saturating_sub(from).saturating_add(1) % span;
        self.world.cut_frame = from + step;
    }

    fn cutscene_covers(&self) -> bool {
        let Some(id) = &self.world.cutscene else {
            return false;
        };
        self.pkg
            .cutscenes
            .get(id)
            .is_some_and(|cut| !cut.frames.is_empty())
    }

    fn backdrop(&self) -> crate::imageutil::RgbaImage {
        if let Some(id) = &self.world.cutscene {
            if let Some(cut) = self.pkg.cutscenes.get(id) {
                if !cut.frames.is_empty() {
                    let index = (self.world.cut_frame as usize).min(cut.frames.len() - 1);
                    return cut.frames[index].clone();
                }
            }
        }
        let room = &self.pkg.rooms[&self.world.room];
        if !self.world.picture.is_empty() {
            if let Some(image) = room.pictures.get(&self.world.picture) {
                return image.clone();
            }
        }
        room.picture.clone()
    }
}

fn pose_token(hidden: bool, anim: Option<String>, x: Option<i32>, y: Option<i32>) -> String {
    if hidden {
        return "hidden".into();
    }
    match (anim, x, y) {
        (Some(anim), Some(x), Some(y)) => format!("anim:{anim}@{x},{y}"),
        (Some(anim), _, _) => format!("anim:{anim}"),
        (None, Some(x), Some(y)) => format!("at:{x},{y}"),
        _ => String::new(),
    }
}

fn decode_pose(raw: Option<&String>) -> (bool, Option<String>, Option<[i32; 2]>) {
    let Some(raw) = raw else {
        return (false, None, None);
    };
    if raw == "hidden" {
        return (true, None, None);
    }
    if let Some(rest) = raw.strip_prefix("at:") {
        return (false, None, parse_pair(rest));
    }
    if let Some(rest) = raw.strip_prefix("anim:") {
        if let Some((anim, at)) = rest.split_once('@') {
            return (false, Some(anim.to_string()), parse_pair(at));
        }
        return (false, Some(rest.to_string()), None);
    }
    (false, None, None)
}

fn parse_pair(text: &str) -> Option<[i32; 2]> {
    let (x, y) = text.split_once(',')?;
    Some([x.parse().ok()?, y.parse().ok()?])
}

fn specificity(rule: &Rule) -> u8 {
    let mut score = 1u8;
    if rule.noun.is_some() && !rule.any_noun {
        score += 2;
    }
    if rule.second.is_some() && !rule.any_second {
        score += 4;
    }
    score
}

fn opt_eq(want: &Option<String>, got: &Option<String>) -> bool {
    match want {
        None => got.is_none(),
        Some(value) => got.as_ref() == Some(value),
    }
}

fn arrow_dir(key: &str) -> Option<Dir> {
    match key {
        "ArrowUp" | "n" | "N" => Some(Dir::N),
        "ArrowDown" | "s" | "S" => Some(Dir::S),
        "ArrowLeft" | "w" | "W" => Some(Dir::W),
        "ArrowRight" | "e" | "E" => Some(Dir::E),
        "ne" => Some(Dir::Ne),
        "nw" => Some(Dir::Nw),
        "se" => Some(Dir::Se),
        "sw" => Some(Dir::Sw),
        _ => None,
    }
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0xf) as usize] as char);
    }
    out
}
