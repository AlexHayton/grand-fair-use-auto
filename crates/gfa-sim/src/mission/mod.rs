//! The mission VM (M7): the original's name tables, what gfa-sim does of
//! each, and the interpreter itself ([`vm`]), ported from GTA8.EXE's own
//! (`docs/MISSION_VM.md`).
//!
//! A mission section's declarations are things in the world (`PARKED`,
//! `TRIGGER`, `DOOR`, …), its commands the steps of the jobs (`BRIEF`,
//! `CAR_ON`, `SURVIVE`, …). GTA8.EXE looks each name up in a table of
//! string pointers and keeps its index (GTA8.EXE, read with
//! `tools/dosprobe/gta8.py`: docs/MISSION_OPCODES.md):
//!
//! - **Declarations**: 72 kinds from 0x132828. A kind not in the table stops
//!   the game while it loads the section (error -171, then exit).
//! - **Commands**: 150 opcodes from 0x13294c. Each line is read as its
//!   label, the name and exactly five numbers into a 22-byte record. A name
//!   not in the table loads as opcode 0xFFFF, and the game stops when a
//!   process reaches it (error -182 with the line's label, then exit). So an
//!   unknown command never succeeds; the VM keeps that.
//! - `DONOWT`, `DRIVEON` and the two `XXXX` entries share one empty case.
//!
//! [`Op`] and [`DeclKind`] keep the original's numbers. GTA8.EXE's live
//! interpreter is the reference for what each does; AmiGTA1 (MIT, its
//! `gta_script.c` and `gta_script_run.c`) names every declaration kind and
//! command that GTA's single-player sections use and is the guide to what
//! each is for. London's executables carry a few more commands of their own
//! (M11). `gfa-tool mission coverage` reports what the scripts use and
//! what of it gfa-sim does.

pub mod vm;

pub use vm::Vm;

/// Names in the original's order: `enum`, its `ALL` table, `name` and the
/// lookup by name, which takes the first match as the original's search does.
macro_rules! name_table {
    ($(#[$meta:meta])* $ty:ident { $($variant:ident = $name:literal,)* }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(u8)]
        pub enum $ty {
            $($variant,)*
        }

        impl $ty {
            /// Every entry, in the original's order: `ALL[n].code() == n`.
            pub const ALL: &[$ty] = &[$($ty::$variant,)*];

            /// The original's number.
            pub const fn code(self) -> u8 {
                self as u8
            }

            /// As the script writes it.
            pub const fn name(self) -> &'static str {
                match self {
                    $($ty::$variant => $name,)*
                }
            }

            /// The entry the script's word names, if the original has it.
            pub fn from_name(word: &str) -> Option<$ty> {
                Self::ALL.iter().copied().find(|t| t.name() == word)
            }
        }
    };
}

name_table! {
    /// A command: the original's opcode (GTA8.EXE, the table at 0x13294c).
    Op {
        LocateStopped = "LOCATE_STOPPED",
        Destroy = "DESTROY",
        Answer = "ANSWER",
        Steal = "STEAL",
        Survive = "SURVIVE",
        Hunton = "HUNTON",
        Huntoff = "HUNTOFF",
        Driveon = "DRIVEON",
        Dummyon = "DUMMYON",
        Sendto = "SENDTO",
        End = "END",
        Makeobj = "MAKEOBJ",
        Explode = "EXPLODE",
        Obtain = "OBTAIN",
        Throw = "THROW",
        Brief = "BRIEF",
        Mphone = "MPHONE",
        Donowt = "DONOWT",
        Disable = "DISABLE",
        Enable = "ENABLE",
        Deccount = "DECCOUNT",
        Goto = "GOTO",
        Crane = "CRANE",
        Park = "PARK",
        Setbomb = "SETBOMB",
        Escort = "ESCORT",
        Arrow = "ARROW",
        LockDoor = "LOCK_DOOR",
        UnlockDoor = "UNLOCK_DOOR",
        ExplLast = "EXPL_LAST",
        DropOn = "DROP_ON",
        PedOn = "PED_ON",
        KillDrop = "KILL_DROP",
        CarOn = "CAR_ON",
        Armedmess = "ARMEDMESS",
        Disarmmess = "DISARMMESS",
        ArrowOff = "ARROW_OFF",
        PBrief = "P_BRIEF",
        PedSendto = "PED_SENDTO",
        WaitForPed = "WAIT_FOR_PED",
        PedBack = "PED_BACK",
        SetupRepo = "SETUP_REPO",
        DoRepo = "DO_REPO",
        KillObj = "KILL_OBJ",
        DoGta = "DO_GTA",
        MissionEnd = "MISSION_END",
        ExplodeCar = "EXPLODE_CAR",
        StartModel = "START_MODEL",
        DoModel = "DO_MODEL",
        ReturnControl = "RETURN_CONTROL",
        KillCar = "KILL_CAR",
        Arrowped = "ARROWPED",
        Arrowcar = "ARROWCAR",
        RemapPed = "REMAP_PED",
        RemapCar = "REMAP_CAR",
        AreBothOnscreen = "ARE_BOTH_ONSCREEN",
        ThrowToPoint = "THROW_TO_POINT",
        GotoDropoff = "GOTO_DROPOFF",
        ModelHunt = "MODEL_HUNT",
        ModelFuture = "MODEL_FUTURE",
        Startup = "STARTUP",
        CheckPedback = "CHECK_PEDBACK",
        StartHiredEsc = "START_HIRED_ESC",
        DummyOff = "DUMMY_OFF",
        PowerupOn = "POWERUP_ON",
        PlainExplBuilding = "PLAIN_EXPL_BUILDING",
        ChangeBlock = "CHANGE_BLOCK",
        ChangeType = "CHANGE_TYPE",
        PBriefTimed = "P_BRIEF_TIMED",
        ChangePedType = "CHANGE_PED_TYPE",
        DoorOn = "DOOR_ON",
        DoorOff = "DOOR_OFF",
        WreckCurrTrain = "WRECK_CURR_TRAIN",
        OpenDoor = "OPEN_DOOR",
        CloseDoor = "CLOSE_DOOR",
        PedOutOfCar = "PED_OUT_OF_CAR",
        BankRobbery = "BANK_ROBBERY",
        DelayCrime = "DELAY_CRIME",
        GetDriverInfo = "GET_DRIVER_INFO",
        KillPed = "KILL_PED",
        BankAlarmOn = "BANK_ALARM_ON",
        BankAlarmOff = "BANK_ALARM_OFF",
        GarageSend = "GARAGE_SEND",
        DummyDriveOn = "DUMMY_DRIVE_ON",
        PlayerAreBothOnscreen = "PLAYER_ARE_BOTH_ONSCREEN",
        CheckCar = "CHECK_CAR",
        WaitForPlayer = "WAIT_FOR_PLAYER",
        ExplPed = "EXPL_PED",
        ParkedOn = "PARKED_ON",
        Kickstart = "KICKSTART",
        IsPedInCar = "IS_PED_IN_CAR",
        CancelBriefing = "CANCEL_BRIEFING",
        FreezeTimed = "FREEZE_TIMED",
        FreezeEnter = "FREEZE_ENTER",
        UnfreezeEnter = "UNFREEZE_ENTER",
        ExplNoFire = "EXPL_NO_FIRE",
        KillProcess = "KILL_PROCESS",
        NextKick = "NEXT_KICK",
        KillSideProc = "KILL_SIDE_PROC",
        SetKilltrig = "SET_KILLTRIG",
        PowerupOff = "POWERUP_OFF",
        KillSpecProc = "KILL_SPEC_PROC",
        ScoreCheck = "SCORE_CHECK",
        FrenzySet = "FRENZY_SET",
        FrenzyCheck = "FRENZY_CHECK",
        IsGoalDead = "IS_GOAL_DEAD",
        GeneralOnscreen = "GENERAL_ONSCREEN",
        GetCarInfo = "GET_CAR_INFO",
        StartChopper = "START_CHOPPER",
        Locate = "LOCATE",
        SpeechBrief = "SPEECH_BRIEF",
        MobileBrief = "MOBILE_BRIEF",
        PixelCarOn = "PIXEL_CAR_ON",
        SetNoCollide = "SET_NO_COLLIDE",
        PedWeapon = "PED_WEAPON",
        FreeupCar = "FREEUP_CAR",
        MessageBrief = "MESSAGE_BRIEF",
        ParkedPixelsOn = "PARKED_PIXELS_ON",
        PedPolice = "PED_POLICE",
        DropWantedLevel = "DROP_WANTED_LEVEL",
        IsPedArrested = "IS_PED_ARRESTED",
        HellOn = "HELL_ON",
        Unused122 = "XXXX",
        Unused123 = "XXXX",
        SetPedSpeed = "SET_PED_SPEED",
        IsPlayerOnTrain = "IS_PLAYER_ON_TRAIN",
        WreckATrain = "WRECK_A_TRAIN",
        Inccount = "INCCOUNT",
        Compare = "COMPARE",
        Reset = "RESET",
        KeepThisProc = "KEEP_THIS_PROC",
        SetCollide = "SET_COLLIDE",
        DeadArrested = "DEAD_ARRESTED",
        IsPowerupDone = "IS_POWERUP_DONE",
        StopFrenzy = "STOP_FRENZY",
        FrenzyBrief = "FRENZY_BRIEF",
        AddALife = "ADD_A_LIFE",
        KfBriefTimed = "KF_BRIEF_TIMED",
        KfCancelBriefing = "KF_CANCEL_BRIEFING",
        KfBriefGeneral = "KF_BRIEF_GENERAL",
        KfCancelGeneral = "KF_CANCEL_GENERAL",
        KfProcess = "KF_PROCESS",
        ResetKf = "RESET_KF",
        WaitForPlayers = "WAIT_FOR_PLAYERS",
        RedArrow = "RED_ARROW",
        ResetWithBriefs = "RESET_WITH_BRIEFS",
        RedArrowOff = "RED_ARROW_OFF",
        IsATrainWrecked = "IS_A_TRAIN_WRECKED",
        IncHeads = "INC_HEADS",
        IsPedStunned = "IS_PED_STUNNED",
    }
}

name_table! {
    /// A declaration's kind: the original's number (GTA8.EXE, the table at
    /// 0x132828).
    DeclKind {
        Car = "CAR",
        Ped = "PED",
        Object = "OBJECT",
        Player = "PLAYER",
        Driver = "DRIVER",
        Parked = "PARKED",
        Model = "MODEL",
        Telephone = "TELEPHONE",
        Trigger = "TRIGGER",
        Door = "DOOR",
        Target = "TARGET",
        Future = "FUTURE",
        Counter = "COUNTER",
        Crane = "CRANE",
        Clock = "CLOCK",
        Dummy = "DUMMY",
        Spray = "SPRAY",
        Barrier = "BARRIER",
        Escort = "ESCORT",
        Carbomb = "CARBOMB",
        Bombshop = "BOMBSHOP",
        Escorted = "ESCORTED",
        Stopped = "STOPPED",
        Init = "INIT",
        Futureped = "FUTUREPED",
        Cartrigger = "CARTRIGGER",
        Futuredrop = "FUTUREDROP",
        Hells = "HELLS",
        DriverPark = "DRIVER_PARK",
        Futurecar = "FUTURECAR",
        Onetrigger = "ONETRIGGER",
        ModelBarrier = "MODEL_BARRIER",
        MovingTrig = "MOVING_TRIG",
        SpecificBarr = "SPECIFIC_BARR",
        Mphones = "MPHONES",
        ModelDoor = "MODEL_DOOR",
        GtaDemand = "GTA_DEMAND",
        MyModel = "MY_MODEL",
        BombshopCost = "BOMBSHOP_COST",
        PhoneTogg = "PHONE_TOGG",
        SpecificDoor = "SPECIFIC_DOOR",
        TargetScore = "TARGET_SCORE",
        DumMissionTrig = "DUM_MISSION_TRIG",
        CorrectModTrig = "CORRECT_MOD_TRIG",
        CorrectCarTrig = "CORRECT_CAR_TRIG",
        ClockStart = "CLOCK_START",
        ClockStop = "CLOCK_STOP",
        MidpointMulti = "MIDPOINT_MULTI",
        MidMultiSetup = "MID_MULTI_SETUP",
        FinalMulti = "FINAL_MULTI",
        Powerup = "POWERUP",
        BlockInfo = "BLOCK_INFO",
        SpecificDoorBomb = "SPECIFIC_DOOR_BOMB",
        MovingTrigHired = "MOVING_TRIG_HIRED",
        SetupSpeed = "SETUP_SPEED",
        CarbombTrig = "CARBOMB_TRIG",
        DamageTrig = "DAMAGE_TRIG",
        GunTrig = "GUN_TRIG",
        GunScreenTrig = "GUN_SCREEN_TRIG",
        CardestroyTrig = "CARDESTROY_TRIG",
        CarwaitTrig = "CARWAIT_TRIG",
        PedcarTrig = "PEDCAR_TRIG",
        CannonStart = "CANNON_START",
        DumPedBlockTrig = "DUM_PED_BLOCK_TRIG",
        ParkedPixels = "PARKED_PIXELS",
        ChopperEndpoint = "CHOPPER_ENDPOINT",
        MissionCounter = "MISSION_COUNTER",
        SecretMissionCounter = "SECRET_MISSION_COUNTER",
        MissionTotal = "MISSION_TOTAL",
        CarstuckTrig = "CARSTUCK_TRIG",
        BasicBarrier = "BASIC_BARRIER",
        AltDamageTrig = "ALT_DAMAGE_TRIG",
    }
}

/// How much of a command or declaration kind gfa-sim does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Every case of it the scripts use, as GTA8.EXE does it.
    Done,
    /// Some of it: the uses gfa-sim cannot do yet wait on it, logged
    /// (`docs/MISSION_VM.md` says which).
    Part,
    /// Not yet: a process that reaches it waits on it, logged.
    Missing,
}

/// The commands the VM carries out as GTA8.EXE's live cases do. Any other
/// one is logged with its section and line and never takes its success
/// path. `PARKED_ON`'s car does not clear its place of the people and cars
/// in the way first, as the original's does.
pub const IMPLEMENTED_OPS: &[Op] = &[
    Op::Donowt,
    Op::Driveon,
    Op::Unused122,
    Op::Unused123,
    Op::Survive,
    Op::Startup,
    Op::Kickstart,
    Op::KeepThisProc,
    Op::KillSideProc,
    Op::Disable,
    Op::Compare,
    Op::Inccount,
    Op::Deccount,
    Op::MobileBrief,
    Op::Brief,
    Op::ParkedOn,
    Op::ArrowOff,
    Op::PowerupOff,
    Op::IsPowerupDone,
];

/// The commands the VM carries out for some of their uses:
/// - `ENABLE` waits on the doors and the watch triggers, whose own routine
///   is not ported.
/// - `ARROW` and `GENERAL_ONSCREEN` wait on anything that moves (a car, a
///   person). Both use the original's view rectangle, built from gfa-sim's
///   model of the camera.
/// - `POWERUP_ON` waits on the 56 uses whose label names no `POWERUP`
///   (sections 102 and 103), and `MPHONE` on its one whose labels name no
///   `TELEPHONE`.
/// - `DOOR_ON` and `DOOR_OFF` wait on the gates and specific doors, whose
///   loaders are not read.
/// - `ARROWCAR`, `STEAL`, `CHECK_CAR` and `IS_GOAL_DEAD` wait on the cars
///   and people the section's loader makes (`PARKED`, `HELLS`, `PED`),
///   which gfa-sim does not make yet. `ARROWCAR` and `STEAL` also wait on
///   declarations that are not cars (`DUMMY`, `TARGET`), and `CHECK_CAR`
///   on the four uses that ask for a colour.
/// - `IS_PED_IN_CAR` waits on the people other than the player
///   (`FUTUREPED`s), which no ported command makes.
/// - `RESET` and `MPHONE` skip the clean-ups for the mission's own cars
///   (which `PARKED_ON` makes), peds and objects.
pub const PARTIAL_OPS: &[Op] = &[
    Op::Enable,
    Op::Arrow,
    Op::GeneralOnscreen,
    Op::PowerupOn,
    Op::Mphone,
    Op::DoorOn,
    Op::DoorOff,
    Op::Reset,
    Op::Arrowcar,
    Op::Steal,
    Op::IsPedInCar,
    Op::CheckCar,
    Op::IsGoalDead,
];

/// The declarations gfa-sim reads. The world loads the start, crates and
/// shops with the level (`World::load_section`, `Pickups::from_section`);
/// when the mission runs, the crates are the script's `POWERUP_ON`s'. The
/// VM fires `TRIGGER`s, `MPHONES` and `PHONE_TOGG`s, counts with the
/// counters, points at `TARGET`s, rings the `TELEPHONE`s and makes the
/// `FUTURECAR`s' cars. Every other trigger-making kind gets its trigger record, in
/// the original's order, but none of them fires yet. `DOOR`s get their door
/// records for `DOOR_ON`, but the doors themselves are not made.
pub const READ_DECLS: &[DeclKind] = &[
    DeclKind::Player,
    DeclKind::Powerup,
    DeclKind::Spray,
    DeclKind::Bombshop,
    DeclKind::BombshopCost,
    DeclKind::Trigger,
    DeclKind::Mphones,
    DeclKind::PhoneTogg,
    DeclKind::Telephone,
    DeclKind::Counter,
    DeclKind::Target,
    DeclKind::Futurecar,
    DeclKind::MissionCounter,
    DeclKind::SecretMissionCounter,
    DeclKind::MissionTotal,
];

impl Op {
    pub fn status(self) -> Status {
        if IMPLEMENTED_OPS.contains(&self) {
            Status::Done
        } else if PARTIAL_OPS.contains(&self) {
            Status::Part
        } else {
            Status::Missing
        }
    }

    pub fn implemented(self) -> bool {
        self.status() == Status::Done
    }
}

impl DeclKind {
    pub fn read(self) -> bool {
        READ_DECLS.contains(&self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_keep_the_originals_numbers() {
        assert_eq!(Op::ALL.len(), 150);
        assert_eq!(DeclKind::ALL.len(), 72);
        for (n, op) in Op::ALL.iter().enumerate() {
            assert_eq!(usize::from(op.code()), n, "{op:?}");
        }
        for (n, k) in DeclKind::ALL.iter().enumerate() {
            assert_eq!(usize::from(k.code()), n, "{k:?}");
        }
        assert_eq!(Op::from_name("DONOWT").map(Op::code), Some(17));
        assert_eq!(Op::from_name("IS_PED_STUNNED").map(Op::code), Some(149));
        assert_eq!(
            DeclKind::from_name("TELEPHONE").map(DeclKind::code),
            Some(7)
        );
        assert_eq!(
            DeclKind::from_name("ALT_DAMAGE_TRIG").map(DeclKind::code),
            Some(71)
        );
    }

    #[test]
    fn lookup_is_whole_word_and_first_match() {
        assert_eq!(Op::from_name("XXXX"), Some(Op::Unused122));
        assert_eq!(Op::from_name("DONOWT "), None);
        assert_eq!(Op::from_name("donowt"), None);
        assert_eq!(Op::from_name("RED_ARROWCAR"), None, "London's, not GTA's");
        assert_eq!(DeclKind::from_name("CAR"), Some(DeclKind::Car));
    }
}
