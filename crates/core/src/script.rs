//! Sandboxed Rhai hooks. Scripts read a snapshot of the world and return effects.
//! They do not keep their own state: the next call sees only what the engine applied.

use crate::model::{Dir, Effect};
use crate::save::World;
use rhai::{Engine as RhaiEngine, AST};
use std::cell::RefCell;
use std::rc::Rc;

pub struct Script {
    ast: AST,
}

impl std::fmt::Debug for Script {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Script")
    }
}

pub struct ScriptResult {
    pub handled: bool,
    pub effects: Vec<Effect>,
}

impl Script {
    pub fn compile(source: &str) -> Result<Self, String> {
        let mut engine = RhaiEngine::new();
        limits(&mut engine);
        let ast = engine
            .compile(source)
            .map_err(|err| format!("script: {err}"))?;
        Ok(Self { ast })
    }

    pub fn has_fn(&self, name: &str) -> bool {
        self.ast.iter_functions().any(|func| func.name == name)
    }

    pub fn call(
        &self,
        world: &World,
        score_now: i32,
        verb: &str,
        noun: &str,
        second: &str,
        name: &str,
    ) -> Result<ScriptResult, String> {
        if !self.has_fn(name) {
            return Ok(ScriptResult {
                handled: false,
                effects: Vec::new(),
            });
        }
        let effects = Rc::new(RefCell::new(Vec::<Effect>::new()));
        let handled = Rc::new(RefCell::new(false));
        let mut engine = RhaiEngine::new();
        limits(&mut engine);
        register(
            &mut engine,
            world,
            score_now,
            verb,
            noun,
            second,
            &effects,
            &handled,
        );
        let mut scope = rhai::Scope::new();
        engine
            .call_fn::<()>(&mut scope, &self.ast, name, ())
            .map_err(|err| format!("script {name}: {err}"))?;
        let handled_flag = *handled.borrow();
        let effects_out = effects.borrow().clone();
        Ok(ScriptResult {
            handled: handled_flag,
            effects: effects_out,
        })
    }
}

fn limits(engine: &mut RhaiEngine) {
    engine.set_max_operations(80_000);
    engine.set_max_call_levels(32);
    engine.set_max_expr_depths(64, 32);
    engine.set_max_string_size(8_000);
    engine.set_max_array_size(1_000);
    engine.set_max_map_size(200);
}

#[allow(clippy::too_many_arguments)]
fn register(
    engine: &mut RhaiEngine,
    world: &World,
    score_now: i32,
    verb: &str,
    noun: &str,
    second: &str,
    effects: &Rc<RefCell<Vec<Effect>>>,
    handled: &Rc<RefCell<bool>>,
) {
    let room = world.room.clone();
    let flags = world.flags.clone();
    let vars = world.vars.clone();
    let inventory = world.inventory.clone();
    let hero_x = world.hero_x >> 8;
    let hero_y = world.hero_y >> 8;
    let verb = verb.to_string();
    let noun = noun.to_string();
    let second = second.to_string();

    engine.register_fn("room", move || room.clone());
    engine.register_fn("verb", move || verb.clone());
    engine.register_fn("noun", move || noun.clone());
    engine.register_fn("second", move || second.clone());
    engine.register_fn("hero_x", move || hero_x);
    engine.register_fn("hero_y", move || hero_y);
    engine.register_fn("score_now", move || i64::from(score_now));

    let flags_read = flags.clone();
    engine.register_fn("flag", move |name: String| flags_read.contains(&name));
    let vars_read = vars.clone();
    engine.register_fn("num", move |name: String| {
        i64::from(vars_read.get(&name).copied().unwrap_or(0))
    });
    let inv_read = inventory.clone();
    engine.register_fn("count", move |id: String| {
        i64::from(inv_read.get(&id).copied().unwrap_or(0))
    });
    let inv_has = inventory;
    engine.register_fn("has", move |id: String| {
        inv_has.get(&id).copied().unwrap_or(0) > 0
    });

    let fx = effects.clone();
    let handled_flag = Rc::clone(handled);
    engine.register_fn("handled", move || {
        *handled_flag.borrow_mut() = true;
    });
    engine.register_fn("say", move |text: String| {
        fx.borrow_mut().push(Effect::Say { text });
    });

    let fx = effects.clone();
    engine.register_fn("take_item", move |id: String| {
        fx.borrow_mut().push(Effect::Take { id });
    });
    let fx = effects.clone();
    engine.register_fn("give_item", move |id: String| {
        fx.borrow_mut().push(Effect::Give { id });
    });
    let fx = effects.clone();
    engine.register_fn("set_flag", move |flag: String| {
        fx.borrow_mut().push(Effect::Set { flag });
    });
    let fx = effects.clone();
    engine.register_fn("clear_flag", move |flag: String| {
        fx.borrow_mut().push(Effect::Clear { flag });
    });
    let fx = effects.clone();
    engine.register_fn("set_var", move |var: String, value: i64| {
        fx.borrow_mut().push(Effect::SetVar {
            var,
            value: clamp_i32(value),
        });
    });
    let fx = effects.clone();
    engine.register_fn("inc_var", move |var: String, by: i64| {
        fx.borrow_mut().push(Effect::Inc {
            var,
            by: clamp_i32(by),
        });
    });
    let fx = effects.clone();
    engine.register_fn("score", move |id: String, points: i64| {
        fx.borrow_mut().push(Effect::Score {
            id,
            points: clamp_i32(points),
        });
    });
    let fx = effects.clone();
    engine.register_fn("goto_room", move |room: String, spawn: String| {
        fx.borrow_mut().push(Effect::Goto { room, spawn });
    });
    let fx = effects.clone();
    engine.register_fn("die", move |text: String| {
        fx.borrow_mut().push(Effect::Say { text });
        fx.borrow_mut().push(Effect::SetDead { restore_to: None });
    });
    let fx = effects.clone();
    engine.register_fn("die_to", move |text: String, checkpoint: String| {
        fx.borrow_mut().push(Effect::Say { text });
        fx.borrow_mut().push(Effect::SetDead {
            restore_to: Some(checkpoint),
        });
    });
    let fx = effects.clone();
    engine.register_fn("finish", move |id: String| {
        fx.borrow_mut().push(Effect::SetEnding { id });
    });
    let fx = effects.clone();
    engine.register_fn("checkpoint", move |name: String| {
        fx.borrow_mut().push(Effect::Checkpoint { name });
    });
    let fx = effects.clone();
    engine.register_fn("keep_autosave", move || {
        fx.borrow_mut().push(Effect::KeepAutosave);
    });
    let fx = effects.clone();
    engine.register_fn("sfx", move |id: String| {
        fx.borrow_mut().push(Effect::Sfx { id });
    });
    let fx = effects.clone();
    engine.register_fn("face", move |dir: String| {
        if let Some(dir) = Dir::from_name(&dir) {
            fx.borrow_mut().push(Effect::Face { dir });
        }
    });
    let fx = effects.clone();
    engine.register_fn("costume", move |id: String| {
        fx.borrow_mut().push(Effect::Costume { id });
    });
    let fx = effects.clone();
    engine.register_fn("teleport", move |x: i64, y: i64| {
        fx.borrow_mut().push(Effect::Teleport {
            x: clamp_i32(x),
            y: clamp_i32(y),
        });
    });
    let fx = effects.clone();
    engine.register_fn("hold", move |locked: bool| {
        fx.borrow_mut().push(Effect::Hold { locked });
    });
    let fx = effects.clone();
    engine.register_fn("picture", move |name: String| {
        fx.borrow_mut().push(Effect::Picture { name });
    });
    let fx = effects.clone();
    engine.register_fn("pose", move |actor: String, anim: String| {
        fx.borrow_mut().push(Effect::Pose {
            actor,
            anim: Some(anim),
            x: None,
            y: None,
            hidden: false,
        });
    });
    let fx = effects.clone();
    engine.register_fn(
        "pose_at",
        move |actor: String, anim: String, x: i64, y: i64| {
            fx.borrow_mut().push(Effect::Pose {
                actor,
                anim: Some(anim),
                x: Some(clamp_i32(x)),
                y: Some(clamp_i32(y)),
                hidden: false,
            });
        },
    );
    let fx = effects.clone();
    engine.register_fn("hide_actor", move |actor: String| {
        fx.borrow_mut().push(Effect::Pose {
            actor,
            anim: None,
            x: None,
            y: None,
            hidden: true,
        });
    });
    let fx = effects.clone();
    engine.register_fn("play_cutscene", move |id: String| {
        fx.borrow_mut().push(Effect::Cutscene { id });
    });
    let fx = effects.clone();
    engine.register_fn("mark_dead", move |checkpoint: String| {
        let restore_to = if checkpoint.is_empty() {
            None
        } else {
            Some(checkpoint)
        };
        fx.borrow_mut().push(Effect::SetDead { restore_to });
    });
    let scored = world.scored.clone();
    engine.register_fn("already", move |id: String| scored.contains(&id));
}

fn clamp_i32(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}
