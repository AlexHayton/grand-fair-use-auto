//! Running a [`Script`] against a [`World`]: the input for each tick, the
//! fixtures, the run log, state hashes and the checks in the script's
//! header.
//!
//! Pure like the rest of the simulation, so the same replay gives the same
//! hashes natively and in wasm.

use crate::input::{Buttons, InputLog, TickInput};
use crate::map::{SimMap, WallTiles};
use crate::script::{Order, Script};
use crate::view::SimView;
use crate::world::{Doing, LogLine, Start, World};
use gfa_formats::cmp::Cmp;
use gfa_formats::mission::Section;
use gfa_formats::style::{CarInfo, Style};
use std::fmt::Write as _;

/// What a tick asks of the caller besides simulating it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TickPlan {
    pub input: TickInput,
    /// Save a picture after this tick.
    pub dump: bool,
}

/// Walks a script one tick at a time.
#[derive(Debug, Clone, Default)]
pub struct Runner {
    /// Index of the current line.
    i: usize,
    /// Ticks left on it; 0 before it starts.
    left: u32,
    /// Keys held from order to order.
    held: Buttons,
    /// Keys pressed for this tick only.
    tap: Buttons,
    /// The special key, held from `tab 1` to `tab 0` across waits (in
    /// AmiGTA1 the key's state outlives the order).
    tab: bool,
    legs: u32,
}

impl Runner {
    pub fn new() -> Self {
        Runner::default()
    }

    /// Has every line run?
    pub fn done(&self, script: &Script) -> bool {
        self.i >= script.lines.len()
    }

    /// Before a tick: start the next order if the last one finished, apply
    /// any fixture to the world, and give the tick's input.
    pub fn before_tick(&mut self, script: &Script, world: &mut World) -> TickPlan {
        self.tap = Buttons::NONE;
        let mut dump = false;
        if self.left == 0 {
            self.start_order(script, world);
        }
        if let Some(line) = script.lines.get(self.i) {
            if matches!(line.order, Order::Dump | Order::Film(_)) {
                dump = true;
            }
        } else {
            // Past the end: hands off the keys.
            self.held = Buttons::NONE;
            self.tab = false;
        }
        let tab = if self.tab {
            Buttons::SPECIAL
        } else {
            Buttons::NONE
        };
        TickPlan {
            input: TickInput::new(Buttons(self.held.0 | self.tap.0 | tab.0)),
            dump,
        }
    }

    /// After a tick: count it off the current order, and log a leg that has
    /// finished.
    pub fn after_tick(&mut self, script: &Script, world: &mut World) {
        if self.done(script) || self.left == 0 {
            return;
        }
        self.left -= 1;
        if self.left == 0 {
            self.finish_order(script, world);
        }
    }

    /// Log any legs of zero ticks waiting at the current line, without
    /// starting anything else.
    pub fn flush(&mut self, script: &Script, world: &mut World) {
        if self.left != 0 {
            return;
        }
        while let Some(Order::Leg { ticks: 0, .. }) = script.lines.get(self.i).map(|l| &l.order) {
            self.finish_order(script, world);
        }
    }

    /// Begin the line at `self.i`, skipping over legs of zero ticks (they
    /// only log).
    fn start_order(&mut self, script: &Script, world: &mut World) {
        while let Some(line) = script.lines.get(self.i) {
            match line.order {
                Order::Wait(_) => self.held = Buttons::NONE,
                Order::Run {
                    up,
                    down,
                    steer,
                    handbrake,
                    ..
                } => {
                    let fire = Buttons(self.held.0 & Buttons::FIRE.0);
                    self.held = fire
                        .with(Buttons::UP, up)
                        .with(Buttons::DOWN, down)
                        .with(Buttons::RIGHT, steer > 0)
                        .with(Buttons::LEFT, steer < 0)
                        .with(Buttons::HANDBRAKE, handbrake);
                }
                Order::Leg {
                    turn,
                    forward,
                    walk,
                    ..
                } => {
                    // The fire key stays as it was, as under a `run`.
                    self.held = Buttons(self.held.0 & Buttons::FIRE.0)
                        .with(Buttons::RIGHT, turn > 0)
                        .with(Buttons::LEFT, turn < 0)
                        .with(Buttons::UP, forward > 0)
                        .with(Buttons::DOWN, forward < 0)
                        .with(Buttons::WALK, walk);
                }
                Order::Goto { bx, by, face } => world.teleport(bx, by, face),
                Order::Stand { bx, by, layer } => world.stand(bx, by, layer),
                Order::Drive {
                    model,
                    bx,
                    by,
                    face,
                } => match usize::try_from(model) {
                    Ok(model) => world.put_in_car(model, bx, by, face),
                    Err(_) => world.say(format!("script: line {}: bad model", line.number)),
                },
                Order::Face(a) => {
                    world.player.angle = a & 255;
                    world.say(format!("player: face {}", a & 255));
                }
                // Held until the next `wait`, as in AmiGTA1.
                Order::Fire(_) => self.held.set(Buttons::FIRE, true),
                Order::Enter => self.tap = Buttons::ENTER,
                Order::Jump => self.tap = Buttons::HANDBRAKE,
                Order::Dump | Order::Film(_) => {}
                Order::Fixture { ref name, ref args } => match (name.as_str(), args.as_slice()) {
                    ("damage", &[points]) => world.damage_car(points),
                    ("park", &[model, dx, dy, face]) => match usize::try_from(model) {
                        Ok(model) => world.park(model, dx, dy, face),
                        Err(_) => world.say(format!("script: line {}: bad model", line.number)),
                    },
                    ("ped", &[dx, dy, angle]) => world.drop_ped(dx, dy, angle),
                    ("tab", &[down]) => self.tab = down != 0,
                    ("weapon", &[w, rounds]) => world.give_weapon(w, rounds),
                    ("hurt", &[points]) => world.hurt(points),
                    ("shootped", &[range]) => world.shoot_ped(range),
                    ("crate", &[dx, dy, kind, amount]) => world.drop_crate(dx, dy, kind, amount),
                    ("copcar", &[dx, dy, face]) => world.drop_copcar(dx, dy, face),
                    ("lives", &[n]) => {
                        world.lives = n;
                        world.say(format!("player: lives {n}"));
                    }
                    ("pool", &[dx, dy]) => world.drop_pool(dx, dy),
                    ("killped", &[line]) => world.kill_ped_decl(line),
                    ("wreck", &[line]) => world.wreck_car_decl(line),
                    ("score", &[delta]) => world.add_score(i64::from(delta)),
                    ("script", &[line]) => world.start_script_line(line),
                    _ => world.say(format!(
                        "script: line {}: `{name}` is not supported yet",
                        line.number
                    )),
                },
            }
            self.left = line.order.ticks();
            if self.left > 0 {
                return;
            }
            self.finish_order(script, world);
        }
    }

    fn finish_order(&mut self, script: &Script, world: &mut World) {
        let line = &script.lines[self.i];
        if let Order::Leg {
            turn,
            forward,
            walk,
            ticks,
        } = line.order
        {
            let summary = world.player_summary();
            world.say(format!(
                "leg {:2}: turn {turn:2} fwd {forward:2} walk {} x{ticks:3}  ->  {summary}",
                self.legs,
                u8::from(walk)
            ));
            self.legs += 1;
        }
        self.i += 1;
    }
}

/// How long to run and how often to hash.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunOptions {
    /// Ticks to run; `None` runs to the end of the script.
    pub ticks: Option<u32>,
    /// Record the state hash every this many ticks (and at the start and
    /// the end).
    pub hash_every: Option<u32>,
}

/// What a run produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub ticks: u32,
    /// `(tick, hash)` pairs.
    pub hashes: Vec<(u32, u64)>,
    pub log: Vec<LogLine>,
    /// Header checks that failed, empty when the run passed.
    pub failures: Vec<String>,
}

/// Run `script` on `world`. `each_tick` is called after every tick with the
/// world and whether the script asked for a picture.
pub fn run(
    world: &mut World,
    script: &Script,
    opts: RunOptions,
    each_tick: impl FnMut(&World, bool),
) -> Report {
    run_recorded(world, script, opts, &mut InputLog::default(), each_tick)
}

/// [`run`], also recording the input of every tick into `recording`.
pub fn run_recorded(
    world: &mut World,
    script: &Script,
    opts: RunOptions,
    recording: &mut InputLog,
    mut each_tick: impl FnMut(&World, bool),
) -> Report {
    let total = opts.ticks.unwrap_or_else(|| script.ticks());
    let mut runner = Runner::new();
    let mut report = Report::default();
    let hash_at = |t: u32| {
        opts.hash_every
            .is_some_and(|k| k > 0 && t.is_multiple_of(k))
    };
    if opts.hash_every.is_some() {
        report.hashes.push((world.tick, world.hash()));
    }
    let first = world.tick;
    for _ in 0..total {
        let plan = runner.before_tick(script, world);
        recording.record(world.tick, plan.input);
        world.step(plan.input);
        runner.after_tick(script, world);
        each_tick(world, plan.dump);
        if hash_at(world.tick - first) {
            report.hashes.push((world.tick, world.hash()));
        }
    }
    // Legs of zero ticks waiting at the end still log.
    runner.flush(script, world);
    if opts.hash_every.is_some() && report.hashes.last().map(|h| h.0) != Some(world.tick) {
        report.hashes.push((world.tick, world.hash()));
    }
    report.ticks = world.tick - first;
    report.log = world.drain_log();
    report.failures = check(script, &report.log);
    report
}

/// Play back a recording: the same ticks with the same input, no script.
pub fn play(
    world: &mut World,
    recording: &InputLog,
    ticks: u32,
    hash_every: Option<u32>,
) -> Vec<(u32, u64)> {
    let first = world.tick;
    let mut hashes = vec![(world.tick, world.hash())];
    for _ in 0..ticks {
        world.step(recording.at(world.tick));
        if hash_every.is_some_and(|k| k > 0 && (world.tick - first).is_multiple_of(k)) {
            hashes.push((world.tick, world.hash()));
        }
    }
    if hashes.last().map(|h| h.0) != Some(world.tick) {
        hashes.push((world.tick, world.hash()));
    }
    hashes
}

/// The script header's checks against a run log.
pub fn check(script: &Script, log: &[LogLine]) -> Vec<String> {
    let mut failures = Vec::new();
    for want in &script.expect {
        if !log.iter().any(|l| l.text.contains(want.as_str())) {
            failures.push(format!("expected a line containing `{want}`"));
        }
    }
    for bad in &script.reject {
        if let Some(l) = log.iter().find(|l| l.text.contains(bad.as_str())) {
            failures.push(format!("tick {}: rejected `{bad}` in `{}`", l.tick, l.text));
        }
    }
    failures
}

/// One line of the per-tick trace, in the format the AmiGTA1 oracle
/// harnesses print, so the two can be diffed: the player on foot
/// (`tools/oracle/walktrace.c`), or his car (`drivetrace.c`).
pub fn trace_line(world: &World) -> String {
    if let (Doing::Driving, Some(d)) = (world.doing, &world.driven) {
        let v = &d.veh;
        let air = d.air.map_or(0, |a| a.z);
        return format!(
            "c {} x {} y {} v {} {} a {} w {} s {} o {} {} z {} air {} d {}",
            world.tick,
            v.x,
            v.y,
            v.vx,
            v.vy,
            v.ang16,
            v.omega,
            v.steer16,
            v.ox,
            v.oy,
            d.layer,
            air,
            v.damage
        );
    }
    let p = &world.player;
    format!(
        "t {} x {:08x} y {:08x} z {} a {} anim {} f {} {} b {}{} g {}",
        world.tick,
        p.x as u32,
        p.y as u32,
        p.layer,
        p.angle,
        p.anim.trace_code(),
        p.frame,
        p.frame_tick,
        u8::from(p.blocked_x),
        u8::from(p.blocked_y),
        p.ground
    )
}

/// A level's world from its parsed files. The player starts at `start`
/// (facing south, as AmiGTA1's walk harness does), or else at the section's
/// `PLAYER`. `gta` is false for London's maps, whose fence tiles are not
/// known yet. `map_cars` false leaves out the map's parked cars, as the
/// AmiGTA1 oracle runs.
pub fn level_world(
    map: Cmp,
    cars: Vec<CarInfo>,
    section: &Section,
    gta: bool,
    view: SimView,
    start: Option<(i32, i32)>,
    map_cars: bool,
) -> Result<World, String> {
    let start = match start {
        Some((bx, by)) => Start {
            bx,
            by,
            angle: None,
        },
        None => Start::from_section(section)
            .ok_or_else(|| format!("section [{}] has no PLAYER", section.id))?,
    };
    let walls = WallTiles::for_map(map.style_number, gta);
    let mut world = World::with_map_cars(SimMap::new(map, walls), cars, view, start, map_cars);
    if map_cars {
        world.load_section(section);
    } else {
        world.section = Some(section.clone());
    }
    Ok(world)
}

impl Report {
    /// The log and the hashes as text, one line each: what `gfa-tool run`
    /// prints, and what the native and wasm builds must agree on.
    pub fn to_text(&self) -> String {
        let mut s = String::new();
        for line in &self.log {
            let _ = writeln!(s, "[{:6}] {}", line.tick, line.text);
        }
        for (tick, hash) in &self.hashes {
            let _ = writeln!(s, "hash {tick} {hash:016x}");
        }
        s
    }
}

/// A whole run from file contents: the map, its style, the mission file,
/// the section to play and the script. What the wasm determinism check runs.
pub fn run_bytes(
    cmp: &[u8],
    style: &[u8],
    mission_ini: &[u8],
    section: u32,
    gta: bool,
    script: &str,
    opts: RunOptions,
) -> Result<Report, String> {
    let script = Script::parse(script).map_err(|e| e.to_string())?;
    let sections = gfa_formats::mission::parse(mission_ini).map_err(|e| e.to_string())?;
    let section = sections
        .iter()
        .find(|s| s.id == section)
        .ok_or_else(|| format!("no section [{section}]"))?;
    let map = Cmp::parse(cmp).map_err(|e| e.to_string())?;
    let cars = Style::parse(style).map_err(|e| e.to_string())?.cars;
    let mut world = level_world(map, cars, section, gta, SimView::GAME, script.start, true)?;
    Ok(run(&mut world, &script, opts, |_, _| {}))
}
