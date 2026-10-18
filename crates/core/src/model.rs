use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Phase 1 picture size. `game.json` is read and must match this.
pub const SCREEN_W: u32 = 384;
pub const SCREEN_H: u32 = 216;

/// Package major this core accepts. Newer minors warn; other majors refuse.
pub const FORMAT_MAJOR: u32 = 0;
pub const FORMAT_MINOR: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dir {
    N,
    Ne,
    E,
    Se,
    S,
    Sw,
    W,
    Nw,
}

impl Dir {
    pub fn name(self) -> &'static str {
        match self {
            Dir::N => "n",
            Dir::Ne => "ne",
            Dir::E => "e",
            Dir::Se => "se",
            Dir::S => "s",
            Dir::Sw => "sw",
            Dir::W => "w",
            Dir::Nw => "nw",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "n" | "north" => Dir::N,
            "ne" => Dir::Ne,
            "e" | "east" => Dir::E,
            "se" => Dir::Se,
            "s" | "south" => Dir::S,
            "sw" => Dir::Sw,
            "w" | "west" => Dir::W,
            "nw" => Dir::Nw,
            _ => return None,
        })
    }

    pub fn delta(self) -> (i32, i32) {
        match self {
            Dir::N => (0, -1),
            Dir::Ne => (1, -1),
            Dir::E => (1, 0),
            Dir::Se => (1, 1),
            Dir::S => (0, 1),
            Dir::Sw => (-1, 1),
            Dir::W => (-1, 0),
            Dir::Nw => (-1, -1),
        }
    }

    /// Sprite fallback when an 8-way animation is missing.
    pub fn cardinal(self) -> Self {
        match self {
            Dir::Ne | Dir::Se => Dir::E,
            Dir::Nw | Dir::Sw => Dir::W,
            other => other,
        }
    }

    pub fn from_delta(dx: i32, dy: i32) -> Option<Self> {
        let sx = dx.signum();
        let sy = dy.signum();
        match (sx, sy) {
            (0, -1) => Some(Dir::N),
            (1, -1) => Some(Dir::Ne),
            (1, 0) => Some(Dir::E),
            (1, 1) => Some(Dir::Se),
            (0, 1) => Some(Dir::S),
            (-1, 1) => Some(Dir::Sw),
            (-1, 0) => Some(Dir::W),
            (-1, -1) => Some(Dir::Nw),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameDef {
    pub id: String,
    pub title: String,
    pub version: String,
    #[serde(default = "default_format")]
    pub format_version: String,
    #[serde(default = "default_engine")]
    pub engine_min: String,
    #[serde(default = "default_resolution")]
    pub resolution: [u32; 2],
    pub start_room: String,
    #[serde(default = "default_spawn")]
    pub start_spawn: String,
    pub hero: String,
    #[serde(default)]
    pub score_max: i32,
    #[serde(default = "default_seed")]
    pub rng_seed: u64,
    #[serde(default = "default_reach")]
    pub reach: i32,
    #[serde(default)]
    pub intro: Vec<String>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub hooks: Hooks,
    #[serde(default)]
    pub timers: Vec<MoveTimer>,
    #[serde(default)]
    pub declarations: Declarations,
    /// Items the hero is holding when the game boots, before the first room line.
    #[serde(default)]
    pub carrying: Vec<String>,
    /// While dead, any command but restore repeats this. Restart and quit stay available.
    #[serde(default)]
    pub death_prompt: Option<String>,
}

fn default_format() -> String {
    "0.1.0".into()
}
fn default_engine() -> String {
    "0.1.0".into()
}
fn default_resolution() -> [u32; 2] {
    [SCREEN_W, SCREEN_H]
}
fn default_spawn() -> String {
    "default".into()
}
fn default_seed() -> u64 {
    1
}
fn default_reach() -> i32 {
    42
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Hooks {
    /// Runs before move timers. Handling the command skips the timers and the rules.
    #[serde(default)]
    pub early: Option<String>,
    /// Runs after move timers and after a reach walk. Handling skips the rule list.
    #[serde(default)]
    pub command: Option<String>,
    /// Runs when the hero arrives, after the room's `onEnter` rules.
    #[serde(default)]
    pub enter: Option<String>,
    /// Runs when a cutscene's last beat finishes.
    #[serde(default)]
    pub cutscene_end: Option<String>,
    /// Runs for a typed command while a cutscene is up.
    #[serde(default)]
    pub cutscene_command: Option<String>,
    /// Runs after the command's own effects, still before the player dismisses them.
    #[serde(default)]
    pub after: Option<String>,
}

/// Counts non-skipped commands while `when` holds. The command that would pass
/// `limit` is replaced by `effects` and does not run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveTimer {
    pub id: String,
    #[serde(default)]
    pub when: Vec<String>,
    pub limit: i32,
    pub var: String,
    #[serde(default)]
    pub skip_verbs: Vec<String>,
    #[serde(default)]
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreDecl {
    pub id: String,
    pub points: i32,
}

/// Flags, vars, score ids, and endings that scripts set and the save checker allows.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Declarations {
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default)]
    pub vars: Vec<String>,
    #[serde(default)]
    pub scores: Vec<ScoreDecl>,
    #[serde(default)]
    pub endings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CutsceneBeat {
    #[serde(default)]
    pub say: Vec<String>,
    #[serde(default)]
    pub frame: u32,
    /// Inclusive flipbook range played while this beat's narration is up.
    #[serde(default)]
    pub flip_from: Option<u32>,
    #[serde(default)]
    pub flip_to: Option<u32>,
    #[serde(default)]
    pub flip_ticks: u32,
    #[serde(default)]
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CutsceneDef {
    pub id: String,
    #[serde(default)]
    pub beats: Vec<CutsceneBeat>,
    /// How many ticks each flipbook frame stays up when the cutscene autoplays.
    #[serde(default)]
    pub frame_ticks: Vec<u32>,
    #[serde(default)]
    pub on_command: Vec<Rule>,
    #[serde(default)]
    pub on_end: Vec<Effect>,
    #[serde(default)]
    pub frame_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Phrase {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rewrite {
    pub pattern: String,
    pub replace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerbDef {
    pub id: String,
    pub words: Vec<String>,
    #[serde(default)]
    pub ranged: bool,
    #[serde(default)]
    pub meta: bool,
    #[serde(default, rename = "default")]
    pub default_reply: Option<String>,
    /// Room id → noun whose stand point this verb walks to when the sentence has none.
    #[serde(default)]
    pub targets: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NounDef {
    pub id: String,
    pub words: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Vocab {
    #[serde(default)]
    pub ignore: Vec<String>,
    #[serde(default)]
    pub prepositions: Vec<String>,
    #[serde(default)]
    pub phrases: Vec<Phrase>,
    #[serde(default)]
    pub rewrites: Vec<Rewrite>,
    #[serde(default)]
    pub verbs: Vec<VerbDef>,
    #[serde(default)]
    pub nouns: Vec<NounDef>,
    /// room id → word → replacement word (then looked up as a noun).
    #[serde(default)]
    pub rooms: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(default = "default_unknown_word")]
    pub unknown_word: String,
    #[serde(default = "default_unknown_command")]
    pub unknown_command: String,
    /// Verb id → noun id used when the sentence has no noun. `go` becomes `go out`.
    #[serde(default)]
    pub bare: BTreeMap<String, String>,
}

fn default_unknown_word() -> String {
    "I don't know the word '{word}'.".into()
}
fn default_unknown_command() -> String {
    "I don't understand that.".into()
}

impl Default for Vocab {
    fn default() -> Self {
        Self {
            ignore: Vec::new(),
            prepositions: Vec::new(),
            phrases: Vec::new(),
            rewrites: Vec::new(),
            verbs: Vec::new(),
            nouns: Vec::new(),
            rooms: BTreeMap::new(),
            unknown_word: default_unknown_word(),
            unknown_command: default_unknown_command(),
            bare: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub plural: String,
    #[serde(default)]
    pub look: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "do", rename_all = "camelCase")]
pub enum Effect {
    Say {
        text: String,
    },
    Take {
        id: String,
    },
    Give {
        id: String,
    },
    Set {
        flag: String,
    },
    Clear {
        flag: String,
    },
    Inc {
        var: String,
        #[serde(default = "one_i32")]
        by: i32,
    },
    SetVar {
        var: String,
        value: i32,
    },
    Score {
        id: String,
        points: i32,
    },
    Goto {
        room: String,
        spawn: String,
    },
    SetDead {
        #[serde(default, rename = "restoreTo", skip_serializing_if = "Option::is_none")]
        restore_to: Option<String>,
    },
    SetEnding {
        id: String,
    },
    Checkpoint {
        name: String,
    },
    WalkTo {
        x: i32,
        y: i32,
    },
    Wait {
        ticks: u32,
    },
    Sfx {
        id: String,
    },
    Face {
        dir: Dir,
    },
    MarkOnce {
        id: String,
    },
    /// Run a Rhai function. It may queue more effects and mark the command handled.
    Call {
        name: String,
    },
    Cutscene {
        id: String,
    },
    Costume {
        id: String,
    },
    Teleport {
        x: i32,
        y: i32,
    },
    /// Lock or unlock free walking. A locked hero still plays scripted `walkTo` effects.
    Hold {
        locked: bool,
    },
    /// Select a room picture variant. An empty name restores the room plate.
    Picture {
        name: String,
    },
    /// Keep the current autosave. Later room changes do not replace it.
    /// Charlie uses this after the cab, so a lake death returns to that arrival.
    KeepAutosave,
    /// Override a placed actor. `hidden` removes them. Empty `anim` keeps the state machine.
    Pose {
        actor: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        anim: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        x: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        y: Option<i32>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        hidden: bool,
    },
}

fn one_i32() -> i32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: String,
    #[serde(default)]
    pub verb: Option<String>,
    #[serde(default)]
    pub noun: Option<String>,
    #[serde(default)]
    pub second: Option<String>,
    /// Match whatever the second noun is, including none.
    #[serde(default)]
    pub any_second: bool,
    /// Match whatever the noun is, including none.
    #[serde(default)]
    pub any_noun: bool,
    #[serde(default)]
    pub req: Vec<String>,
    #[serde(default)]
    pub once: bool,
    #[serde(default)]
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Band {
    pub y: i32,
    pub priority: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitDef {
    pub id: String,
    pub trigger: [i32; 4],
    pub to: String,
    pub spawn: String,
    #[serde(default)]
    pub blocked_if: Option<String>,
    #[serde(default)]
    pub blocked_text: Option<String>,
    #[serde(default)]
    pub approach: Option<[i32; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotspotDef {
    pub id: String,
    pub rect: [i32; 4],
    #[serde(default)]
    pub nouns: Vec<String>,
    #[serde(default)]
    pub look: String,
    #[serde(default)]
    pub stand: Option<[i32; 2]>,
    #[serde(default)]
    pub face: Option<Dir>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameDef {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub ax: i32,
    pub ay: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropDef {
    pub id: String,
    pub pos: [i32; 2],
    #[serde(default)]
    pub show_if: Option<String>,
    #[serde(default)]
    pub priority: Option<u8>,
    pub frame: FrameDef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorState {
    #[serde(rename = "if", default)]
    pub if_conds: Vec<String>,
    pub anim: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActorPlace {
    pub id: String,
    pub actor: String,
    pub pos: [i32; 2],
    #[serde(default)]
    pub facing: Option<Dir>,
    #[serde(default)]
    pub costume: Option<String>,
    #[serde(default)]
    pub anim: Option<String>,
    #[serde(default)]
    pub priority_override: Option<u8>,
    #[serde(default)]
    pub states: Vec<ActorState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub look: String,
    #[serde(default)]
    pub picture: String,
    #[serde(default)]
    pub spawns: BTreeMap<String, [i32; 2]>,
    #[serde(default)]
    pub exits: Vec<ExitDef>,
    #[serde(default)]
    pub hotspots: Vec<HotspotDef>,
    #[serde(default)]
    pub actors: Vec<ActorPlace>,
    #[serde(default)]
    pub props: Vec<PropDef>,
    #[serde(default)]
    pub on_enter: Vec<Rule>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub bands: Vec<Band>,
    /// Extra full-screen plates, selected by the `picture` effect.
    #[serde(default)]
    pub pictures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Animation {
    #[serde(default = "default_fps")]
    pub fps: u32,
    #[serde(rename = "loop", default = "yes")]
    pub looped: bool,
    pub frames: Vec<FrameDef>,
}

fn default_fps() -> u32 {
    1
}
fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Costume {
    pub animations: BTreeMap<String, Animation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorDef {
    pub id: String,
    #[serde(default)]
    pub costumes: BTreeMap<String, Costume>,
}

/// Source walk polygon. Not stored in the package; the packer rasterises it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Polygon {
    pub points: Vec<[i32; 2]>,
    #[serde(default)]
    pub holes: Vec<Vec<[i32; 2]>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalkDef {
    pub polygons: Vec<Polygon>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityDef {
    pub bands: Vec<Band>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
}

impl CmpOp {
    pub fn eval(self, left: i32, right: i32) -> bool {
        match self {
            CmpOp::Eq => left == right,
            CmpOp::Ne => left != right,
            CmpOp::Gt => left > right,
            CmpOp::Ge => left >= right,
            CmpOp::Lt => left < right,
            CmpOp::Le => left <= right,
        }
    }
}
