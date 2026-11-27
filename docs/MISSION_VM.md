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
game. A new process takes the first free slot from 4 (`PROC_START`,
0x75e90). The process's parent is the player who set off its trigger. A
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
  `DECCOUNT`.
- **Does not pay:** `MOBILE_BRIEF`, `BRIEF`, `STARTUP`, `KICKSTART`. In the
  brief commands, p5 is the text key.

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
  - It marks p1's trigger as started, calls 0x10b50(10, 0, 20) (not read),
    and moves on by p2.
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
  - Then it runs eight clean-up routines, not read yet: 0x765b0, 0x76490,
    0x766e0, 0xe8460, 0xe84a0, 0xfef50, 0xe7670 and 0xf20f0.
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
    the block's centre.
  - It pays and moves on by p2.
- **`ARROW_OFF`**: it puts the arrow out, pays and moves on by p2.
- **`MOBILE_BRIEF`**: it shows text p5 to the root player (0xf1cd0) and
  moves on by p2. A p2 of -1 ends the process through the routine that
  re-arms its trigger.
- **`BRIEF`**: it uses p1 (a declaration's position, through 0x7acf0), p3
  (its sign matters), p4 and p5 (0xf1b70). AmiGTA1 reads p5 only.

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
- `MISSION_COUNTER`, `SECRET_MISSION_COUNTER` and `MISSION_TOTAL` keep
  their first number in both, and also in globals (0x2d2600 to 0x2d2610).

**The scan** (inside 0xf3800) takes the triggers in record order and skips
disabled ones. For each trigger it checks every player:
- **His state.** It reads the player's state (player +0x31). A player in
  state 1 is skipped. That reads as getting in or out of a car, or being
  down; 0 is on foot and 2 is in a car.
- **The range test** compares blocks, inclusively. The player's block (his
  position >> 22) must lie within the trigger's x−r to x+r and y−r to
  y+r, so a radius of 0 is the trigger's own block. AmiGTA1 rounds 0 up to
  1 and measures from the block's centre.
- **Per type.** Some types first pass a check of their own (the switch at
  0xf41cc). Type 4 has none, so a `TRIGGER` fires on foot or in a car.
- **Firing.** An armed trigger in range goes to state 0, and the type's
  case runs (the switch at 0xf594a). Type 4 starts its line with the
  player as parent, which marks it started, and records the new process
  (0x76970).
- **The kill link.** If the trigger's kill link is set (and its type is
  not 0x21), that process ends too.

The other 33 types have not been read one by one yet.

**`ENABLE`'s per-kind routine** (0x7aa60) does nothing for most kinds,
including `TRIGGER`, `MPHONES`, `PHONE_TOGG`, `CARTRIGGER`, `SPRAY` and
`COUNTER`; for those, `ENABLE` only re-arms. It has work of its own for
these kinds, which are not read yet:
- the doors and gates;
- `ONETRIGGER`, `MOVING_TRIG` and `DUM_MISSION_TRIG`;
- `CORRECT_CAR_TRIG`, kinds 45 to 58, and the watch triggers from 59 to
  69.

## The port so far

`gfa_sim::mission::Vm` follows this file. Test scripts turn it on with
`# mission: on`.

- **Ported:**
  - the frame;
  - the slots, parents, states and keep classes;
  - the two advance helpers;
  - pay;
  - the check on the points target after every command (the target itself
    waits for the career data, M8);
  - the trigger records in load order;
  - the scan for type 4;
  - the commands marked done in `MISSION_OPCODES.md`.
- **What the coverage report counts:** a command is done when every use of
  it in GTA's scripts runs as above, and in part when some uses wait.
- **What waits:** a command not ported, and any use of one that gfa-sim
  cannot serve yet. The process waits on it, logged once with the
  section and line. It never takes the success path.
- **Ours:** where GTA8.EXE writes outside its arrays, gfa-sim does nothing
  and logs the line. That covers a start with no free slot (the
  `KICKSTART` child), a trigger index out of range, and a `KICKSTART` with
  no record free.
- **Next:**
  - `POWERUP_ON`, the opening's next command; with it, the crates become
    the script's;
  - then the phones (`MPHONE`, `ANSWER`, the `MPHONES` and `PHONE_TOGG`
    triggers) and `RESET`, which start Liberty City's first job.
- **Still to read:**
  - the other trigger types;
  - the per-kind `ENABLE` routine;
  - `RESET`'s eight clean-ups;
  - 0x10b50;
  - the score routine's multiplier (0xca190);
  - what slots 0 to 3 run in a single-player game.
