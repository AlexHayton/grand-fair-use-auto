# How GTA8.EXE runs a mission script

This is what GTA8.EXE's own interpreter does, read from its code with
`tools/dosprobe/gta8.py`. It is the specification for gfa-sim's mission
script interpreter (the VM, M7). AmiGTA1's `gta_script_run.c` (MIT) is a
good map of what each command is for, but it was written without the
original's code. Where the two differ, this file follows GTA8.EXE. How the
script is read in, and the command and declaration tables, are in
`MISSION_OPCODES.md`.

Addresses are GTA8.EXE's load addresses. "Read" means the code was read.
"Reads as" means the code was read but the purpose of a value was inferred.

## Where the interpreter is

GTA8.EXE holds three copies of the command switch:

| Function | Jump table | Called from                    | Status |
| -------- | ---------- | ------------------------------ | ------ |
| 0x896c0  | 0x89464    | nothing                        | dead   |
| 0x95a50  | 0x957ec    | nothing                        | dead   |
| 0xa1490  | 0xa1234    | the main loop: 0x57165, 0x57340, 0x5746d | live   |

All three share the per-command helpers (0x7axxx to 0x7fxxx). A command is
read from the live table at 0xa1234. The dead copies differ in places, so
they can mislead.

The trigger scan at 0xf3800 is live: 0xa1490 calls it first. The trigger
code at 0xe8a40, 0xe8e20 and 0xebf70 has no callers.

## The frame

The main loop calls 0xa1490 once a frame, beside the other subsystems. So
the script runs at the game frame, 23.333 Hz (the rate the weapons were
measured at, `M6B_PROBES.md`), not AmiGTA1's 25 Hz. Each call does this:

1. **The trigger and shop scan** (0xf3800). This covers the spray, bomb and
   garage zones and the script triggers, and it starts processes.
2. **The processes.** Slots 0 to 31 run in order, one command each for
   every slot in use. A process started earlier in the same frame runs
   its first command in that frame if its slot comes later in the loop.
3. **After every command** (0xea550, at 0xa151f), each player's score is
   checked against the level's target, which comes from the career data
   (0x49460). Once any player reaches it, every process is ended at once:
   the jobs stop the moment the level's points target is met.

## Processes

There are 32 slots, kept in parallel arrays indexed by slot:

| Array    | Holds                                                          |
| -------- | -------------------------------------------------------------- |
| 0x2924ae | in use                                                         |
| 0x292328 | the command it is on (an index, not a label)                   |
| 0x292368 | its state. 1 means it has just arrived at the command. Commands count it up (`SURVIVE`: 1, then 2) |
| 0x2923e8 | the trigger that started it, or -1                             |
| 0x292468 | its parent slot, or -1                                         |
| 0x2d2580 | its keep class. `RESET` spares 1 and 2 (reads as `KEEP_THIS_PROC` and the frenzy's) |
| 0x26cf70 | a value every child copies from its parent                     |

Slots 0 to 3 belong to the players, which reads as up to four in a network
game. At the end of the loader (0x7a695), each player's slot is in use at
command 0, with state 1, trigger 0 and parent -1, and slots 4 to 31 are
freed. In GTA's sections command 0 is `0 DONOWT`, so the player's slot
idles there. A test arena whose commands 0 and 1 are `POWERUP_ON`s gets
its crates from the player's slot. A new process takes the first free
slot from 4 (`PROC_START`, 0x75e90). The process's parent is the player who set off its trigger. A
`KICKSTART` child's parent is that same player, the root of its parent's
chain.

The root player is who gets paid. If no slot is free, the start fails
silently. Its trigger has already been marked as started (0xeaa60 runs
before the search), so the trigger is used up.

## How a command moves on

Two helpers move a process on: 0x7a820 takes p2 (success) and 0x7a8d0 takes
p3 (failure). They set the state back to 1, then:

| Value                | What happens                                                                 |
| -------------------- | ---------------------------------------------------------------------------- |
| 0                    | The next command in file order. There is no end-of-file check.               |
| -1                   | The process ends and re-arms its trigger (state 1) unless that is -1.        |
| any other negative   | The slot is freed. Nothing is re-armed.                                      |
| positive             | The command at that label, straight from the label table.                    |

The label table (0x2924ee) is only written by the loader and never cleared.
A jump to a label the section lacks therefore lands on whatever the table
holds for it. In a fresh game that is index 0, and every section's command
0 is `0 DONOWT`, so the process idles there for good and keeps its slot.

AmiGTA1 ends such a process instead. GTA's single-player sections have 42
such jumps, for example `[1] 779 WAIT_FOR_PED … 780`. In a run that loaded
another section first, a stale index from that section is used.

**Pay.** On the success path, most commands pay p5 to the root player
(0x7a980, then 0xca190 with `100, 100, 0`). Read so far:
- **Pays:** `SURVIVE`, `DISABLE`, `ENABLE`, `COMPARE`, `INCCOUNT`,
  `DECCOUNT`, `POWERUP_OFF`, `IS_POWERUP_DONE`, `PARKED_ON`, `ARROWCAR`,
  `STEAL`, `IS_PED_IN_CAR`, `CHECK_CAR`, `IS_GOAL_DEAD`, `PARK`.
- **Does not pay:** `MOBILE_BRIEF`, `BRIEF`, `STARTUP`, `KICKSTART`,
  `POWERUP_ON`. In the brief commands, p5 is the text key.

AmiGTA1 pays on 17 named commands only. GTA's scripts carry a non-zero p5 on
`SURVIVE` 94 times and on `DISABLE` 27 times, so the difference is in the
data. Which commands pay is to be read case by case as each is ported.

## Commands read so far

- **`DONOWT`** (and `DRIVEON` and the two `XXXX` entries): the empty case.
  The process stays on the command.
- **`SURVIVE`**:
  - The counter lives in the command's own p3, not in the process, so two
    processes on the same line would share it.
  - The first frame loads p3 from p4. Each later frame counts it down, and
    it moves on when the count reaches 0.
  - That makes p4 + 1 frames, where AmiGTA1 takes p4. It pays p5.
- **`STARTUP`**:
  - It runs once a game, behind a global flag (0x26cf6c). A process that
    reaches it again stays on it for good.
  - It marks p1's trigger as started, puts sound 0 on the timed queue for
    ten frames on (0x10b50(10, 0, 20), see _Timed events_), and moves on
    by p2.
  - Unlike AmiGTA1, it does not clear its process's trigger.
- **`KICKSTART`**:
  - It starts a child at p1's command, or at the next command when p1 is
    0. The child's parent is the root player, and it copies the keep
    class.
  - The current process moves on by p2 without pay.
  - It records its children in a declaration record, and it rewrites its
    own p4 and p5 as it runs:
    1. It searches the records from 1399 downward for a free one, where
       x, y, z and kind are all -1.
    2. It writes that record's index into its own p4 and marks the
       record's kind 100.
    3. It stores the child's slot in the record and writes its own slot
       into its p5.
  - Whatever the script writes in p4 is therefore overwritten: -1 (1,011
    times), 0 (162) or 10452 (once).
- **`KILL_SIDE_PROC`**:
  - It reads the `KICKSTART` named by p1, then that command's p4 record.
  - It ends each of the up to five processes recorded there; ending one
    re-arms its trigger.
  - Then it marks the record 100. AmiGTA1 searches by parent instead.
- **`DISABLE`**:
  - It sets the trigger in p1's handle to state 4. There is no check on
    the declaration's kind: a handle outside 0 to 209 stops the game.
  - It clears this process's trigger, then pays and moves on by p2.
- **`ENABLE`**:
  - It first runs a routine per declaration kind (0x7aa60, a switch not
    read yet).
  - Then, for every kind except the doors and gates, it re-arms the
    trigger in p1's handle, with no range check. The door and gate kinds
    are `DOOR`, `BARRIER`, `MODEL_BARRIER`, `SPECIFIC_BARR`, `MODEL_DOOR`,
    `SPECIFIC_DOOR` and `SPECIFIC_DOOR_BOMB`.
  - It pays and moves on by p2.
- **`COMPARE`**: if p1's value equals p4, it pays and takes p2; otherwise
  it takes p3.
- **`INCCOUNT`**: it adds one to p1's value. If p4 is -1 or below, or the
  value is less than p4, it takes p3; otherwise it pays and takes p2.
- **`DECCOUNT`**: it subtracts one. Above 0 it takes p3; otherwise it pays
  and takes p2.
- **`RESET`**:
  - It ends every process in slots 3 to 31 except itself, those waiting
    on `MPHONE` and keep classes 1 and 2.
  - Ending one re-arms its trigger. AmiGTA1 ends them without re-arming,
    so after its `RESET` a permanent trigger stays used up where the
    original's is live again.
  - Then it runs eight clean-up routines:
    - 0x765b0, 0x76490 and 0x766e0 take away the mission's own peds, cars
      and objects (lists at 0x26cfb8, 0x26d468 and 0x26d210), except those
      the routines keep. Every `TELEPHONE` in GTA's scripts has flag 1, so
      no phone is in the objects list.
    - 0xe8460 disables every trigger whose declaration has no flag.
    - 0xe84a0 shuts and turns off every door whose declaration has no flag.
    - 0xfef50 puts the arrow out.
    - 0xe7670 and 0xf20f0 stop sounds (five channels, and four at
      0x3dcca0).
  - Then it pays p5 and moves on by p2.
- **`RESET_WITH_BRIEFS`**: it is not `RESET`. It runs the same loop and two
  more clean-ups (0x102260 and 0x1029a0, which read as clearing the
  briefs), and it skips 0xf20f0. AmiGTA1 treats the two as one.
- **`KEEP_THIS_PROC`**: it sets the keep class to 1, then pays and moves
  on by p2.
- **`KILL_PROCESS`** and **`KILL_SPEC_PROC`**: both work through the same
  `KICKSTART` record. `KILL_SPEC_PROC` ends the one child that p4 (1 to 4)
  picks, and pays. The record's slots are, in order, its +0x10, +0x14,
  +0x18, +4 and +8; `KICKSTART`'s own child is in the first.
- **`ARROW`**:
  - p1 of -1 puts the arrow out (0xfef50).
  - Otherwise it points the arrow at p1's position (0x7acf0, in the
    original's pixels). A position exactly on a block corner is nudged to
    the block's centre. For a `TELEPHONE` that is its block's centre, the
    same as the phone itself.
  - 0xfee50 sets the target and starts the easing again from 0 (see _The
    arrow_).
  - It pays and moves on by p2.
- **`ARROW_OFF`**: it puts the arrow out, pays and moves on by p2.
- **`DOOR_ON`** (0xa293f) and **`DOOR_OFF`** (0xa298f):
  - They read p1's handle as a door record, with no check on the kind, and
    call 0xe8800 or 0xe87a0.
  - While the door is opening or closing (state 2 or 3) these refuse, and
    the process stays on the command.
  - Otherwise the door is turned on or off (see _Doors_), and the command
    pays and moves on by p2.
  - GTA's scripts give them gates and specific doors 20 times, whose
    loaders have not been read.
- **`GENERAL_ONSCREEN`** (0xa583b):
  - It tests p1's position (0x7acf0, in whole pixels) against the root
    player's view rectangle (0xcd760, see _The view rectangle_), edges
    included.
  - Inside, it pays and takes p2; outside, it takes p3. Liberty City's
    opening gives p3 its own label, so it waits there.
- **`MPHONE`** (0x7d400, see _Phones_): it rings p3 phones, labelled p1,
  p1 + 1 and on, until the player answers one or goes out of reach. It
  never pays. In one section the labels name no `TELEPHONE`.
- **`MOBILE_BRIEF`**: it shows text p5 to the root player (0xf1cd0) and
  moves on by p2. A p2 of -1 ends the process through the routine that
  re-arms its trigger.
- **`BRIEF`** (0xa1628): it shows text p5, read as a word, by the phone
  (0xf1b70, see _Briefs_) and moves on by p2 without pay. It first reads
  p1's position (0x7acf0) and p3 into the interpreter's scratch
  (0x32a4e8 to 0x32a4fc), and 0xf1b70 rounds p4 / 30 to the nearest 10,
  but the brief box reads none of them. A key below 1000 stops the game
  (error -246).
- **`PARKED_ON`** (0xa34d4, see _Cars_): it makes p1's `FUTURECAR`'s car.
  With a car made, it pays and takes p2; with none, p3. GTA's scripts give
  it a `FUTURECAR` every time, and p4 is always 0.
- **`IS_PED_IN_CAR`** (0xa4c40): if p1's ped is in a car (its +0x66 is 6
  or 7) and, with p4 above -1, the car's model (+0x22) is p4, it pays and
  takes p2. Otherwise, or with a handle of -1, it takes p3. GTA's scripts
  name the `PLAYER` in all but 24 uses (`FUTUREPED`s, once a `DUMMY`).
- **`CHECK_CAR`** (0xa309a): if the root player is in a car (his state 0)
  and it is p1's, and with p4 above -1 the car's colour (+0x23b) is p4, it
  pays and takes p2; otherwise p3. The four uses with a colour follow a
  respray.
- **`IS_GOAL_DEAD`** (0xa5658): if p1 has no health left (0x7af40), it pays
  and takes p2, otherwise p3. 0x7af40 gives, by p1's kind:
  - a car's (`CAR`, `PARKED`, `MODEL`, `ESCORT`, `ESCORTED`, `STOPPED`,
    `HELLS`): 0 once its record is gone (+8 is -1), otherwise 100 less its
    damage (+0xf7), down to 0;
  - a person's (`PED`, `PLAYER`, and kind 99, which reads as a person a
    command made): 0 once his +0x52 is set or his +0x66 is 5, 23 or 24,
    otherwise his health (+0x44);
  - every other kind's: 100. So a `FUTURECAR` or `FUTUREPED` not yet made
    is alive.

  With p4 above 0, a kind 99 must also have been killed by p4 (its +0x54).
- **`ARROWCAR`** (0xa2306): if p1's handle is not -1, the arrow follows
  that car (0xfedd0, see _The arrow_). It does not check the kind, so a
  `FUTURECAR` not yet made would have its model read as a car. Then it
  pays and moves on by p2.
- **`STEAL`** (0x7bea0):
  - On arrival the state becomes p4 + 5 if p4 > 0, otherwise 0, as for
    `IS_POWERUP_DONE`.
  - Once the process's player is at the wheel of p1's car, it plays sound
    17, pays and takes p2. 0xee930 asks that the car's driver (+2) is his
    ped, the ped is in a car (+0x66 6 or 7) and alive (+0x44), and its
    sprite (+0x17) is 0 or 127 and up. In the probe (see _In a car_) that
    held from when the door shut behind him.
  - Until then, with p4 > 0 it takes p3 p4 frames after arriving (that
    frame included). With p4 ≤ 0 it waits for good, where
    `IS_POWERUP_DONE` takes p3 at once.
  - GTA's scripts give it `FUTURECAR`s, `PARKED` and `HELLS` cars, and
    `DUMMY`s; p4 is 0, 100, 200 or 3000.
- **`POWERUP_ON`** (0xa262f):
  - It reads p1's `POWERUP`: the kind at +4, the amount at +8, and the
    position through 0x7acf0.
  - It calls 0xd7c10(kind, amount, x << 16, y << 16, (z << 16) − 1),
    which makes the crate (see _Crates_ below).
  - It moves on by p2 without pay, whether or not a crate was made.
  - It does not check p1's kind. In sections [102] and [103], 56 uses
    name a label the section lacks, so they read declaration record 0
    (an `MPHONES` in [102], a `PARKED` in [103]).
  - AmiGTA1 also refuses a second crate on the same `POWERUP`; the
    original makes one.
- **`POWERUP_OFF`** (0xa26ae): it takes away the first crate record at
  p1's exact x and y (0xd7fd0), whether or not there is one. Then it pays
  and moves on by p2.
- **`IS_POWERUP_DONE`** (0x881a0):
  - On arrival the state becomes p4 + 5 if p4 > 0, otherwise 0.
  - If the crate at p1's place is gone (0xd8060 finds none), it pays and
    takes p2.
  - If it is still there and p4 > 0, the state, a signed word, counts
    down, and p3 is taken when it reaches 5. That is p4 frames, counting
    the arrival frame.
  - If it is still there and p4 ≤ 0, it takes p3 at once. AmiGTA1 waits
    for ever instead.
  - For kind 15 it plays sound 12 (0x16870) when it takes p3. Sound waits
    for M8.

## Triggers

Triggers are 29-byte records at 0x3db2e8, at most 210 of them. A
declaration that makes a trigger holds the record's index in its handle
(record +4).

**Making them** (0xe8340). The records are numbered in load order, and
every one starts armed:

| Offset | Holds                                                                  |
| ------ | ---------------------------------------------------------------------- |
| +0     | state: 0 firing, 1 armed, 2 waiting, 3 started, 4 disabled             |
| +4     | type, 0 to 33                                                          |
| +8     | the line to start (a `TRIGGER`'s first number)                         |
| +0xc   | -1, or the third number for types 0xa, 0xb, 0xe, 0xf, 0x10, 0x13, 0x15, 0x18, 0x19, 0x1b to 0x1e and 0x21 |
| +0x10  | the block, as three bytes                                              |
| +0x13  | the radius (a `TRIGGER`'s second number)                               |
| +0x17  | the delay: an armed trigger with one calls 0x10b50 first and waits in state 2. Types 0x1d and 0x20 skip this. 0 for the types listed at +0xc |
| +0x1b  | the process to end when it fires (`SET_KILLTRIG`), -1 at first        |
| +0x1c  | the declaration's flag                                                 |

**Which kinds make which type.** This was read from each case of the
loader's switch (0x798ea) and the helpers those cases call.

| Type | Declaration kind | Type | Declaration kind |
| ---- | ---------------- | ---- | ---------------- |
| 3    | `SPRAY`          | 19   | `MIDPOINT_MULTI` |
| 4    | `TRIGGER`        | 20   | `FINAL_MULTI`    |
| 8    | `BOMBSHOP`       | 21   | `DUM_PED_BLOCK_TRIG` |
| 9    | `CARTRIGGER`     | 22   | `MOVING_TRIG_HIRED` |
| 10   | `ONETRIGGER`     | 24   | `CARBOMB_TRIG`   |
| 11   | `MOVING_TRIG`    | 25   | `DAMAGE_TRIG`    |
| 12   | `MPHONES`        | 26   | `GUN_TRIG`       |
| 13   | `PHONE_TOGG`     | 27   | `GUN_SCREEN_TRIG` |
| 14   | `DUM_MISSION_TRIG` | 28 | `CARDESTROY_TRIG` |
| 15   | `CORRECT_MOD_TRIG` | 30 | `PEDCAR_TRIG`    |
| 16   | `CORRECT_CAR_TRIG` | 31 | `CANNON_START`   |
| 17   | `CLOCK_START`    | 32   | `CARSTUCK_TRIG`  |
| 18   | `CLOCK_STOP`     | 33   | `ALT_DAMAGE_TRIG` |

`CARWAIT_TRIG` takes its type from the declaration it watches (0x79030),
which has not been read. The doors make no trigger.

**Other loader cases:**
- `COUNTER`, `TARGET` and `FUTURE` (0x79b5d) keep their first number at
  +4 and their second at +8.
- `POWERUP` (0x79db3) does the same: the kind at +4, the amount at +8. It
  makes no crate. A crate exists only once a `POWERUP_ON` names it.
- `FUTURECAR` shares `POWERUP`'s case: the model at +4, the turn at +8. Its
  car exists only once a `PARKED_ON` names it.
- `PARKED` (0x76bf0) makes its car at once, as `PARKED_ON` does (see
  _Cars_).
- `MISSION_COUNTER`, `SECRET_MISSION_COUNTER` and `MISSION_TOTAL` keep
  their first number in both, and also in globals (0x2d2600 to 0x2d2610).

**The scan** (inside 0xf3800) takes the triggers in record order and skips
disabled ones. For each trigger it checks every player:
- **His state.** It reads the player's state (his record +0xb9, 0x31 into
  the player part). In state 0 he is in a car, whose position is used; in
  state 2 he is on foot (the position switch at 0xcdb40 reads the car for
  0 and the ped for 2). A player in state 1 is skipped for every trigger.
  A probe found state 2 while he got in and out of a car, so that is not
  state 1 (see _In a car_); what is has not been read.
- **The range test** compares blocks, inclusively. The player's block (his
  position >> 22) must lie within the trigger's x−r to x+r and y−r to
  y+r, so a radius of 0 is the trigger's own block. AmiGTA1 rounds 0 up to
  1 and measures from the block's centre.
- **Per type.** Some types first pass a check of their own (the switch at
  0xf41cc). Type 4 has none, so a `TRIGGER` fires on foot or in a car.
- **Firing.** An armed trigger in range goes to state 0. Any trigger in
  state 0 then runs its type's case (the switch at 0xf594a), in range or
  not, so a trigger whose case leaves it in state 0 runs every frame.
  Type 4 starts its line with the player as parent, which marks it
  started, and records the new process (0x76970).
- **The kill link.** After the case, if the trigger's kill link is set
  (and its type is not 0x21), that process ends too.

**`MPHONES` (type 12):**
- Its line is its first number and its radius its second.
- Its check (0xf4075) skips a player driving a car of model 47. A trigger
  still in state 0 from the frame before is put back to armed and skipped.
- Its case (0xf4fec) does nothing while a phone rings (0x3dcbb8) or the
  root player's phone lock is on (0x76ad0 reads 0x2d24f0). The trigger
  stays in state 0, so the check re-arms it on the next frame and it can
  fire again on the frame after. Otherwise it starts its line, as type 4
  does, and goes to state 3.

**`PHONE_TOGG` (type 13):**
- In place of a line it holds the trigger of the `MPHONES` its first
  number names; its radius is its second number.
- It has no check. Once the player first comes within its radius it stays
  in state 0, so its case (0xf5053) runs every frame until a `DISABLE`.
- The case re-arms its `MPHONES` (state 3 to 1) while the player is within
  its radius but more than 3 blocks from it on either axis.

The other 31 types have not been read one by one yet.

**`ENABLE`'s per-kind routine** (0x7aa60) does nothing for most kinds,
including `TRIGGER`, `MPHONES`, `PHONE_TOGG`, `CARTRIGGER`, `SPRAY` and
`COUNTER`; for those, `ENABLE` only re-arms. It has work of its own for
these kinds, which are not read yet:
- the doors and gates;
- `ONETRIGGER`, `MOVING_TRIG` and `DUM_MISSION_TRIG`;
- `CORRECT_CAR_TRIG`, kinds 45 to 58, and the watch triggers from 59 to
  69.

## Crates

The crate records are at 0x3baafc: 256 of them, 28 bytes each. Each holds
x, y, z, the object (-1 when the record is free), the state (1 a crate,
2 the item), the kind and the amount.

**Making one** (0xd7c10):
- It takes the first free record. It does not look for a crate already on
  that place.
- A kill frenzy (kind 14) is made as its item, object 0x5f, in state 2,
  with no crate under it. Every other kind is made as a crate (0x54).
- The object routine (0xae040) makes nothing for a block outside 1 to 255
  or a height outside 1 to 5.

**Where it stands.** `POWERUP_ON` passes the block's corner (block × 64 in
the original's pixels). In a DOSBox-X probe (the `weapons_ammo` arena),
the crates stand in line with the player, who starts at the block's
centre. gfa-sim puts them at the centre.

**What a crate holds when it opens** (the table at 0xd7ce8):

| Kind     | Object | Kind | Object |
| -------- | ------ | ---- | ------ |
| 1 to 4   | 0x4e to 0x51 (the guns) | 11 | 0x64 |
| 6        | 0x61   | 12   | 0x62   |
| 9        | 0x65   | 13, 15 | 0x63 |
| 10       | 0x60   |      |        |

Kinds 0, 5, 7, 8 and 14 hold nothing. gfa-sim gives speed for 7 and 8,
which GTA's data never uses.

**Liberty City's start.** The level declares 151 `POWERUP`s, 32 of them
kill frenzies. The opening's block at 30000 puts out 89: 73 crates and
frenzies 1 to 16, standing loose. The others wait for the jobs that name
them, and frenzies 19, 22, 25 and 31 are never put out in section [1].

## Doors

The door records are at 0x3da9e5: at most 64, 36 bytes each, counted at
0x3dcab4. The loader's `DOOR` case (0x79bb9) makes one (0xe7e10) and keeps
its index in the declaration's +4:
- **+4, the state:** 0 open, 1 shut, 2 opening, 3 closing. It starts
  shut.
- **+8:** 1 when the door is off, 0 when on. 0xe8600 then 0xe87a0 leave
  every door off; `DOOR_ON` turns it on and `DOOR_OFF` off again.
- **+0x26:** the declaration's flag, which `RESET` reads.
- **The scan's door pass** (the start of 0xf3800) runs before the
  triggers. For each door that is on and open or shut, and each player:
  - in range (his block within +0xc of the door's block on both axes) and
    shut: if he passes the check its type (+0x1f, the table at 0xf36b0)
    names, the door opens (0xe84e0);
  - out of range and open: if the doorway is clear (0xe8900, not read), it
    starts closing (state 3): the block animation runs backwards (0x12350,
    0x12450) and 0xe7d30 plays its sound.
  - The checks: a `DOOR` (type 0) opens for a player in a car (his state
    0). Others want an emergency vehicle (models 4, 5, 15, 16, 32 and 44,
    or a car whose +0x34 is 14), a given car in a given colour, a given
    model, or the mission's car; their kinds are not matched up yet.
- **Opening** (0xe84e0): state 2. The block becomes field (0x69450 with
  0x40), and the block animation starts forwards (0x122a0 with 15, then
  0x12450).
- A 65th door stops the game.

**What a `DOOR` is** (0xe74d0, then 0xe7e10). Its numbers are its facing,
the first frame of its animation and a floor tile:
- the facing (the first number, 1 to 4, 0xe74d0) picks which side of the
  declared block the door stands on (the tables at 0xe7dfc and 0xe7dec);
- the door gets a block animation (0x120d0) of 8 frames from the second
  number (0x12410), shown on its face (0x69ac0), and its block is flagged
  0x80 (0x69670, which reads as flat);
- its type (+0x1f) is 0 and its record starts shut and off.

AmiGTA1 (`gta_main.c`, `doors_build`) models the same door from research
notes: a thin wall on one edge of the block beside the declared one, its
face the animated tile, solid while shut. Opening clears the wall and turns
the declared block to field; closing puts the wall back. It gives a `DOOR`
radius 2 and plays a frame every 15 of the original's frames.

Liberty City's opening turns on the seven bomb shop doors (31500 to
31560). Before that, no bomb shop opens.

## Phones

**The phone** (the loader's `TELEPHONE` case, 0x76da0):
- It moves the declaration's position to its block's centre, in pixels
  (block × 64 + 32; height × 64). The position reader (0x7acf0) returns
  that for the kind.
- It makes object 0x28 there (0xf1990), turned by the second number
  (1024 steps a turn). Its handle (+4) holds the object, and +8 the first
  number when above 0 (otherwise -1).
- A phone without a flag also goes in the mission's objects list. Every
  `TELEPHONE` in GTA's scripts has flag 1.
- Its frames: 0xf1b10 sets 1 (still) or 3 (ringing), and 0xf1dd0 swaps 2
  and 3 at each ring.

**`MPHONE`** (0x7d400), for the root player:
1. While his phone lock (0x2d24f0) is on, it does nothing.
2. **Arriving** (state 1), the state becomes 0. If no phone is ringing
   (0x3dcbb8), the phones ring: the flag is set, p4 becomes 25, and each
   phone with a handle goes to frame 3.
3. **Each frame** p4 counts down by one.
4. **Out of reach:** if the player is more than 15 blocks from the first
   phone on either axis (0xf1900: his car's block when in one), the
   phones go still and the flag clears. The process frees its slot
   without re-arming its trigger.
5. **Each phone in turn:**
   - At p4 0 and at -10 each phone with a handle rings (0xf1dd0).
   - The phone is answered if four things hold:
     - he stands within 16 pixels of it on both axes (0xf17a0, the ped's
       own position);
     - it has a handle;
     - his lock is off;
     - his ped's +0x66 is not 6 or 7, which reads as not in or at a car.
6. **The rings:** a ring at 0 sets p4 to -1, and one at -10 sets it to
   25. So two rings, ten frames apart, every 36 frames.
7. **Answering** (0x7d700):
   - The phones go still, and every other process ends as for `RESET`.
   - The clean-ups run: `RESET`'s, without 0xf20f0, and with the two
     brief routines.
   - The process moves on by p2, then forward by the phone's place in the
     list, so each phone leads to its own line.
   - The answered phone's handle becomes -1, so it never rings again.
   - The ringing flag clears.
   - The lock goes on, and an event clears it 250 frames later.

## Timed events

0x10b50(frames, kind, value) puts an event on a queue at 0x142880, kept
in order of when it is due. Once a frame, just before the script, 0x10ed0
runs the events due at the clock (0x1427a0), then moves the clock on by
one. A new event goes before others due at the same time. The handlers
are a switch on the kind (0x10eac):
- **Kind 0** (0x9f7d0) by value: 20 plays sound 0 (from `STARTUP`), and
  50 to 53 clear that player's phone lock (from `MPHONE`).
- **Kind 4** (0xf6010) is a trigger's delay. It has not been read.

## In a car

A DOSBox-X probe (`tools/dosprobe/scripts/mission_incar.probe`, on the
`getin` overlay) dumped RAM while the player got into the Bulldog and out
again. What the commands read:

| When                               | Ped +0x66 | Ped +0x72 | Ped +0x17 | Car +2 (driver) | State (+0xb9) |
| ---------------------------------- | --------- | --------- | --------- | --------------- | ------------- |
| on foot, walking to the door       | 2         | 2, then 18 | his sprite | -1            | 2             |
| sat down, the door swinging shut   | 7         | 17        | 0x1a, 0x1f | his ped         | 2             |
| the door shut                      | 7         | 17        | 0         | his ped         | 0, +0xbd the car |
| getting out                        | 7         | 19        | his sprite | -1              | 2             |
| out                                | 2         | 2         | 0x88      | -1              | 2             |

His state (record +0xb9, read by 0xcdd80) is 0 only at the wheel with the
door shut; getting in and out it is 2, on foot, and his ped's position
counts. So the trigger scan places him by his ped then, not skipping him.
gfa-sim's door shuts when `door_tick` runs out. State 1 was not seen; its
writers are not read.

## Briefs

0x1023d0(kind, key, time, player) shows text `key` to `player` if he plays
on this machine (0xc9580). It writes the key's decimal digits (0x10fd8d),
looks them up in the text file (0xf8e50) and copies the text out. Then it
switches on the kind (the table at 0x1023ac). The routines that call it,
and the live command that calls each (the dead copies call them too):

| Routine | Kind | Live command    | What 0x1023d0 does                               |
| ------- | ---- | --------------- | ------------------------------------------------ |
| 0xf1c00 | 0    | `P_BRIEF`       | 0x102ec0, not read                               |
| 0xf1b70 | 1    | `BRIEF`         | the brief box, icon 0 (the phone)                |
| 0xf1c90 | 2    | `P_BRIEF_TIMED` | 0x1031d0 with the time, which reads as the pager |
| 0xf1cd0 | 3    | `MOBILE_BRIEF`  | the brief box, icon 2 (the mobile)               |
| 0xf1d10 | 4    | `SPEECH_BRIEF`  | the brief box, icon 1 (talk)                     |
| 0xf1d50 | 5    | `FRENZY_BRIEF`  | the brief box, icon 5 (the skull)                |

Each routine stops the game for a key below 1000 (error -246). For kinds
1, 3 and 4, 0x15960 also runs (0xe4d20, behind two flags, not read).

The brief box is 0x101f00(icon, text). Its icons are Arrow sprites 11, 10,
12, 14, 13 and 15 for icons 0 to 5 (the table at 0x101ee8), plus the HUD
art's base. Icons 0 to 2 outrank 3 to 5. gfa-app's brief box
(`gfa_app::hud`) came from AmiGTA1 and already uses these sprites in this
order. The VM's briefs go to it through the world's HUD events.

## Cars

**Making one** (`PARKED_ON`, 0xa34d4):
1. It reads p1's position (0x7acf0): a `FUTURECAR`'s block, in pixels.
2. 0xedfd0(block, model, turn, p4) makes the car: 0x19ab0 at the block's
   centre, at height z × 64 − 2. If 0x19ab0 makes none, the game stops
   (error -78), so the command's own path for no car is never taken.
3. 0xefd10 then clears the place for it, ignoring the new car:
   - 0xefbf0 and 0xefb60 take away what stands on its block: objects of
     class 3, and of class 1 with +0x62 at 25. These read as people;
   - 0xef8b0 takes away the other cars its box touches, with exceptions
     not read in full (among them models 4, 5, 15, 16, 32 and 42). For a
     long car it does more with the blocks round it, also not read.
4. The car is marked the mission's: +0x12b holds p1, and +0x130 is
   cleared.
5. The declaration becomes a `CAR` (kind 0) whose handle is the car.
6. One without a flag goes on the mission's list of cars (0x26d468,
   counted at 0x26d6c0).

**Its turn.** The map loader makes a parked car from a map object by
handing the object's rotation word (+8) to 0x19ab0 in the same place, so a
script's turn reads as the map's own cars' do. gfa-sim turns both by
`turn / 4` to its 256 steps, as AmiGTA1 does.

**The section's own cars.** The loader makes a `PARKED` car the same way
(0x76bf0). It clears the place first, then makes the car, then the kind
becomes `CAR`. Liberty City declares nine, among them `293` beside the
player's start (`294 PLAYER 293 256`).

**Parking one** (`PARK`, 0x7f1e0). Each player has a car being parked
(0x26d6cc, -1 for none). The first time the root player is in a car (his
state 0), that car becomes it. While the process is below state 3, he must
stay in it, or the command waits. Then, by the process's state:
1. **Arriving.** The car's +0x1c is 5 and +0xbb 1. For a `PARK` without
   p3, if the car is within the process's trigger's area, 0xcd7f0 runs
   for the player (not read). The state moves on.
2. **Driving in.** p4 (0 to 3) names the side the car comes in from (the
   table at 0x7f1b4). The car must be inside the door's block by half its
   length (32 pixels, or 64 for a long one), no more than 3 blocks past it,
   and within 64 pixels of its line. Then:
   - the game takes the car over: its +0x1c is 5, or -5 when it faces the
     way in (0xef020 reads its heading in quarters: 2 north, 1 south, 4
     east, 8 west), and +0xa, +0xb1 and +0xbb are set as for the car's own
     control code, which is not read;
   - if he is at the wheel, he is made to get out (0xcf850) and walk to a
     point 74 pixels out from the door on its line, facing it (the turns
     at 0x132ba8: 0, 768, 512, 256 for p4 0 to 3).
3. **In.** While the car is still short of the door's block, it is pushed
   in at 10 (-10 facing out). Once he is out of the car (his ped's +0x66
   is no longer 7), the door shuts (0xe8580), the car stops and is no
   longer the mission's (+0x12b -1), an event of kind 0 for the car runs
   10 frames later (0x10b50 with 200 + the car), the command pays p5 and
   takes p2, and 0xcdea0 and 0x759a0 run for the player (not read).

**`RESET`'s clean-up** (0x76490) takes away each car on the mission's list,
together with its driver, unless a test keeps it (0x1e0f0, not read). One
it cannot take away yet stays on the list.

## The view rectangle

The root player's record (0x3ba478, 0x1a0 bytes a player) keeps his view
in the original's pixels. 0x736d0, and copies of it inlined in the camera
routines, write it each frame:
- +0x30 is the camera's depth. The camera works it out as the mode's
  height (300 for mode 0, from the table at 0x1327a8) less the player's
  height and the zoom, plus 32, and keeps it between 16 and 716. On foot on
  the ground it is 78. The M6B probes found the same field and called it
  the lens (`docs/M6B_PROBES.md`): 80 in a car at rest, rising with speed
  to 240.
- +0x3c and +0x40 are the plane, 284 at rest.
- +0x34 and +0x38 are the view's centre, in whole pixels.
- The rectangle (+4: left, right, top, bottom) reaches
  `(depth + 384) × 160 / plane` pixels either side of the centre and three
  quarters of that above and below, with no edge below 0. At rest that is
  260 and 195, a rectangle of 520 by 390.
- The projection at 0x733a0 shows why it is only roughly the screen: a
  point's depth is its height plus the camera's depth, so the ground is
  nearer than the rectangle assumes. The screen's y is also squeezed to 5/6
  of x (320 by 200 pixels).

gfa-sim builds the rectangle with this formula from its own model of the
camera's two settings (`SimCamera`). That model rests at 80 where the
original has 78 on foot, because it leaves out the player's height. Its
rectangle is therefore a pixel wider either side: 261.

## The arrow

Read in GTA8.EXE and checked against RAM dumps of it in play
(`tools/dosprobe/scripts/mission_arrow.probe`, Liberty City's opening):
- **The state:** two arrows (0x3e73e0 and 0x3e7410, 0x30 bytes apart):
  - `ARROW` sets the first (0xfee50): the target in 16.16 pixels at
    0x3e73e6, what it follows at +0 (1, a fixed point), the margin's kind
    0 at +0x12, the easing (0x3e74f0) back to 0, and on (0x3e74d8).
  - `ARROWCAR` sets it to follow a car (0xfedd0): +0 is 0 and +4 the car,
    with the margin's kind 0, the easing 0, and on. 0xfee10 is the same
    with 3, which reads as a ped. Other setters set the second arrow or
    the margin's kind; the commands that use them are not read.
  - 0xfef50 clears it.
- **What it points at** is read each frame (0xfefe0), by +0: 0 a car's
  position (0x1aa50), 1 the fixed target, 3 a ped's (0xc5c50), 4 a
  player's (0xcdb40). 2 stops the game.
- **Each frame** (0x105f9b) the arrow is placed and drawn (0x3e74e0)
  unless its target's position equals the player's, z included. In a car
  his position is the car's, so an arrow on the car he drives is not
  drawn, and gfa-sim tests that. Standing on the ground he is at 254 and a
  declaration's z is 256, so a fixed point never passes the test, and
  gfa-sim leaves it out there.
- **Where it stands** (0xff0e0):
  - The arrow is an object (0x3e7428) on the line from the player to the
    target, at the player's z.
  - Its angle is 0x1040e's: 1024 steps a turn, 0 north, 256 west, 512
    south, 768 east, so it grows counter-clockwise on screen. 0x1040e
    divides the y difference by the whole pixels of the x difference and
    searches a 256-entry tangent table (0x1039c). The sine table at
    0x1934a4 has 1280 entries, so the cosine is the sine read a quarter
    turn on. Both are built at run time. The dumps match
    `trunc(sin(2πi/1024) × 65536)` and `trunc(tan(πi/512) × 65536)`, with
    tan exactly 1.0 at 45° (`gfa_fixed::dos_trig`).
  - It stands `ease + margin + depth / 4` pixels out:
    `x − sin(a) × r`, `y − cos(a) × r`. The margin is 32 for kind 0, 38 for
    1 and 32 + 6 × (n + 1) for 2.
  - While the target is in the view rectangle, the easing value grows by 8
    a frame until the arrow would pass a point the margin short of the
    target, measured on the longer axis. Then it is set to stop the arrow
    there. Close to the target it goes below 0, and the arrow comes in to
    the player. Outside the view it shrinks by 8 a frame, down to 0.
  - In the dumps the target was `137 TARGET` at (7935, 8736), out of view.
    The player stood at (6752, 7648) on foot (depth 78), and the arrow
    stood at angle 646, 51 pixels out, to the 1/65536 pixel. gfa-sim gives
    the same, as its tests check.
- **Its sprite:** `[0x3dddc0] + [0x3e6e90]`. The first is the Arrow
  category's base, 0, because it is the style's first category (0xf7c60).
  The second is set when the fonts are loaded (0xfda00): 24 with font set
  2, otherwise 0. GTA8.EXE loads only set 1 (0xf84b2), so it draws Arrow 0,
  12 by 10 pixels. The G24 styles have a second set of HUD art from 24 on,
  whose arrow is 20 by 28. gfa-app uses that set and the `*2.FON` fonts
  with a G24 style, so it draws Arrow 24 by the same rule.
- **Drawing:** 0xf6cc0 draws it over everything, its art (pointing north)
  turned counter-clockwise by its angle, centred on it. In a game for
  several players it is also recoloured (0xf7250). 0x186ebc must be set; it
  was 1 in play, and gfa-sim takes it as always on.

## The port so far

`gfa_sim::mission::Vm` follows this file. The game runs it from the start
of every level (`gfa_app::game::Game::new`), and test scripts turn it on
with `# mission: on`. When it starts, the crates the world put out from
the level's `POWERUP`s go, and the script's `POWERUP_ON`s make them again
(`World::start_mission`).

- **Ported:**
  - the frame;
  - the slots, parents, states and keep classes, with each player's slot
    starting at command 0;
  - the crate records, with kill frenzies standing loose;
  - the two advance helpers;
  - pay;
  - the check on the points target after every command (the target itself
    waits for the career data, M8);
  - the trigger records in load order;
  - the scan for types 4, 12 and 13;
  - the door records, which never move because the doors themselves are
    not made yet;
  - the phones, drawn from the style as object 0x28 in the frame
    `MPHONE` sets, and the phone lock;
  - the timed queue, for the two events the ported commands use;
  - the view rectangle, from gfa-sim's model of the camera;
  - the arrow: placed each frame as 0xff0e0 does, on a fixed point or on
    a car, and drawn from the style;
  - the briefs, shown in gfa-app's brief box;
  - `PARKED_ON`'s cars, which stand in gfa-sim's fleet as the mission's.
    The clearing of their place and `RESET`'s clean-up are not ported;
  - the commands marked done or in part in `MISSION_OPCODES.md`.
- **What the coverage report counts:** a command is done when every use of
  it in GTA's scripts runs as above, and in part when some uses wait.
- **What waits:** a command not ported, and any use of one that gfa-sim
  cannot serve yet. The process waits on it, logged once with the
  section and line. It never takes the success path.
- **Ours:** where GTA8.EXE writes outside its arrays, gfa-sim does nothing
  and logs the line. That covers a start with no free slot (the
  `KICKSTART` child), a trigger index out of range, and a `KICKSTART` with
  no record free.
- **How far Liberty City runs:**
  - **The opening.** Trigger 303 fires on the first frame. Then come
    `SURVIVE`, `ARROW`, `MOBILE_BRIEF` and `STARTUP`, the crate block at
    30000, the `DISABLE`s and the bomb shop doors.
  - **What it starts.** 32000 starts the 8000 block, which waits at
    `8011 SCORE_CHECK` (tick 437). 32001 starts the phone arrow at 13 and
    ends the opening. The side processes it started earlier wait on
    `IS_GOAL_DEAD`.
  - **The phones.** Within 3 blocks of them, line 27's `MPHONE` rings
    them. Answering phone 68 ends the arrow loop and runs `29 RESET`, then
    60 to 65 and the first job at 100.
  - **The first job.** `101 BRIEF` shows 1002 by the phone. `104
    PARKED_ON` makes car 189, model 18, at (167,97), and `106 ARROWCAR`
    points the arrow at it. `108 STEAL` pays 5000 once he is at its wheel.
    Then come `112 MOBILE_BRIEF` 1003 and `120 ARROW` at the garage door
    (191). Its side processes watch the car (`150 IS_GOAL_DEAD`) and him
    (`430 IS_PED_IN_CAR`). On the garage's trigger, 225, `125
    IS_PED_IN_CAR` and `126 CHECK_CAR` pass, and `130 DOOR_ON` turns the
    door on. The job waits at `135 PARK`.
  - **The checks.** `tools/scripts/mission_opening.txt`,
    `mission_crates.txt`, `mission_phones.txt` and `mission_job1.txt`
    check this.
- **Next:** the garage: the doors themselves (made in the map, opened by
  the scan's door pass, closed by `CLOSE_DOOR`), then `PARK`. Then the
  section's own cars (`PARKED`), which the later jobs' car commands name.
- **Still to read:**
  - the other trigger types, and the doors' scan pass;
  - the per-kind `ENABLE` routine;
  - the brief clean-ups (0x102260, 0x1029a0), and kinds 0 and 2 of the
    brief routine (0x102ec0, 0x1031d0);
  - `RESET`'s test for keeping a car (0x1e0f0);
  - for the doors: which kinds get which check type, the range (+0xc),
    the doorway test (0xe8900), the block animation's timing (0x12350,
    0x122a0) and what the door does to the map in each facing;
  - for `PARK`: the car's control fields it sets, and 0xcd7f0, 0xcdea0
    and 0x759a0;
  - the timed queue's kind 4;
  - how a `TELEPHONE`'s turn reaches the screen. gfa-app draws it as
    Carnage3D reads the map's objects (clockwise, the art facing south).
    The original turns the arrow's art counter-clockwise. If the phones are
    drawn the same way, a phone turned 0 or 512 is drawn half a turn
    round. It needs a DOS screenshot of such a phone;
  - the score routine's multiplier (0xca190);
  - what taking a kill frenzy does beyond freeing its record. The script
    runs the frenzy once `IS_POWERUP_DONE` sees it gone. Until those
    commands are ported, gfa-sim's stand-in (an endless machine gun for
    1,500 ticks) runs it.
