//! The mission script interpreter, ported from GTA8.EXE's own: the live
//! scheduler at 0xa1490, its command switch through the table at 0xa1234,
//! and the trigger scan at 0xf3800 (`docs/MISSION_VM.md`, read with
//! `tools/dosprobe/gta8.py`). AmiGTA1's `gta_script_run.c` (MIT) is the
//! guide to what each command is for; where it differs from the original,
//! this follows the original.
//!
//! The shape of it:
//!
//! - **The frame.** The script runs once a game frame (23.333 Hz): the
//!   trigger scan first, then slots 0 to 31 in order, one command each.
//!   After every command the points target is checked; once a player has
//!   reached it, every process ends.
//! - **Processes.** 32 slots. Slots 0 to 3 are the players' own: the
//!   loader starts each player's at command 0 (`0 DONOWT` in every section
//!   of GTA's, so it idles there). A new process takes the first free slot
//!   from 4. Each keeps the command it is
//!   on (an index), its state (1 when it has just arrived there), the
//!   trigger that started it, its parent (the root of the chain is the
//!   player, who is paid) and its keep class (`RESET` spares 1 and 2).
//! - **Moving on.** `p2` on success, `p3` on failure: 0 is the next command,
//!   -1 ends the process and re-arms its trigger, any other negative frees
//!   the slot, and anything else is a label, looked up in a table that is
//!   never cleared: a label the section lacks lands on command 0, which is
//!   `DONOWT` in every section, and the process idles there.
//! - **Pay.** Most commands pay `p5` on their success path (each case says).
//! - **Triggers.** 29-byte records made by the declarations in file order,
//!   all born armed. The scan fires an armed one whose block range holds
//!   the player.
//!
//! What gfa-sim cannot do yet it does not pretend to: a command not ported
//! (or a use of one gfa-sim cannot serve yet) is logged once with its
//! section and line, and the process waits on it. It never takes the
//! command's success path. A name GTA8.EXE does not know stops the original
//! (error -182); here it waits the same way, logged.
//!
//! Ours, where GTA8.EXE writes outside its arrays (a start with no free
//! slot, a trigger index out of range, a `KICKSTART` with no record free):
//! nothing happens and the line is logged.

use std::collections::{BTreeMap, BTreeSet};

use gfa_formats::mission::Section;

use super::{DeclKind, Op};
use crate::clock::Ratio;
use crate::hash::StateHasher;
use crate::script_world::ScriptWorld;

/// Process slots.
pub const SLOTS: usize = 32;
/// The first slot a process can take: 0 to 3 are the players'.
pub const FIRST_SCRIPT_SLOT: usize = 4;
/// Trigger records, at most (GTA8.EXE's range check, 0xeaa60).
pub const MAX_TRIGGERS: usize = 210;
/// Children a `KICKSTART`'s record holds.
const KICK_SLOTS: usize = 5;
/// The player whose processes these are. Single player only.
const PLAYER_SLOT: i32 = 0;
/// Door records, at most (0xe84a0 walks 64 of them).
pub const MAX_DOORS: usize = 64;

/// The trigger types the scan fires (GTA8.EXE's record +4).
const TYPE_TRIGGER: u8 = 4;
const TYPE_MPHONES: u8 = 12;
const TYPE_PHONE_TOGG: u8 = 13;
/// A player driving this model sets off no `MPHONES` (the scan's check, 0xf4075).
const MPHONES_IGNORES_MODEL: u8 = 47;
/// A `PHONE_TOGG` re-arms its phone once the player is outside this many
/// blocks of it, but inside its own radius (0xf5053).
const PHONE_TOGG_INNER: i32 = 3;
/// `MPHONE` gives up when the player is more than this many blocks from its
/// first phone (0xf1900 with 15).
const MPHONE_REACH: i32 = 15;
/// He answers a phone within this many of the original's pixels of it, on
/// either axis, exclusive (0xf17a0).
const ANSWER_REACH_PX: i32 = 16;
/// `MPHONE`'s ring count, in its own p4: set at the start, counted down a
/// frame, a ring at 0 and another at -10, then set again.
const RING_START: i32 = 25;
const RING_SECOND: i32 = -10;
/// After he answers, no phone rings or answers for this many frames (the
/// event 0x10b50(250, 0, 50 + player)).
const PHONE_LOCK_FRAMES: u32 = 250;
/// `STARTUP`'s event: 0x10b50(10, 0, 20).
const STARTUP_SOUND_FRAMES: u32 = 10;

/// A trigger record's state (record +0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TriggerState {
    /// Firing: the scan's state between armed and started.
    Firing = 0,
    Armed = 1,
    /// An armed trigger with a delay waits here (0x10b50, not ported).
    Waiting = 2,
    Started = 3,
    Disabled = 4,
}

/// A trigger record (GTA8.EXE's 29 bytes at 0x3db2e8, made by 0xe8340).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trigger {
    pub state: TriggerState,
    /// GTA8.EXE's trigger type, 0 to 33 (record +4). `TRIGGER` makes 4.
    pub kind: u8,
    /// The line it starts (+8).
    pub line: i32,
    /// +0xc: -1, or for some types the third number.
    pub arg: i32,
    /// The block (+0x10 to +0x12, bytes).
    pub block: [i32; 3],
    /// Blocks either way (+0x13).
    pub radius: i32,
    /// +0x17: an armed trigger with a delay waits first.
    pub delay: i32,
    /// The process ended when it fires (+0x1b, `SET_KILLTRIG`), or -1.
    pub kill: i32,
    /// The declaration's flag (+0x1c).
    pub flag: u8,
    /// The declaration that made it.
    pub decl: usize,
}

/// A door's state (its record +4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DoorState {
    Open = 0,
    Shut = 1,
    Opening = 2,
    Closing = 3,
}

/// A door record (GTA8.EXE's 36 bytes at 0x3da9e5, made by 0xe7e10 as the
/// section loads). This is what the script sees. The door itself (its
/// object, its blocks, and the scan that opens it for the player) is the
/// world's and is not ported, so no door here ever moves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Door {
    pub state: DoorState,
    /// Turned on by `DOOR_ON`, so it can open (the record's +8, 0 when on).
    /// Every door loads off.
    pub on: bool,
    /// The declaration's flag (+0x26). `RESET` shuts and turns off the doors
    /// without one.
    pub flag: u8,
    /// The declaration that made it.
    pub decl: usize,
}

/// A phone (GTA8.EXE's object 0x28, made by 0x76da0 for each `TELEPHONE`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phone {
    /// The declaration's block: the phone stands at its centre.
    pub block: [i32; 3],
    /// Its facing: the declaration's second number, 1024 steps a turn.
    pub turn: i32,
    /// Reads as the object's frame (its +0xc, set by 0xad590): 0 as made,
    /// 3 when it starts ringing, 2 and 3 in turn at each ring (0xf1dd0),
    /// and 1 once it stops.
    pub frame: u8,
}

/// An event on GTA8.EXE's timed queue (0x10b50 puts one on, 0x10ed0 runs
/// the ones due once a frame, before the script). Only the two the ported
/// commands schedule are kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Event {
    /// Kind 0 with 20, from `STARTUP`: sound 0 (0x16870). Sound is M8's.
    StartupSound,
    /// Kind 0 with 50 + the player, from answering a phone: his phone lock
    /// ends (0x2d24f0 cleared, at 0x9f85d).
    PhoneLockOver,
}

/// A declaration as the interpreter keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decl {
    pub label: i32,
    /// `None` for a name GTA8.EXE does not know.
    pub kind: Option<DeclKind>,
    pub flag: u8,
    /// As written: blocks or the original's pixels (64 to a block).
    pub pos: [i32; 3],
    pub nums: Vec<i32>,
    pub source_line: usize,
    /// Record +4: a counter's value, a trigger's index, or -1.
    pub handle: i32,
    /// Record +8.
    pub second: i32,
}

/// A command. Its numbers can change as it runs: `SURVIVE` counts in its
/// `p3`, and `KICKSTART` writes its record into `p4` and its slot into `p5`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cmd {
    pub label: i32,
    /// `None` for a name GTA8.EXE does not know.
    pub op: Option<Op>,
    pub name: String,
    pub p: [i32; 5],
    pub source_line: usize,
}

/// A process slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Proc {
    pub in_use: bool,
    /// The command it is on, by index.
    pub cmd: usize,
    /// 1 when it has just arrived at the command; commands count it up.
    pub state: u16,
    /// The trigger that started it, or -1.
    pub trigger: i32,
    /// The slot that started it, or -1: the root of the chain is paid.
    pub parent: i32,
    /// `RESET` spares 1 (`KEEP_THIS_PROC`) and 2.
    pub keep: i32,
}

impl Proc {
    const FREE: Proc = Proc {
        in_use: false,
        cmd: 0,
        state: 0,
        trigger: -1,
        parent: -1,
        keep: 0,
    };
}

/// A line of text a command shows. Drawing it is the HUD's (M7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Brief {
    /// `MOBILE_BRIEF`: the text with this key, on the mobile phone.
    Mobile(i32),
}

/// One section's script, running.
#[derive(Debug, Clone)]
pub struct Vm {
    pub section: u32,
    pub decls: Vec<Decl>,
    decl_labels: BTreeMap<i32, usize>,
    pub cmds: Vec<Cmd>,
    cmd_labels: BTreeMap<i32, usize>,
    pub procs: [Proc; SLOTS],
    pub triggers: Vec<Trigger>,
    /// `KICKSTART` records: the slots of its children, -1 for none.
    kicks: Vec<[i32; KICK_SLOTS]>,
    /// `STARTUP` has run (GTA8.EXE's 0x26cf6c): it runs once a game.
    started: bool,
    /// The arrow's target in 16.16 world pixels and the layer, or `None`.
    pub arrow: Option<(i32, i32, i32)>,
    /// The door records, in load order.
    pub doors: Vec<Door>,
    /// An `MPHONE` is ringing (0x3dcbb8): one rings at a time.
    pub ringing: bool,
    /// He answered a phone in the last 250 frames (0x2d24f0): no `MPHONES`
    /// starts and no `MPHONE` rings or answers.
    pub phone_lock: bool,
    /// The phones made, in load order. A `TELEPHONE`'s handle is its index
    /// here (GTA8.EXE's is its object's), -1 when none was made or once it
    /// is answered; the phone itself stays.
    pub phones: Vec<Phone>,
    /// The event clock (0x1427a0): one a game frame.
    clock: u32,
    /// Timed events, soonest first: (due, event).
    events: Vec<(u32, Event)>,
    /// Texts shown since the last [`Vm::drain_briefs`].
    briefs: Vec<Brief>,
    /// The game frame: 7 of every 15 ticks.
    frame: Ratio,
    /// Commands already logged as waiting, by index, so each is logged once.
    waiting: BTreeSet<usize>,
    log: Vec<String>,
}

/// What one command did with its process.
enum Step {
    /// Stays on the command (counting, waiting, idling).
    Stay,
    /// Moved on (the case has set the command).
    Moved,
    /// Cannot be done here yet: logged once, and the process waits on it.
    Wait(&'static str),
}

impl Vm {
    /// The script of `section`, with its triggers made and armed.
    pub fn new(section: &Section) -> Vm {
        let mut vm = Vm {
            section: section.id,
            decls: Vec::with_capacity(section.decls.len()),
            decl_labels: BTreeMap::new(),
            cmds: Vec::with_capacity(section.cmds.len()),
            cmd_labels: BTreeMap::new(),
            procs: [Proc::FREE; SLOTS],
            triggers: Vec::new(),
            kicks: Vec::new(),
            started: false,
            arrow: None,
            doors: Vec::new(),
            ringing: false,
            phone_lock: false,
            phones: Vec::new(),
            clock: 0,
            events: Vec::new(),
            briefs: Vec::new(),
            frame: crate::clock::game_frame(),
            waiting: BTreeSet::new(),
            log: Vec::new(),
        };
        for (i, d) in section.decls.iter().enumerate() {
            vm.load_decl(i, d);
        }
        for (i, c) in section.cmds.iter().enumerate() {
            let mut p = [0; 5];
            for (k, v) in p.iter_mut().enumerate() {
                *v = c.p(k + 1);
            }
            // The loader writes each label's index; a later line with the same
            // label wins, as it does there.
            vm.cmd_labels.insert(c.id, i);
            vm.cmds.push(Cmd {
                label: c.id,
                op: Op::from_name(&c.name),
                name: c.name.clone(),
                p,
                source_line: c.source_line,
            });
        }
        // 0x7a695: with the commands read, each player's slot starts at
        // command 0 (its trigger 0, its parent none), and slots 4 to 31 are
        // freed.
        vm.procs[PLAYER_SLOT as usize] = Proc {
            in_use: true,
            cmd: 0,
            state: 1,
            trigger: 0,
            parent: -1,
            keep: 0,
        };
        vm
    }

    /// GTA8.EXE's loader case for one declaration (the switch at 0x798ea).
    fn load_decl(&mut self, index: usize, d: &gfa_formats::mission::Decl) {
        let kind = DeclKind::from_name(&d.kind);
        let a = d.nums.first().copied().unwrap_or(0);
        let b = d.nums.get(1).copied().unwrap_or(0);
        let flag = d.flag.and_then(|f| u8::try_from(f).ok()).unwrap_or(0);
        let mut decl = Decl {
            label: d.id,
            kind,
            flag,
            pos: d.pos,
            nums: d.nums.clone(),
            source_line: d.source_line,
            handle: -1,
            second: 0,
        };
        match kind {
            // 0x79b5d, and `POWERUP`'s own 0x79db3: the first two numbers
            // into +4 and +8. A `POWERUP` is no more than that until a
            // `POWERUP_ON` makes its crate.
            Some(DeclKind::Counter | DeclKind::Target | DeclKind::Future | DeclKind::Powerup) => {
                decl.handle = a;
                decl.second = b;
            }
            // 0x7a47e, 0x7a49f, 0x7a4c0: the first number, twice.
            Some(
                DeclKind::MissionCounter | DeclKind::SecretMissionCounter | DeclKind::MissionTotal,
            ) => {
                decl.handle = a;
                decl.second = a;
            }
            // 0x79bb9: a door record, shut and off, its index in +4.
            Some(DeclKind::Door) => decl.handle = self.make_door(index, &decl),
            // 0x7991f (0x76da0): a phone at the block's centre; +4 holds it,
            // +8 the first number when above 0 (otherwise -1).
            Some(DeclKind::Telephone) => {
                decl.handle = self.make_phone(&decl);
                decl.second = if a > 0 { a } else { -1 };
            }
            // 0x79ffb: the line it would start is the trigger of the
            // `MPHONES` its first number names; the radius is its second.
            Some(DeclKind::PhoneTogg) => {
                let phone = self.decls.get(self.decl_index(a)).map_or(-1, |m| m.handle);
                decl.handle = self.make_trigger(TYPE_PHONE_TOGG, index, &decl, phone, b);
            }
            Some(k) => {
                if let Some(t) = trigger_type(k) {
                    decl.handle = self.make_trigger(t, index, &decl, a, b);
                }
            }
            None => {}
        }
        self.decl_labels.insert(d.id, index);
        self.decls.push(decl);
    }

    /// 0xe7e10 and 0xe8600, as the loader's `DOOR` case calls them: a door
    /// record, shut and off. Past the last record GTA8.EXE stops (error -1);
    /// ours: no record, logged.
    fn make_door(&mut self, decl: usize, d: &Decl) -> i32 {
        if self.doors.len() >= MAX_DOORS {
            self.log.push(format!(
                "mission: door {} has no record free: the original stops here",
                d.label
            ));
            return -1;
        }
        self.doors.push(Door {
            state: DoorState::Shut,
            on: false,
            flag: d.flag,
            decl,
        });
        (self.doors.len() - 1) as i32
    }

    /// 0x76da0: a phone (object 0x28) at the block's centre, turned by the
    /// second number. The object routine makes nothing off the map's blocks
    /// or heights, as for a crate.
    fn make_phone(&mut self, d: &Decl) -> i32 {
        let [x, y, z] = d.pos;
        if !(1..=255).contains(&x) || !(1..=255).contains(&y) || !(1..=5).contains(&z) {
            return -1;
        }
        self.phones.push(Phone {
            block: d.pos,
            turn: d.nums.get(1).copied().unwrap_or(0),
            frame: 0,
        });
        (self.phones.len() - 1) as i32
    }

    /// The phone a `TELEPHONE`'s handle names, while it has one.
    fn phone_mut(&mut self, d: usize) -> Option<&mut Phone> {
        let h = usize::try_from(self.decls[d].handle).ok()?;
        self.phones.get_mut(h)
    }

    /// Set the frame of each of these phones that still has its handle
    /// (0xf1b10).
    fn phones_frame(&mut self, phones: &[usize], frame: u8) {
        for &d in phones {
            if let Some(p) = self.phone_mut(d) {
                p.frame = frame;
            }
        }
    }

    /// 0xe8340: a trigger record, armed. `TRIGGER` and `MPHONES` give it
    /// the line to start (their first number) and the radius (their second);
    /// `PHONE_TOGG` gives the trigger it re-arms in place of the line. The
    /// other kinds' records are made in the same order so the numbering
    /// matches, but none of them fires yet.
    fn make_trigger(&mut self, kind: u8, decl: usize, d: &Decl, a: i32, b: i32) -> i32 {
        if self.triggers.len() >= MAX_TRIGGERS {
            return -1;
        }
        let index = self.triggers.len() as i32;
        self.triggers.push(Trigger {
            state: TriggerState::Armed,
            kind,
            line: a,
            arg: -1,
            block: d.pos,
            radius: b,
            delay: 0,
            kill: -1,
            flag: d.flag,
            decl,
        });
        index
    }

    /// The declaration a label names. GTA8.EXE's label table is never
    /// cleared, so a label the section lacks reads record 0.
    fn decl_index(&self, label: i32) -> usize {
        self.decl_labels.get(&label).copied().unwrap_or(0)
    }

    /// The `POWERUP` a label names, by index. GTA8.EXE reads whatever
    /// record the label gives (0x75f20, 0x7acf0): in sections 102 and 103,
    /// 56 `POWERUP_ON`s name labels the section lacks and read record 0 (a
    /// phone, a parked car). gfa-sim does not make crates from those.
    fn powerup(&self, label: i32) -> Option<usize> {
        let d = self.decl_index(label);
        (self.decls.get(d)?.kind == Some(DeclKind::Powerup)).then_some(d)
    }

    /// The command a label names; a missing label lands on command 0.
    fn cmd_index(&self, label: i32) -> usize {
        self.cmd_labels.get(&label).copied().unwrap_or(0)
    }

    /// The texts shown since the last call, oldest first.
    pub fn drain_briefs(&mut self) -> Vec<Brief> {
        std::mem::take(&mut self.briefs)
    }

    /// The log lines since the last call.
    pub fn drain_log(&mut self) -> Vec<String> {
        std::mem::take(&mut self.log)
    }

    /// The running processes: slot, the label of the command each is on,
    /// and its name.
    pub fn running(&self) -> Vec<(usize, i32, &str)> {
        self.procs
            .iter()
            .enumerate()
            .filter(|(_, p)| p.in_use)
            .map(|(s, p)| match self.cmds.get(p.cmd) {
                Some(c) => (s, c.label, c.name.as_str()),
                None => (s, -1, "past the end"),
            })
            .collect()
    }

    /// One simulation tick: a game frame of the script when the clock says.
    pub fn tick(&mut self, w: &mut dyn ScriptWorld) {
        if self.frame.step() {
            self.run_frame(w);
        }
    }

    /// One game frame: the events due, the trigger scan, then every process
    /// once.
    pub fn run_frame(&mut self, w: &mut dyn ScriptWorld) {
        self.run_events();
        self.scan_triggers(w);
        for slot in 0..SLOTS {
            if !self.procs[slot].in_use {
                continue;
            }
            self.step(slot, w);
            // 0xea550 after every command: the level's target met ends every
            // process (the slots freed, nothing re-armed).
            if w.target_reached() {
                for p in &mut self.procs {
                    p.in_use = false;
                }
                self.log
                    .push("mission: the points target is met; every process ends".into());
            }
        }
    }

    /// GTA8.EXE's 0x10b50: an event `frames` from now. It goes before the
    /// first one due at the same time or later.
    fn schedule(&mut self, frames: u32, e: Event) {
        let due = self.clock.wrapping_add(frames);
        let at = self.events.partition_point(|&(d, _)| d < due);
        self.events.insert(at, (due, e));
    }

    /// 0x10ed0: the events due now, then the clock moves on.
    fn run_events(&mut self) {
        while self.events.first().is_some_and(|&(d, _)| d == self.clock) {
            let (_, e) = self.events.remove(0);
            match e {
                Event::StartupSound => self
                    .log
                    .push("mission: STARTUP's sound 0 is due (sound waits for M8)".into()),
                Event::PhoneLockOver => {
                    self.phone_lock = false;
                    self.log.push("mission: the phones can ring again".into());
                }
            }
        }
        self.clock = self.clock.wrapping_add(1);
    }

    /// The script triggers of GTA8.EXE's scan (0xf3800), in record order,
    /// for the one player. Types 4 (`TRIGGER`), 12 (`MPHONES`) and 13
    /// (`PHONE_TOGG`) are ported; the others' records stay as they are.
    fn scan_triggers(&mut self, w: &mut dyn ScriptWorld) {
        // A player getting in or out of a car (his state 1) is skipped for
        // every trigger.
        let Some((bx, by)) = w.trigger_block() else {
            return;
        };
        let car = w.trigger_car_model();
        for t in 0..self.triggers.len() {
            let (state, kind) = (self.triggers[t].state, self.triggers[t].kind);
            if state == TriggerState::Disabled
                || !matches!(kind, TYPE_TRIGGER | TYPE_MPHONES | TYPE_PHONE_TOGG)
            {
                continue;
            }
            // The type's own check (the switch at 0xf41cc). `MPHONES` skips
            // a driver of model 47, and puts back a trigger still firing from
            // the frame before (its start waited), skipping it this frame.
            if kind == TYPE_MPHONES {
                let mut skip = car == Some(MPHONES_IGNORES_MODEL);
                if state == TriggerState::Firing {
                    self.triggers[t].state = TriggerState::Armed;
                    skip = true;
                }
                if skip {
                    continue;
                }
            }
            let trig = &self.triggers[t];
            if trig.state == TriggerState::Armed && near(trig.block, trig.radius, bx, by) {
                if trig.delay > 0 {
                    // 0x10b50 counts the delay down; not read yet, and none
                    // of these types has one.
                    self.triggers[t].state = TriggerState::Waiting;
                    continue;
                }
                self.triggers[t].state = TriggerState::Firing;
            }
            // A trigger left firing runs its case every frame, in range or
            // not (0xf45b0).
            if self.triggers[t].state != TriggerState::Firing {
                continue;
            }
            self.fire(t, bx, by);
            // The kill link (`SET_KILLTRIG`): that process ends too.
            let kill = self.triggers[t].kill;
            if kill > -1 {
                self.kill(kill as usize);
            }
        }
    }

    /// A firing trigger's case (the switch at 0xf594a).
    fn fire(&mut self, t: usize, bx: i32, by: i32) {
        let trig = self.triggers[t].clone();
        match trig.kind {
            // 0xf4b63: its line starts, which marks it started.
            TYPE_TRIGGER => self.fire_start(t),
            // 0xf4fec: not while a phone rings or the phone lock is on; it
            // stays firing, and the check puts it back next frame.
            TYPE_MPHONES => {
                if self.ringing || self.phone_lock {
                    return;
                }
                self.fire_start(t);
                self.triggers[t].state = TriggerState::Started;
            }
            // 0xf5053: inside its radius but outside 3 blocks, it re-arms
            // the phone trigger it names if that has started. It never stops
            // firing, so this runs every frame from the first time he comes
            // near until a `DISABLE`.
            TYPE_PHONE_TOGG => {
                if !near(trig.block, trig.radius, bx, by)
                    || near(trig.block, PHONE_TOGG_INNER, bx, by)
                {
                    return;
                }
                let Some(phone) = usize::try_from(trig.line).ok().filter(|&p| p < self.triggers.len())
                else {
                    return;
                };
                if self.triggers[phone].state == TriggerState::Started {
                    self.triggers[phone].state = TriggerState::Armed;
                    self.log.push(format!(
                        "mission: trigger {} re-arms trigger {}",
                        self.decls[trig.decl].label,
                        self.decls[self.triggers[phone].decl].label
                    ));
                }
            }
            _ => {}
        }
    }

    /// Start a trigger's line with the player as parent (0x75e90).
    fn fire_start(&mut self, t: usize) {
        let (line, decl) = (self.triggers[t].line, self.triggers[t].decl);
        match self.start(PLAYER_SLOT, t as i32, line) {
            Some(slot) => self.log.push(format!(
                "mission: trigger {} starts line {line} in slot {slot}",
                self.decls[decl].label
            )),
            None => self.log.push(format!(
                "mission: trigger {} has no free slot for line {line}",
                self.decls[decl].label
            )),
        }
    }

    /// GTA8.EXE's 0x75e90: mark the trigger started, then take the first
    /// free slot from 4. With none free the start fails and the trigger
    /// stays used up.
    fn start(&mut self, parent: i32, trigger: i32, label: i32) -> Option<usize> {
        if trigger > -1 {
            self.trigger_set(trigger, TriggerState::Started);
        }
        let slot = (FIRST_SCRIPT_SLOT..SLOTS).find(|&s| !self.procs[s].in_use)?;
        let keep = self.procs[slot].keep;
        self.procs[slot] = Proc {
            in_use: true,
            cmd: self.cmd_index(label),
            state: 1,
            trigger,
            parent,
            // The keep class is not set here: a slot freed without clearing
            // it hands the old one on (0x75e90 writes every field but it).
            keep,
        };
        Some(slot)
    }

    /// 0xeaa40, 0xeaa60, 0xeaab0. Outside the records GTA8.EXE stops the
    /// game (start and disable) or writes past them (arm).
    fn trigger_set(&mut self, t: i32, state: TriggerState) -> bool {
        match usize::try_from(t)
            .ok()
            .and_then(|t| self.triggers.get_mut(t))
        {
            Some(trig) => {
                trig.state = state;
                true
            }
            None => false,
        }
    }

    /// 0x7a7c0: end a process, re-arming its trigger.
    fn kill(&mut self, slot: usize) {
        let Some(p) = self.procs.get(slot).copied() else {
            return;
        };
        if p.trigger != -1 {
            self.trigger_set(p.trigger, TriggerState::Armed);
        }
        self.procs[slot] = Proc {
            trigger: 0,
            ..Proc::FREE
        };
    }

    /// The loop `RESET` and an answered `MPHONE` share: every process in
    /// slots 3 to 31 ends, re-arming its trigger, except this one, those
    /// waiting on `MPHONE`, and keep classes 1 and 2.
    fn end_others(&mut self, slot: usize) {
        for s in 3..SLOTS {
            let p = self.procs[s];
            if s == slot || !p.in_use || matches!(p.keep, 1 | 2) {
                continue;
            }
            if self.cmds.get(p.cmd).and_then(|c| c.op) == Some(Op::Mphone) {
                continue;
            }
            self.kill(s);
        }
    }

    /// The clean-ups after that loop that gfa-sim has anything for.
    /// GTA8.EXE's also take away the mission's own peds, cars and objects
    /// (0x765b0, 0x76490, 0x766e0), which no ported command makes yet, and
    /// stop its sounds (0xe7670, and for `RESET` 0xf20f0), which are M8's.
    /// `MPHONE` also clears the briefs (0x102260, 0x1029a0, not read), which
    /// are not drawn yet.
    fn clean_up(&mut self) {
        // 0xe8460: every trigger without its declaration's flag is disabled.
        for t in &mut self.triggers {
            if t.flag == 0 {
                t.state = TriggerState::Disabled;
            }
        }
        // 0xe84a0: every door without one is shut and turned off.
        for d in &mut self.doors {
            if d.flag == 0 {
                d.state = DoorState::Shut;
                d.on = false;
            }
        }
        // 0xfef50: the arrow goes out.
        self.arrow = None;
    }

    /// The root of a process's chain: the player who is paid.
    fn root(&self, slot: usize) -> usize {
        let mut s = slot;
        for _ in 0..SLOTS {
            match usize::try_from(self.procs[s].parent) {
                Ok(p) if p < SLOTS => s = p,
                _ => break,
            }
        }
        s
    }

    /// 0x7a980: pay `p5` to the root player.
    fn pay(&mut self, slot: usize, w: &mut dyn ScriptWorld) {
        let p5 = self.cmds[self.procs[slot].cmd].p[4];
        if p5 > 0 && self.root(slot) == PLAYER_SLOT as usize {
            w.pay(p5);
        }
    }

    /// GTA8.EXE's 0x7a820 (success, `p2`) and 0x7a8d0 (failure, `p3`).
    fn advance(&mut self, slot: usize, to: i32) -> Step {
        let cur = self.procs[slot].cmd;
        if to < 0 {
            self.procs[slot].in_use = false;
        }
        self.procs[slot].state = 1;
        if to == -1 {
            self.kill(slot);
            return Step::Moved;
        }
        self.procs[slot].cmd = if to == 0 { cur + 1 } else { self.cmd_index(to) };
        Step::Moved
    }

    fn success(&mut self, slot: usize) -> Step {
        let p2 = self.cmds[self.procs[slot].cmd].p[1];
        self.advance(slot, p2)
    }

    fn failure(&mut self, slot: usize) -> Step {
        let p3 = self.cmds[self.procs[slot].cmd].p[2];
        self.advance(slot, p3)
    }

    /// Pay `p5`, then take `p2`: most commands' success path.
    fn paid(&mut self, slot: usize, w: &mut dyn ScriptWorld) -> Step {
        self.pay(slot, w);
        self.success(slot)
    }

    /// One command for one process: GTA8.EXE's live cases (table 0xa1234).
    fn step(&mut self, slot: usize, w: &mut dyn ScriptWorld) {
        let ci = self.procs[slot].cmd;
        let Some(c) = self.cmds.get(ci) else {
            self.wait_on(ci, "a command past the last one");
            return;
        };
        let (op, p) = (c.op, c.p);
        let outcome = match op {
            None => {
                Step::Wait("a name GTA8.EXE does not know: the original stops here (error -182)")
            }
            Some(op) => self.run(op, p, slot, ci, w),
        };
        if let Step::Wait(why) = outcome {
            self.wait_on(ci, why);
        }
    }

    fn wait_on(&mut self, ci: usize, why: &str) {
        if !self.waiting.insert(ci) {
            return;
        }
        let (label, name, line) = self
            .cmds
            .get(ci)
            .map_or((-1, "?", 0), |c| (c.label, c.name.as_str(), c.source_line));
        self.log.push(format!(
            "mission: [{}] line {line}: {label} {name} waits: {why}",
            self.section
        ));
    }

    fn run(
        &mut self,
        op: Op,
        p: [i32; 5],
        slot: usize,
        ci: usize,
        w: &mut dyn ScriptWorld,
    ) -> Step {
        let [p1, _p2, _p3, p4, p5] = p;
        match op {
            // The empty case: the process stays on it.
            Op::Donowt | Op::Driveon | Op::Unused122 | Op::Unused123 => Step::Stay,
            // 0xa16d9: the count lives in the command's own p3. The first frame
            // loads it from p4; each later one counts it down, and the one that
            // reaches 0 pays and moves on: p4 + 1 frames in all.
            Op::Survive => {
                if self.procs[slot].state == 1 {
                    self.cmds[ci].p[2] = p4;
                    self.procs[slot].state += 1;
                    return Step::Stay;
                }
                self.cmds[ci].p[2] -= 1;
                if self.cmds[ci].p[2] > 0 {
                    return Step::Stay;
                }
                self.paid(slot, w)
            }
            // 0xa24d4: once a game. It marks p1's trigger started, puts sound
            // 0 on the timed queue for ten frames on (0x10b50(10, 0, 20)), and
            // moves on by p2 without pay; a later process stays on it for good.
            Op::Startup => {
                if self.started {
                    return Step::Stay;
                }
                let t = self.decls[self.decl_index(p1)].handle;
                if !self.trigger_set(t, TriggerState::Started) {
                    return Step::Wait("STARTUP names no trigger: the original stops here");
                }
                self.schedule(STARTUP_SOUND_FRAMES, Event::StartupSound);
                let s = self.success(slot);
                self.started = true;
                s
            }
            // 0xa36e0: a child at p1 (at the next command when p1 is 0), its
            // parent the root player, its keep class this one's. This process
            // moves on by p2 without pay. Its children go in a record whose
            // index it writes into its own p4, and its slot into its p5.
            Op::Kickstart => {
                let root = self.root(slot) as i32;
                let child = self.start(root, -1, p1);
                if let Some(ch) = child {
                    self.procs[ch].cmd = if p1 == 0 { ci + 1 } else { self.cmd_index(p1) };
                    self.procs[ch].keep = self.procs[slot].keep;
                }
                let s = self.success(slot);
                let mut rec = [-1; KICK_SLOTS];
                rec[0] = child.map_or(-1, |c| c as i32);
                self.kicks.push(rec);
                self.cmds[ci].p[3] = (self.kicks.len() - 1) as i32;
                self.cmds[ci].p[4] = slot as i32;
                s
            }
            // 0xa72af: keep class 1, then pay and move on.
            Op::KeepThisProc => {
                self.procs[slot].keep = 1;
                self.paid(slot, w)
            }
            // 0xa413c: the KICKSTART at p1's record; each child it holds ends
            // (re-arming its trigger) and the record empties. GTA's scripts
            // give it no p5, so whether it pays is moot.
            Op::KillSideProc => {
                let ks = self.cmd_index(p1);
                let rec = self.cmds.get(ks).map_or(-1, |c| c.p[3]);
                if let Some(r) = usize::try_from(rec).ok().filter(|&r| r < self.kicks.len()) {
                    for k in 0..KICK_SLOTS {
                        let child = self.kicks[r][k];
                        if let Ok(c) = usize::try_from(child) {
                            self.kill(c);
                        }
                        self.kicks[r][k] = -1;
                    }
                }
                self.success(slot)
            }
            // 0xa1a98: p1's trigger disabled, whatever the declaration's kind;
            // this process forgets its own trigger, then pays and moves on.
            Op::Disable => {
                let t = self.decls[self.decl_index(p1)].handle;
                if !self.trigger_set(t, TriggerState::Disabled) {
                    return Step::Wait("DISABLE names no trigger: the original stops here");
                }
                self.procs[slot].trigger = -1;
                self.paid(slot, w)
            }
            // 0x7d940: the kind's own routine (0x7aa60), then, for all but the
            // doors and gates, p1's trigger re-armed; then pay and move on.
            Op::Enable => {
                let d = &self.decls[self.decl_index(p1)];
                if d.kind.is_some_and(enable_has_routine) {
                    return Step::Wait(
                        "ENABLE of a door or watch trigger: its routine is not ported",
                    );
                }
                let t = d.handle;
                if !self.trigger_set(t, TriggerState::Armed) {
                    // GTA8.EXE writes past its records here; ours: nothing.
                    self.log
                        .push(format!("mission: ENABLE {p1} names no trigger"));
                }
                self.paid(slot, w)
            }
            // 0xa703c: p1's value against p4.
            Op::Compare => {
                if self.decls[self.decl_index(p1)].handle == p4 {
                    self.paid(slot, w)
                } else {
                    self.failure(slot)
                }
            }
            // 0xa6eeb: add one; at p4 or past it (and p4 not negative) pay and
            // take p2, otherwise p3.
            Op::Inccount => {
                let d = self.decl_index(p1);
                self.decls[d].handle += 1;
                if p4 <= -1 || self.decls[d].handle < p4 {
                    self.failure(slot)
                } else {
                    self.paid(slot, w)
                }
            }
            // 0xa1be6: take one; above 0 take p3, otherwise pay and take p2.
            Op::Deccount => {
                let d = self.decl_index(p1);
                self.decls[d].handle -= 1;
                if self.decls[d].handle > 0 {
                    self.failure(slot)
                } else {
                    self.paid(slot, w)
                }
            }
            // 0xa5d0b: text p5 on the mobile; moves on by p2 without pay.
            Op::MobileBrief => {
                self.briefs.push(Brief::Mobile(p5));
                self.log.push(format!("mission: brief {p5} (mobile)"));
                self.success(slot)
            }
            // 0xa1c49: the arrow at p1's position (-1 puts it out), a block
            // corner nudged to its centre; then pay and move on.
            Op::Arrow => {
                if p1 == -1 {
                    self.arrow = None;
                } else {
                    let d = &self.decls[self.decl_index(p1)];
                    if d.kind.is_some_and(moves) {
                        return Step::Wait("ARROW at something that moves: not ported");
                    }
                    let [mut x, mut y, z] = original_pixels(d);
                    if x % 64 == 0 && y % 64 == 0 {
                        x += 32;
                        y += 32;
                    }
                    self.arrow = Some((x << 15, y << 15, z));
                    self.log.push(format!("mission: arrow at {p1}"));
                }
                self.paid(slot, w)
            }
            // 0xa1e8b: the arrow out; pay and move on.
            Op::ArrowOff => {
                self.arrow = None;
                self.paid(slot, w)
            }
            // 0xa262f: a crate of the POWERUP's kind (+4) holding its amount
            // (+8) on its block (0xd7c10). It moves on by p2 without pay,
            // whether a crate was made or not.
            Op::PowerupOn => {
                let Some(d) = self.powerup(p1) else {
                    return Step::Wait(
                        "POWERUP_ON of a declaration that is not a POWERUP: the original makes a crate from its numbers",
                    );
                };
                let d = &self.decls[d];
                let (block, kind, amount) = (d.pos, d.handle, d.second);
                if !w.powerup_on(block, kind, amount) {
                    self.log
                        .push(format!("mission: POWERUP_ON {p1} made no crate"));
                }
                self.success(slot)
            }
            // 0xa26ae: the first crate or item on p1's block taken away
            // (0xd7fd0), opened or not; pay and move on whether there was one
            // or not.
            Op::PowerupOff => {
                let Some(d) = self.powerup(p1) else {
                    return Step::Wait("POWERUP_OFF of a declaration that is not a POWERUP");
                };
                let [bx, by, _] = self.decls[d].pos;
                w.powerup_off(bx, by);
                self.paid(slot, w)
            }
            // 0x881a0. Arriving, the state becomes p4 + 5 (0 when p4 is not
            // above 0). Nothing left on p1's block (taken, or turned off): pay
            // and take p2. Still there: with p4 above 0 the state counts down,
            // and p3 is taken when it reaches 5, p4 frames after arriving
            // (that frame included); with none, p3 at once. A kind 15 also
            // sounds 0x16870(12) then: the sound is M8's.
            Op::IsPowerupDone => {
                let Some(d) = self.powerup(p1) else {
                    return Step::Wait("IS_POWERUP_DONE of a declaration that is not a POWERUP");
                };
                if self.procs[slot].state == 1 {
                    self.procs[slot].state = if p4 > 0 { (p4 + 5) as u16 } else { 0 };
                }
                let [bx, by, _] = self.decls[d].pos;
                if !w.powerup_there(bx, by) {
                    return self.paid(slot, w);
                }
                if p4 <= 0 {
                    return self.failure(slot);
                }
                // The state is a signed word there.
                let state = self.procs[slot].state as i16;
                if state > 5 {
                    self.procs[slot].state -= 1;
                    if state - 1 == 5 {
                        return self.failure(slot);
                    }
                }
                Step::Stay
            }
            // 0xa293f and 0xa298f: p1's door turned on (0xe8800) or off
            // (0xe87a0), then pay and move on. While the door is opening or
            // closing it is refused and the process stays on the command.
            Op::DoorOn | Op::DoorOff => {
                let d = &self.decls[self.decl_index(p1)];
                if d.kind != Some(DeclKind::Door) {
                    return Step::Wait("a door or gate of a kind whose loader is not read");
                }
                let Some(door) = usize::try_from(d.handle)
                    .ok()
                    .and_then(|i| self.doors.get_mut(i))
                else {
                    return Step::Wait("a DOOR that got no record");
                };
                if matches!(door.state, DoorState::Opening | DoorState::Closing) {
                    return Step::Stay;
                }
                door.on = op == Op::DoorOn;
                self.paid(slot, w)
            }
            // 0x7d400.
            Op::Mphone => self.mphone(p, slot, ci, w),
            // 0xa7172: the loop and clean-ups above, then pay and move on.
            Op::Reset => {
                self.end_others(slot);
                self.clean_up();
                self.log.push("mission: RESET".into());
                self.paid(slot, w)
            }
            // 0xa583b: is p1's position inside the player's view? Then pay
            // and take p2, otherwise p3. GTA8.EXE tests it in pixels against
            // the rectangle in his player record (0xcd760), whose making is
            // not read; gfa-sim's view stands in, by blocks.
            Op::GeneralOnscreen => {
                let d = &self.decls[self.decl_index(p1)];
                if d.kind.is_some_and(moves) {
                    return Step::Wait("GENERAL_ONSCREEN of something that moves: not ported");
                }
                let [x, y, _] = original_pixels(d);
                if w.onscreen(x >> 6, y >> 6) {
                    self.paid(slot, w)
                } else {
                    self.failure(slot)
                }
            }
            _ => Step::Wait("not ported yet"),
        }
    }

    /// `MPHONE` (0x7d400): p3 phones, labelled from p1 on, ring until he
    /// answers one or goes out of reach. It never pays.
    fn mphone(&mut self, p: [i32; 5], slot: usize, ci: usize, w: &mut dyn ScriptWorld) -> Step {
        let [first, _, count, _, _] = p;
        let phones: Vec<usize> = (0..count.max(0))
            .map(|i| self.decl_index(first + i))
            .collect();
        if phones
            .iter()
            .any(|&d| self.decls[d].kind != Some(DeclKind::Telephone))
        {
            return Step::Wait(
                "MPHONE of a declaration that is not a TELEPHONE: the original reads it as one",
            );
        }
        // Nothing at all while the phone lock is on.
        if self.phone_lock {
            return Step::Stay;
        }
        // Arriving: the phones start ringing, unless another MPHONE's are.
        if self.procs[slot].state == 1 {
            self.procs[slot].state = 0;
            if !self.ringing {
                self.ringing = true;
                self.cmds[ci].p[3] = RING_START;
                self.phones_frame(&phones, 3);
                self.log.push(format!(
                    "mission: phones {first} to {} ring",
                    first + count - 1
                ));
            }
        }
        self.cmds[ci].p[3] -= 1;
        // More than 15 blocks from the first phone: they stop, and the
        // process frees its slot without re-arming its trigger.
        let [fx, fy, _] = original_pixels(&self.decls[self.decl_index(first)]);
        let (px, py, _) = w.player_pos();
        if !near([fx >> 6, fy >> 6, 0], MPHONE_REACH, px >> 21, py >> 21) {
            self.phones_frame(&phones, 1);
            self.ringing = false;
            self.procs[slot].in_use = false;
            self.log
                .push("mission: he is out of the phones' reach; they stop ringing".into());
            return Step::Moved;
        }
        // Each phone in turn: a ring at 0 and at -10, which rocks it between
        // its frames 2 and 3 (0xf1dd0), and the answer: he stands on foot
        // within 16 of the original's pixels of it.
        let ring = self.cmds[ci].p[3];
        let (hx, hy) = (px >> 15, py >> 15);
        let mut rang = None;
        for (i, &d) in phones.iter().enumerate() {
            let handle = self.decls[d].handle;
            if handle > -1 && (ring == 0 || ring == RING_SECOND) {
                rang = Some(ring);
                if let Some(p) = self.phone_mut(d) {
                    p.frame = match p.frame {
                        2 => 3,
                        3 => 2,
                        f => f,
                    };
                }
            }
            let [x, y, _] = original_pixels(&self.decls[d]);
            let at = (x - hx).abs() < ANSWER_REACH_PX && (y - hy).abs() < ANSWER_REACH_PX;
            if at && handle > -1 && !self.phone_lock && w.player_on_foot() {
                self.phones_frame(&phones, 1);
                return self.answer(slot, ci, i, d);
            }
        }
        match rang {
            Some(0) => self.cmds[ci].p[3] = -1,
            Some(_) => self.cmds[ci].p[3] = RING_START,
            None => {}
        }
        Step::Stay
    }

    /// 0x7d700: he answered the `i`th phone, `d`. Every other process ends
    /// and the clean-ups run, as for `RESET`. The process moves on by p2,
    /// then `i` commands further, so each phone has its own line. That phone
    /// rings no more, and the phone lock is on for 250 frames.
    fn answer(&mut self, slot: usize, ci: usize, i: usize, d: usize) -> Step {
        self.end_others(slot);
        self.clean_up();
        let s = self.advance(slot, self.cmds[ci].p[1]);
        if self.procs[slot].in_use {
            self.procs[slot].cmd += i;
        }
        self.decls[d].handle = -1;
        self.ringing = false;
        self.schedule(PHONE_LOCK_FRAMES, Event::PhoneLockOver);
        self.phone_lock = true;
        self.log
            .push(format!("mission: phone {} answered", self.decls[d].label));
        s
    }

    /// Everything that decides later frames.
    pub fn sim_hash(&self, h: &mut StateHasher) {
        h.u32(self.frame.phase());
        h.bool(self.started);
        for p in &self.procs {
            h.bool(p.in_use);
            if p.in_use {
                h.u32(p.cmd as u32);
                h.u32(u32::from(p.state));
                h.i32(p.trigger);
                h.i32(p.parent);
            }
            h.i32(p.keep);
        }
        for t in &self.triggers {
            h.u8(t.state as u8);
            h.i32(t.kill);
        }
        for d in &self.decls {
            h.i32(d.handle);
        }
        for c in &self.cmds {
            for v in c.p {
                h.i32(v);
            }
        }
        for r in &self.kicks {
            for v in r {
                h.i32(*v);
            }
        }
        match self.arrow {
            Some((x, y, z)) => {
                h.bool(true);
                h.i32(x);
                h.i32(y);
                h.i32(z);
            }
            None => h.bool(false),
        }
        for d in &self.doors {
            h.u8(d.state as u8);
            h.bool(d.on);
        }
        for p in &self.phones {
            h.u8(p.frame);
        }
        h.bool(self.ringing);
        h.bool(self.phone_lock);
        h.u32(self.clock);
        for &(due, e) in &self.events {
            h.u32(due);
            h.u8(e as u8);
        }
    }
}

/// Is block `(bx, by)` within `r` blocks of `block` on both axes, edges
/// included?
fn near(block: [i32; 3], r: i32, bx: i32, by: i32) -> bool {
    let [x, y, _] = block;
    (x - r..=x + r).contains(&bx) && (y - r..=y + r).contains(&by)
}

/// The trigger type a declaration kind's loader makes (0xe8340's `type`),
/// read from each case of the switch at 0x798ea and the helpers it calls.
/// `CARWAIT_TRIG` picks its type from the declaration it watches (0x79030),
/// not read yet; it is given 0xff.
fn trigger_type(k: DeclKind) -> Option<u8> {
    use DeclKind as K;
    Some(match k {
        K::Spray => 3,
        K::Trigger => 4,
        K::Bombshop => 8,
        K::Cartrigger => 9,
        K::Onetrigger => 10,
        K::MovingTrig => 11,
        K::Mphones => 12,
        K::PhoneTogg => 13,
        K::DumMissionTrig => 14,
        K::CorrectModTrig => 15,
        K::CorrectCarTrig => 16,
        K::ClockStart => 17,
        K::ClockStop => 18,
        K::MidpointMulti => 19,
        K::FinalMulti => 20,
        K::DumPedBlockTrig => 21,
        K::MovingTrigHired => 22,
        K::CarbombTrig => 24,
        K::DamageTrig => 25,
        K::GunTrig => 26,
        K::GunScreenTrig => 27,
        K::CardestroyTrig => 28,
        K::PedcarTrig => 30,
        K::CannonStart => 31,
        K::CarstuckTrig => 32,
        K::AltDamageTrig => 33,
        K::CarwaitTrig => 0xff,
        _ => return None,
    })
}

/// Kinds `ENABLE` runs a routine of their own for (0x7aa60) before, or in
/// place of, re-arming: the doors and gates and the watch triggers. Not
/// ported; every other kind's case returns at once.
fn enable_has_routine(k: DeclKind) -> bool {
    use DeclKind as K;
    let n = k.code();
    matches!(
        k,
        K::Door
            | K::Barrier
            | K::Onetrigger
            | K::MovingTrig
            | K::SpecificBarr
            | K::ModelDoor
            | K::SpecificDoor
            | K::DumMissionTrig
            | K::CorrectCarTrig
            | K::CardestroyTrig
            | K::CarwaitTrig
            | K::PedcarTrig
            | K::DumPedBlockTrig
            | K::CarstuckTrig
            | K::ModelBarrier
            | K::SpecificDoorBomb
    ) || (45..=58).contains(&n)
}

/// Kinds whose position is a thing that moves (a car, a person), which the
/// original reads live. Not ported.
fn moves(k: DeclKind) -> bool {
    use DeclKind as K;
    matches!(
        k,
        K::Car
            | K::Ped
            | K::Player
            | K::Driver
            | K::Parked
            | K::Model
            | K::Dummy
            | K::Futureped
            | K::Futurecar
            | K::Hells
            | K::DriverPark
            | K::Escort
            | K::Escorted
            | K::Stopped
            | K::Carbomb
            | K::MyModel
            | K::ParkedPixels
    )
}

/// A declaration's position in the original's pixels (64 to a block):
/// most kinds are written in blocks; these few in pixels (AmiGTA1's
/// `gta_script_is_block`). A `TELEPHONE`'s the loader moves to its block's
/// centre (0x76da0), and the position reader (0x7acf0) returns that.
fn original_pixels(d: &Decl) -> [i32; 3] {
    use DeclKind as K;
    if d.kind == Some(K::Telephone) {
        let [x, y, z] = d.pos;
        return [x * 64 + 32, y * 64 + 32, z * 64];
    }
    let pixels = matches!(
        d.kind,
        Some(
            K::Crane
                | K::Target
                | K::Futureped
                | K::Ped
                | K::Object
                | K::Future
                | K::ParkedPixels
                | K::ChopperEndpoint
        )
    );
    if pixels { d.pos } else { d.pos.map(|v| v * 64) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A world with nothing in it but the player's block (he stands at its
    /// centre, or at `at` in the original's pixels), his car, his score, what
    /// is on screen and the crates: `(block, kind, amount)`.
    #[derive(Default)]
    struct Fake {
        block: Option<(i32, i32)>,
        at: Option<(i32, i32)>,
        car: Option<u8>,
        paid: Vec<i32>,
        target: bool,
        shown: Vec<(i32, i32)>,
        crates: Vec<([i32; 3], i32, i32)>,
    }

    impl ScriptWorld for Fake {
        fn player_pos(&self) -> (i32, i32, i32) {
            if let Some((x, y)) = self.at {
                return (x << 15, y << 15, 2);
            }
            let (x, y) = self.block.unwrap_or_default();
            ((x << 21) + (1 << 20), (y << 21) + (1 << 20), 2)
        }
        fn player_on_foot(&self) -> bool {
            self.car.is_none()
        }
        fn onscreen(&self, bx: i32, by: i32) -> bool {
            self.shown.contains(&(bx, by))
        }
        fn trigger_block(&self) -> Option<(i32, i32)> {
            match self.at {
                Some((x, y)) => Some((x >> 6, y >> 6)),
                None => self.block,
            }
        }
        fn trigger_car_model(&self) -> Option<u8> {
            self.car
        }
        fn pay(&mut self, points: i32) {
            self.paid.push(points);
        }
        fn target_reached(&self) -> bool {
            self.target
        }
        fn powerup_on(&mut self, block: [i32; 3], kind: i32, amount: i32) -> bool {
            self.crates.push((block, kind, amount));
            true
        }
        fn powerup_off(&mut self, bx: i32, by: i32) -> bool {
            let at = self.crates.iter().position(|c| c.0[..2] == [bx, by]);
            at.map(|i| self.crates.remove(i)).is_some()
        }
        fn powerup_there(&self, bx: i32, by: i32) -> bool {
            self.crates.iter().any(|c| c.0[..2] == [bx, by])
        }
    }

    fn vm(script: &str) -> Vm {
        let text = format!("[1]\nT, 22, nyc.cmp, 0,\n1 1\n{script}");
        let s = gfa_formats::mission::parse(text.as_bytes()).unwrap();
        Vm::new(&s[0])
    }

    /// Run game frames until `n` of them have passed.
    fn frames(vm: &mut Vm, w: &mut Fake, n: usize) {
        for _ in 0..n {
            vm.run_frame(w);
        }
    }

    /// The script's processes, past the player's own slot.
    fn jobs(vm: &Vm) -> Vec<(usize, i32, &str)> {
        let mut r = vm.running();
        r.retain(|j| j.0 >= FIRST_SCRIPT_SLOT);
        r
    }

    #[test]
    fn a_trigger_starts_its_line_in_slot_four_and_survive_lasts_p4_plus_one_frames() {
        let mut v = vm("1 1 (10,10,4) TRIGGER 100 0\n-1\n0 DONOWT 0 0 0 0 0\n\
            100 SURVIVE 0 0 0 3 250\n101 DONOWT 0 0 0 0 0\n-1\n");
        let mut w = Fake {
            block: Some((11, 10)),
            ..Fake::default()
        };
        frames(&mut v, &mut w, 2);
        assert!(jobs(&v).is_empty(), "radius 0 is the block itself");
        w.block = Some((10, 10));
        // Frame 1: fired, and SURVIVE loads its count; frames 2, 3, 4 count it
        // down; the fourth reaches 0, pays and moves on.
        frames(&mut v, &mut w, 3);
        assert_eq!(jobs(&v), vec![(4, 100, "SURVIVE")]);
        assert_eq!(v.cmds[1].p[2], 1, "the count is the command's own p3");
        frames(&mut v, &mut w, 1);
        assert_eq!(jobs(&v), vec![(4, 101, "DONOWT")]);
        assert_eq!(w.paid, vec![250], "SURVIVE pays p5");
        assert_eq!(v.triggers[0].state, TriggerState::Started);
    }

    #[test]
    fn ending_rearms_the_trigger_unless_disable_made_the_process_forget_it() {
        let mut v = vm(
            "1 1 (5,5,4) TRIGGER 100 1\n2 1 (50,50,4) TRIGGER 200 0\n-1\n\
            0 DONOWT 0 0 0 0 0\n100 SURVIVE 0 -1 0 0 0\n200 DISABLE 2 -1 0 0 0\n-1\n",
        );
        let mut w = Fake {
            block: Some((6, 4)),
            ..Fake::default()
        };
        frames(&mut v, &mut w, 2);
        assert!(jobs(&v).is_empty(), "SURVIVE 0 ends on its second frame");
        assert_eq!(v.triggers[0].state, TriggerState::Armed, "re-armed by -1");
        w.block = Some((50, 50));
        frames(&mut v, &mut w, 1);
        assert!(jobs(&v).is_empty());
        assert_eq!(
            v.triggers[1].state,
            TriggerState::Disabled,
            "and not re-armed"
        );
    }

    #[test]
    fn a_missing_label_lands_on_command_zero_and_an_unported_one_waits_logged() {
        let mut v = vm("1 1 (5,5,4) TRIGGER 100 0\n-1\n0 DONOWT 0 0 0 0 0\n\
            100 COMPARE 1 999 0 0 0\n-1\n");
        // The trigger's handle is 0, which equals p4: success, to label 999.
        let mut w = Fake {
            block: Some((5, 5)),
            ..Fake::default()
        };
        frames(&mut v, &mut w, 1);
        assert_eq!(
            jobs(&v),
            vec![(4, 0, "DONOWT")],
            "999 is no label: command 0"
        );

        let mut v = vm("1 1 (5,5,4) TRIGGER 100 0\n-1\n0 DONOWT 0 0 0 0 0\n\
            100 CRANE 0 0 0 0 0\n-1\n");
        frames(&mut v, &mut w, 3);
        assert_eq!(jobs(&v), vec![(4, 100, "CRANE")], "it waits on it");
        let log = v.drain_log();
        assert_eq!(
            log.iter().filter(|l| l.contains("waits")).count(),
            1,
            "{log:?}"
        );
    }

    #[test]
    fn counters_compare_and_pay_on_success() {
        let mut v = vm(
            "1 1 (5,5,4) TRIGGER 100 0\n7 (0,0,0) COUNTER 2 0\n-1\n0 DONOWT 0 0 0 0 0\n\
            100 INCCOUNT 7 0 110 3 40\n101 DECCOUNT 7 0 120 0 0\n102 COMPARE 7 0 130 2 7\n\
            103 DONOWT 0 0 0 0 0\n110 DONOWT 0 0 0 0 0\n120 DONOWT 0 0 0 0 0\n\
            130 DONOWT 0 0 0 0 0\n-1\n",
        );
        let mut w = Fake {
            block: Some((5, 5)),
            ..Fake::default()
        };
        frames(&mut v, &mut w, 3);
        // 2 + 1 = 3 reaches p4: pay 40, p2; 3 - 1 = 2 > 0: p3 (120).
        assert_eq!(jobs(&v), vec![(4, 120, "DONOWT")]);
        assert_eq!(w.paid, vec![40]);
        assert_eq!(v.decls[1].handle, 2);
    }

    #[test]
    fn kickstart_records_its_child_and_kill_side_proc_ends_it() {
        let mut v = vm("1 1 (5,5,4) TRIGGER 100 0\n-1\n0 DONOWT 0 0 0 0 0\n\
            100 KICKSTART 0 102 0 -1 0\n101 DONOWT 0 0 0 0 0\n102 SURVIVE 0 0 0 2 0\n\
            103 KILL_SIDE_PROC 100 0 0 0 0\n104 DONOWT 0 0 0 0 0\n-1\n");
        let mut w = Fake {
            block: Some((5, 5)),
            ..Fake::default()
        };
        frames(&mut v, &mut w, 1);
        // p1 0: the child starts at the next command (101) in slot 5, and runs
        // in this frame because its slot comes later.
        assert_eq!(jobs(&v), vec![(4, 102, "SURVIVE"), (5, 101, "DONOWT")]);
        assert_eq!(
            v.procs[5].parent, 0,
            "the child's parent is the root player"
        );
        assert_eq!((v.cmds[1].p[3], v.cmds[1].p[4]), (0, 4), "record 0, slot 4");
        frames(&mut v, &mut w, 4);
        assert_eq!(jobs(&v), vec![(4, 104, "DONOWT")], "the child is gone");
    }

    #[test]
    fn startup_runs_once_and_the_target_ends_everything() {
        let mut v = vm("1 1 (5,5,4) TRIGGER 100 0\n2 1 (9,9,4) TRIGGER 100 0\n-1\n\
            0 DONOWT 0 0 0 0 0\n100 STARTUP 1 0 0 0 0\n101 MOBILE_BRIEF 0 0 0 0 1001\n\
            102 DONOWT 0 0 0 0 0\n-1\n");
        let mut w = Fake {
            block: Some((5, 5)),
            ..Fake::default()
        };
        frames(&mut v, &mut w, 2);
        assert_eq!(v.drain_briefs(), vec![Brief::Mobile(1001)]);
        w.block = Some((9, 9));
        frames(&mut v, &mut w, 2);
        assert_eq!(jobs(&v), vec![(4, 102, "DONOWT"), (5, 100, "STARTUP")]);
        w.target = true;
        frames(&mut v, &mut w, 1);
        assert!(v.running().is_empty(), "the player's slot too");
    }
    #[test]
    fn the_players_slot_runs_from_command_zero() {
        // As in the probe arena (`weapons_ammo`): nothing triggers these.
        let mut v = vm(
            "3 (126,128,4) POWERUP 2 20\n4 (125,128,4) POWERUP 4 10\n-1\n\
            0 POWERUP_ON 3 0 0 0 0\n1 POWERUP_ON 4 0 0 0 0\n2 DONOWT 0 0 0 0 0\n-1\n",
        );
        let mut w = Fake::default();
        assert_eq!(v.running(), vec![(0, 0, "POWERUP_ON")]);
        assert_eq!((v.procs[0].trigger, v.procs[0].parent), (0, -1));
        frames(&mut v, &mut w, 2);
        assert_eq!(v.running(), vec![(0, 2, "DONOWT")]);
        assert_eq!(
            w.crates,
            vec![([126, 128, 4], 2, 20), ([125, 128, 4], 4, 10)]
        );
        assert!(w.paid.is_empty(), "POWERUP_ON does not pay");
    }

    #[test]
    fn powerup_off_takes_the_crate_away_and_pays() {
        let mut v = vm("3 (126,128,4) POWERUP 15 1\n-1\n0 POWERUP_ON 3 0 0 0 0\n\
            1 POWERUP_OFF 3 0 0 0 500\n2 POWERUP_OFF 3 0 0 0 500\n3 DONOWT 0 0 0 0 0\n-1\n");
        let mut w = Fake::default();
        frames(&mut v, &mut w, 2);
        assert!(w.crates.is_empty());
        frames(&mut v, &mut w, 1);
        assert_eq!(
            w.paid,
            vec![500, 500],
            "it pays whether or not one was there"
        );
        assert_eq!(v.running(), vec![(0, 3, "DONOWT")]);
    }

    #[test]
    fn is_powerup_done_waits_p4_frames_or_none() {
        let script = |p4: i32| {
            format!(
                "3 (126,128,4) POWERUP 2 20\n-1\n0 POWERUP_ON 3 0 0 0 0\n\
                 1 IS_POWERUP_DONE 3 10 20 {p4} 90\n2 DONOWT 0 0 0 0 0\n\
                 10 DONOWT 0 0 0 0 0\n20 DONOWT 0 0 0 0 0\n-1\n"
            )
        };
        // Still there: p3 on the third frame on it.
        let mut v = vm(&script(3));
        let mut w = Fake::default();
        frames(&mut v, &mut w, 3);
        assert_eq!(v.running(), vec![(0, 1, "IS_POWERUP_DONE")]);
        frames(&mut v, &mut w, 1);
        assert_eq!(v.running(), vec![(0, 20, "DONOWT")]);
        assert!(w.paid.is_empty());

        // With p4 0 it looks once.
        let mut v = vm(&script(0));
        let mut w = Fake::default();
        frames(&mut v, &mut w, 2);
        assert_eq!(v.running(), vec![(0, 20, "DONOWT")]);

        // Taken while it waits: pay p5 and take p2.
        let mut v = vm(&script(3));
        let mut w = Fake::default();
        frames(&mut v, &mut w, 2);
        w.crates.clear();
        frames(&mut v, &mut w, 1);
        assert_eq!(v.running(), vec![(0, 10, "DONOWT")]);
        assert_eq!(w.paid, vec![90]);
    }

    #[test]
    fn door_on_turns_the_door_on_and_reset_puts_out_what_has_no_flag() {
        let mut v = vm("5 1 (10,10,3) DOOR 0 11 7\n6 (20,20,3) DOOR 0 11 7\n\
            7 (5,5,4) TRIGGER 100 0\n8 1 (6,6,4) TRIGGER 100 0\n9 (30,30,3) BARRIER 0 0\n-1\n\
            0 DOOR_ON 5 0 0 0 30\n1 DOOR_ON 6 0 0 0 0\n2 RESET 0 0 0 0 0\n\
            3 DOOR_ON 9 0 0 0 0\n-1\n");
        let mut w = Fake::default();
        assert_eq!(v.doors.len(), 2);
        assert!(v.doors.iter().all(|d| d.state == DoorState::Shut && !d.on));
        frames(&mut v, &mut w, 2);
        assert!(v.doors[0].on && v.doors[1].on);
        assert_eq!(w.paid, vec![30], "DOOR_ON pays p5");
        frames(&mut v, &mut w, 1);
        assert!(v.doors[0].on, "a door with its flag stays on");
        assert!(!v.doors[1].on, "one without is turned off");
        assert_eq!(v.triggers[0].state, TriggerState::Disabled);
        assert_eq!(v.triggers[1].state, TriggerState::Armed);
        frames(&mut v, &mut w, 2);
        assert_eq!(v.running(), vec![(0, 3, "DOOR_ON")], "a BARRIER waits");
    }

    #[test]
    fn door_on_waits_while_the_door_moves() {
        let mut v = vm("5 1 (10,10,3) DOOR 0 11 7\n-1\n\
            0 DOOR_OFF 5 0 0 0 0\n1 DONOWT 0 0 0 0 0\n-1\n");
        let mut w = Fake::default();
        v.doors[0].state = DoorState::Closing;
        v.doors[0].on = true;
        frames(&mut v, &mut w, 3);
        assert_eq!(v.running(), vec![(0, 0, "DOOR_OFF")]);
        v.doors[0].state = DoorState::Open;
        frames(&mut v, &mut w, 1);
        assert_eq!(v.running(), vec![(0, 1, "DONOWT")]);
        assert!(!v.doors[0].on);
    }

    /// Two phones on (50,50) and (51,50), their MPHONES trigger (radius 3)
    /// and its PHONE_TOGG (radius 5); a side process from a TRIGGER.
    const PHONES: &str = "10 1 (50,50,4) TELEPHONE 0 0\n11 1 (51,50,4) TELEPHONE 0 256\n\
        20 1 (50,50,4) MPHONES 100 3\n21 1 (50,50,4) PHONE_TOGG 20 5\n\
        22 1 (53,50,4) TRIGGER 200 0\n-1\n\
        0 DONOWT 0 0 0 0 0\n100 MPHONE 10 0 2 30 100\n101 SURVIVE 0 0 0 1000 0\n\
        102 SURVIVE 0 0 0 1000 0\n200 DONOWT 0 0 0 0 0\n-1\n";

    #[test]
    fn the_phone_rings_and_answering_takes_that_phones_line() {
        let mut v = vm(PHONES);
        assert_eq!(v.decls[1].second, -1, "a TELEPHONE's first number, if above 0");
        assert_eq!(v.triggers[1].line, 0, "the PHONE_TOGG names trigger 0");
        let mut w = Fake {
            block: Some((53, 50)),
            ..Fake::default()
        };
        frames(&mut v, &mut w, 1);
        assert_eq!(jobs(&v), vec![(4, 100, "MPHONE"), (5, 200, "DONOWT")]);
        assert!(v.ringing);
        assert_eq!(v.triggers[0].state, TriggerState::Started);
        // He walks onto the second phone, within 16 pixels of its centre.
        w.at = Some((51 * 64 + 32 + 15, 50 * 64 + 32 - 15));
        frames(&mut v, &mut w, 1);
        assert_eq!(jobs(&v), vec![(4, 102, "SURVIVE")], "its line; 200 ended");
        assert_eq!(v.triggers[2].state, TriggerState::Armed, "re-armed by the end");
        assert!(!v.ringing && v.phone_lock);
        assert_eq!(v.decls[1].handle, -1, "that phone rings no more");
        assert!(w.paid.is_empty(), "MPHONE never pays");
        // The lock lasts 250 frames.
        frames(&mut v, &mut w, 250);
        assert!(v.phone_lock);
        frames(&mut v, &mut w, 1);
        assert!(!v.phone_lock);
    }

    #[test]
    fn the_phone_rings_twice_every_36_frames_and_stops_out_of_reach() {
        let mut v = vm(PHONES);
        let mut w = Fake {
            block: Some((50, 53)),
            ..Fake::default()
        };
        frames(&mut v, &mut w, 25);
        assert_eq!(v.cmds[1].p[3], -1, "the first ring, at 0");
        frames(&mut v, &mut w, 9);
        assert_eq!(v.cmds[1].p[3], 25, "the second, at -10");
        w.block = Some((66, 50));
        frames(&mut v, &mut w, 1);
        assert!(jobs(&v).iter().all(|j| j.2 != "MPHONE"), "the slot is free");
        assert!(!v.ringing);
        assert_eq!(
            v.triggers[0].state,
            TriggerState::Started,
            "and its trigger is not re-armed"
        );
        // The PHONE_TOGG re-arms it between 3 and 5 blocks out.
        w.block = Some((54, 50));
        frames(&mut v, &mut w, 1);
        assert_eq!(v.triggers[0].state, TriggerState::Armed);
    }

    #[test]
    fn mphones_waits_out_the_lock_and_a_driver_of_model_47() {
        let mut v = vm(PHONES);
        // Near the phones, but not on one.
        let mut w = Fake {
            block: Some((50, 52)),
            car: Some(47),
            ..Fake::default()
        };
        frames(&mut v, &mut w, 2);
        assert!(jobs(&v).iter().all(|j| j.1 != 100));
        w.car = None;
        v.phone_lock = true;
        frames(&mut v, &mut w, 1);
        assert_eq!(v.triggers[0].state, TriggerState::Firing, "it waits firing");
        frames(&mut v, &mut w, 1);
        assert_eq!(v.triggers[0].state, TriggerState::Armed, "put back");
        v.phone_lock = false;
        frames(&mut v, &mut w, 1);
        assert_eq!(v.triggers[0].state, TriggerState::Started);
        assert!(jobs(&v).iter().any(|j| j.1 == 100));
    }

    #[test]
    fn general_onscreen_takes_p2_with_pay_or_p3() {
        // As Liberty City's opening has it: p3 is its own label.
        let mut v = vm("3 (100,100,4) TELEPHONE 0 0\n-1\n\
            5 GENERAL_ONSCREEN 3 0 5 0 40\n6 DONOWT 0 0 0 0 0\n-1\n");
        let mut w = Fake::default();
        frames(&mut v, &mut w, 3);
        assert_eq!(v.running(), vec![(0, 5, "GENERAL_ONSCREEN")]);
        assert!(w.paid.is_empty());
        w.shown.push((100, 100));
        frames(&mut v, &mut w, 1);
        assert_eq!(v.running(), vec![(0, 6, "DONOWT")]);
        assert_eq!(w.paid, vec![40]);
    }

    #[test]
    fn powerup_on_of_another_kind_waits() {
        // Label 9 is missing: record 0, a TRIGGER, is read, as GTA8.EXE does.
        let mut v = vm("1 1 (5,5,4) TRIGGER 100 0\n-1\n0 POWERUP_ON 9 0 0 0 0\n-1\n");
        let mut w = Fake::default();
        frames(&mut v, &mut w, 2);
        assert_eq!(v.running(), vec![(0, 0, "POWERUP_ON")]);
        assert!(w.crates.is_empty());
        assert!(v.drain_log().iter().any(|l| l.contains("not a POWERUP")));
    }
}
