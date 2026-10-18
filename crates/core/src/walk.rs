use crate::engine::Engine;
use crate::model::Dir;

#[derive(Debug)]
pub struct WalkError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for WalkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

pub trait WalkHost {
    fn snapshot(&mut self, name: &str, rgba: &[u8], w: u32, h: u32) -> Result<(), String>;
    fn save_slot(&mut self, name: &str, data: &[u8]) -> Result<(), String>;
    fn load_slot(&mut self, name: &str) -> Result<Vec<u8>, String>;
}

pub fn run_walk(
    engine: &mut Engine,
    script: &str,
    host: &mut dyn WalkHost,
) -> Result<(), WalkError> {
    for (idx, raw) in script.lines().enumerate() {
        let line_no = idx + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix('@') {
            at_command(engine, host, line_no, rest.trim())?;
        } else {
            if engine.is_locked() {
                return Err(fail(line_no, "input is locked; @dismiss first"));
            }
            engine.submit_line(line);
            pump_route(engine, line_no)?;
        }
    }
    Ok(())
}

fn at_command(
    engine: &mut Engine,
    host: &mut dyn WalkHost,
    line_no: usize,
    rest: &str,
) -> Result<(), WalkError> {
    let (cmd, args) = split_cmd(rest);
    match cmd {
        "expect" => expect(engine, line_no, args),
        "walk" => walk_dir(engine, line_no, args),
        "walkto" => walk_to(engine, line_no, args),
        "wait" => {
            let ticks: u32 = args
                .parse()
                .map_err(|_| fail(line_no, "usage: @wait ticks"))?;
            engine.step(ticks);
            Ok(())
        }
        "dismiss" => engine.dismiss_narration().map_err(|msg| fail(line_no, msg)),
        "interrupt" => {
            if args.is_empty() {
                return Err(fail(line_no, "usage: @interrupt command"));
            }
            engine.interrupt_line(args);
            pump_route(engine, line_no)?;
            Ok(())
        }
        "snapshot" => {
            if args.is_empty() || args.split_whitespace().count() != 1 {
                return Err(fail(line_no, "usage: @snapshot name"));
            }
            host.snapshot(args, engine.framebuffer(), engine.width(), engine.height())
                .map_err(|msg| fail(line_no, msg))
        }
        "save" => {
            if args.is_empty() {
                return Err(fail(line_no, "usage: @save slot"));
            }
            let bytes = engine
                .save()
                .map_err(|err| fail(line_no, err.to_string()))?;
            host.save_slot(args, &bytes)
                .map_err(|msg| fail(line_no, msg))
        }
        "restore" => {
            if args.is_empty() {
                let restored = if engine.is_dead() {
                    engine.restore_after_death()
                } else {
                    engine.restore_last_checkpoint()
                };
                return restored.map_err(|err| fail(line_no, err.to_string()));
            }
            let bytes = host.load_slot(args).map_err(|msg| fail(line_no, msg))?;
            engine
                .restore(&bytes)
                .map_err(|err| fail(line_no, err.to_string()))
        }
        "tap" => {
            let (x, y) = parse_xy(args).map_err(|msg| fail(line_no, msg))?;
            engine.input(crate::engine::Input::Tap { x, y });
            let mut ticks = 0;
            while engine.route_active() {
                engine.step(1);
                ticks += 1;
                if ticks > 1800 {
                    return Err(fail(line_no, "tap walk timed out"));
                }
            }
            Ok(())
        }
        other => Err(fail(line_no, format!("unknown command @{other}"))),
    }
}

fn expect(engine: &Engine, line_no: usize, args: &str) -> Result<(), WalkError> {
    let mut parts = args.split_whitespace();
    let kind = parts
        .next()
        .ok_or_else(|| fail(line_no, "usage: @expect ..."))?;
    let fail_got = |message: String| Err(fail(line_no, message));
    match kind {
        "room" => {
            let id = parts
                .next()
                .ok_or_else(|| fail(line_no, "usage: @expect room id"))?;
            if engine.room_id() != id {
                return fail_got(format!("expected room {id}, in {}", engine.room_id()));
            }
        }
        "flag" => {
            let name = parts
                .next()
                .ok_or_else(|| fail(line_no, "usage: @expect flag name"))?;
            if !engine.has_flag(name) {
                return fail_got(format!("flag {name} is not set"));
            }
        }
        "!flag" => {
            let name = parts
                .next()
                .ok_or_else(|| fail(line_no, "usage: @expect !flag name"))?;
            if engine.has_flag(name) {
                return fail_got(format!("flag {name} is set"));
            }
        }
        "var" => {
            let name = parts
                .next()
                .ok_or_else(|| fail(line_no, "usage: @expect var name value"))?;
            let value: i32 = parts
                .next()
                .ok_or_else(|| fail(line_no, "usage: @expect var name value"))?
                .parse()
                .map_err(|_| fail(line_no, "var value must be an integer"))?;
            if engine.var(name) != value {
                return fail_got(format!(
                    "var {name} is {}, expected {value}",
                    engine.var(name)
                ));
            }
        }
        "item" => {
            let id = parts
                .next()
                .ok_or_else(|| fail(line_no, "usage: @expect item id"))?;
            match parts.next() {
                None => {
                    if engine.item_count(id) < 1 {
                        return fail_got(format!("not carrying {id}"));
                    }
                }
                Some(text) => {
                    let expected: i32 = text
                        .parse()
                        .map_err(|_| fail(line_no, "item count must be an integer"))?;
                    let got = engine.item_count(id);
                    if got != expected {
                        return fail_got(format!("item {id} count is {got}, expected {expected}"));
                    }
                }
            }
        }
        "score" => {
            let value: i32 = parts
                .next()
                .ok_or_else(|| fail(line_no, "usage: @expect score n"))?
                .parse()
                .map_err(|_| fail(line_no, "score must be an integer"))?;
            if engine.score() != value {
                return fail_got(format!("score is {}, expected {value}", engine.score()));
            }
        }
        "text" => {
            let needle = args
                .strip_prefix("text")
                .unwrap_or(args)
                .trim()
                .to_lowercase();
            if needle.is_empty() {
                return Err(fail(line_no, "usage: @expect text ..."));
            }
            let hay = engine.said().join("\n").to_lowercase();
            if !hay.contains(&needle) {
                return fail_got(format!("narration does not contain '{needle}'"));
            }
        }
        "death" => {
            if !engine.is_dead() {
                return fail_got("expected to be dead".into());
            }
        }
        "alive" => {
            if engine.is_dead() {
                return fail_got("expected to be alive".into());
            }
        }
        "ending" => {
            let id = parts
                .next()
                .ok_or_else(|| fail(line_no, "usage: @expect ending id"))?;
            if engine.ending() != Some(id) {
                return fail_got(format!("ending is {:?}, expected {id}", engine.ending()));
            }
        }
        "!item" => {
            let id = parts
                .next()
                .ok_or_else(|| fail(line_no, "usage: @expect !item id"))?;
            if engine.item_count(id) > 0 {
                return fail_got(format!("still carrying {id}"));
            }
        }
        other => return Err(fail(line_no, format!("unknown expect '{other}'"))),
    }
    Ok(())
}

fn walk_dir(engine: &mut Engine, line_no: usize, args: &str) -> Result<(), WalkError> {
    if engine.is_locked() {
        return Err(fail(line_no, "input is locked; @dismiss first"));
    }
    let mut parts = args.split_whitespace();
    let dir = parts
        .next()
        .ok_or_else(|| fail(line_no, "usage: @walk dir ticks"))?;
    let ticks: u32 = parts
        .next()
        .ok_or_else(|| fail(line_no, "usage: @walk dir ticks"))?
        .parse()
        .map_err(|_| fail(line_no, "walk ticks must be an integer"))?;
    let dir =
        Dir::from_name(dir).ok_or_else(|| fail(line_no, format!("unknown direction {dir}")))?;
    engine.force_walk(dir, ticks);
    engine.step(ticks);
    Ok(())
}

fn walk_to(engine: &mut Engine, line_no: usize, args: &str) -> Result<(), WalkError> {
    if engine.is_locked() {
        return Err(fail(line_no, "input is locked; @dismiss first"));
    }
    let (x, y) = if let Some(id) = args.strip_prefix("hotspot:") {
        engine
            .hotspot_stand(id)
            .ok_or_else(|| fail(line_no, format!("no stand point for hotspot {id}")))?
    } else if let Some(id) = args.strip_prefix("exit:") {
        engine
            .exit_target(id)
            .ok_or_else(|| fail(line_no, format!("no target for exit {id}")))?
    } else {
        parse_xy(args).map_err(|msg| fail(line_no, msg))?
    };
    engine
        .script_walk_to(x, y)
        .map_err(|msg| fail(line_no, msg))?;
    let mut ticks = 0;
    while engine.route_active() {
        engine.step(1);
        ticks += 1;
        if ticks > 1800 {
            return Err(fail(line_no, format!("walk to {x},{y} timed out")));
        }
    }
    Ok(())
}

fn parse_xy(args: &str) -> Result<(i32, i32), String> {
    let (x, y) = args
        .split_once(',')
        .ok_or_else(|| format!("expected x,y, got '{args}'"))?;
    let x = x.trim().parse().map_err(|_| format!("bad x in '{args}'"))?;
    let y = y.trim().parse().map_err(|_| format!("bad y in '{args}'"))?;
    Ok((x, y))
}

fn split_cmd(rest: &str) -> (&str, &str) {
    match rest.split_once(char::is_whitespace) {
        Some((cmd, args)) => (cmd, args.trim()),
        None => (rest, ""),
    }
}

fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(idx) => &line[..idx],
        None => line,
    }
}

fn pump_route(engine: &mut Engine, line_no: usize) -> Result<(), WalkError> {
    let mut ticks = 0;
    while engine.route_active() {
        engine.step(1);
        ticks += 1;
        if ticks > 1800 {
            return Err(fail(line_no, "walk timed out"));
        }
    }
    Ok(())
}

fn fail(line: usize, message: impl Into<String>) -> WalkError {
    WalkError {
        line,
        message: message.into(),
    }
}
