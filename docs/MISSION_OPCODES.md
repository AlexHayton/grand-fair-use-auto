# Mission commands and declarations

This lists every command and declaration kind that the single-player
sections of GTA's `MISSION.INI` use: how often each is used, in which
sections, and what gfa-sim does of it. It is the work list for M7's mission
script interpreter (the VM). The names and numbers live in
`gfa_sim::mission`.

How the original runs a script (its processes, triggers and the commands read so far) is in `MISSION_VM.md`.

The tables at the end come from `gfa-tool mission coverage --markdown`.
`cargo data-test` fails when they no longer match the user's data or
gfa-sim: see the test `mission_coverage_doc_is_current`. Without
`--markdown`, the command prints the same report for a terminal, with each
name's first use. Give it another mission file to report on London's
scripts, and add `--multiplayer` to count the multiplayer sections too.

## How GTA8.EXE reads a script

These addresses are GTA8.EXE's load addresses, read with
`tools/dosprobe/gta8.py`.

- **Names.** Two tables of string pointers, each ending in 0:
  - 72 declaration kinds, from 0x132828;
  - 150 commands, from 0x13294c.

  A name's position in its table is its number. `gfa_sim::mission::DeclKind`
  and `Op` keep those numbers. The lookup (inlined at 0x7988a and 0x7a5fa)
  compares whole words and takes the first match.
- **The reader.** The script is read as a stream of characters.
  - Spaces, tabs and line breaks all separate words, and text in `{…}` is
    skipped.
  - The number reader (0xa82f0) takes an optional `-` and then digits. At
    anything else it returns 0 without moving on.
  - Line breaks mean nothing to it, so the reader counts numbers, not
    lines.
- **A declaration** is read as:
  1. its label;
  2. the optional flag;
  3. `(x,y,z)`;
  4. the kind's name;
  5. two numbers.

  The game then switches on the kind (0x798e1). An unknown kind stops the
  game while it loads: error -171 at 0x7a588. The error routine at 0x436e0
  ends in `exit(-1)`.
- **A command** is read by 0x7a5c3 as:
  1. its label;
  2. its name;
  3. exactly five numbers.

  These go into a 22-byte record at 0x26d6e0: the opcode as a u16, then
  five i32. The label goes into a table at 0x2924ee, two bytes per label
  (0 to 65535), that holds the record's index. A name not in the table
  loads as opcode 0xFFFF.
- **Running.** The dispatcher at 0x95347 compares the opcode with 0x95 and
  jumps through the table at 0x89464.
  - Anything above 0x95, which includes an unknown name, goes to 0x95313.
    That path finds the record's label and calls the error routine with
    -182 and the label, so the game stops.
  - An unknown command is therefore not an error at load. It stops the game
    when a process reaches it, and it never succeeds.
  - `DONOWT`, `DRIVEON` and the two `XXXX` entries share one empty case
    (0x89731).

All 20,130 command lines in GTA's single-player sections carry exactly five
numbers, and every name they use is in GTA8.EXE's tables. The data test
checks both.

## Sources for each command

- **AmiGTA1** (MIT, `native/gta_script.c`, `gta_script_run.c`) names every
  command these sections use (129) and every declaration kind (50). Its
  `step()` has a case for each command, so it is the port source. Most of
  its numbers are the original's. It numbers eight from 150 and 200 up
  because it did not know their place. GTA8.EXE's tables put them here:
  - commands `PLAYER_ARE_BOTH_ONSCREEN` (84), `IS_PLAYER_ON_TRAIN` (125) and
    `WRECK_A_TRAIN` (126);
  - declarations `GTA_DEMAND` (36), `TARGET_SCORE` (41), `MISSION_COUNTER`
    (66), `SECRET_MISSION_COUNTER` (67) and `MISSION_TOTAL` (68).
- **Carnage3D's `docs/ped.txt`** is Ben "Cerbera" Millard's 2004 study of
  people in missions. It covers `PED_ON`'s colours, `FUTUREPED`, and how
  friendly and enemy people's bullets work.
- **The original running under DOSBox-X**, for what neither source settles
  (`docs/M6B_PROBES.md`).

## London (M11)

Each London executable carries its own command table. Nothing of their
numbering has been read yet.

- `gta8_uk.exe`, `gta24_uk.exe`, `gta8_61.exe` and `Gta24_61.exe` add
  `RED_ARROWCAR`. `MISSUK.INI` uses it once.
- Only `Gta24_61.exe` has `PLAYER_DRIVE_BY` and `PLAYER_DRIVE_BY_OFF`.
  `MISSUKE.INI` uses them 8 and 13 times, but `gta8_61.exe` lacks both.
- One `MOBILE_BRIEF` in `MISSUKE.INI` carries a sixth number (line 2604).
  GTA8.EXE's reader would take it as the next command's label. That would
  insert an unknown command, and the next line would be read with label 0.

The test `london_uses_commands_gta8_lacks` holds all three facts.

## Multiplayer

The multiplayer sections are deathmatch and the Canonball Runs, numbered
1000 and up in all three games. Multiplayer is out of scope. Four of their
declaration kinds appear in no single-player section: `MIDPOINT_MULTI`,
`MID_MULTI_SETUP`, `FINAL_MULTI` and `CANNON_START`. AmiGTA1 has none of
them.

## What gfa-sim does

- **The interpreter** (`gfa_sim::mission::Vm`) is ported from GTA8.EXE's
  live code (`MISSION_VM.md`). The game runs it from the start of every
  level. A test script turns it on with `# mission: on`.
- **Commands it carries out:**
  - flow and processes: `DONOWT`, `SURVIVE`, `STARTUP`, `KICKSTART`,
    `KEEP_THIS_PROC`, `KILL_SIDE_PROC`;
  - triggers and counters: `DISABLE`, `COMPARE`, `INCCOUNT`, `DECCOUNT`;
  - crates: `POWERUP_OFF`, `IS_POWERUP_DONE`;
  - text and the arrow: `MOBILE_BRIEF`, `ARROW_OFF`.
- **Commands it carries out in part:**
  - `ENABLE`: it waits when the declaration is a door or a watch trigger.
  - `ARROW`: it waits when the target is something that moves.
  - `GENERAL_ONSCREEN`: it waits when the target is something that moves.
    It tests gfa-sim's view by blocks, where the original tests a
    rectangle in pixels whose making has not been read.
  - `POWERUP_ON`: it waits when the label names no `POWERUP`. That is 56
    uses in sections [102] and [103], which in the original read
    declaration record 0.
  - `DOOR_ON` and `DOOR_OFF`: they wait on the gates and specific doors
    (20 uses), whose loaders have not been read.
  - `MPHONE`: it waits on its one use whose labels name no `TELEPHONE`.
  - `RESET` and `MPHONE`: their clean-ups skip the mission's own cars,
    peds and objects, which no ported command makes yet.
- **Any other command:** a process that reaches it waits there and logs
  its section and line once. It never takes the command's success path.
  AmiGTA1 takes it for a command it lacks.
- **Declarations:**
  - The interpreter fires `TRIGGER`s, `MPHONES` and `PHONE_TOGG`s, counts
    with the counters and rings the `TELEPHONE`s, which are not drawn yet.
    Every other kind that makes a trigger gets its record in the
    original's order, but none of those fires yet.
  - Each `DOOR` gets its door record, which `DOOR_ON`, `DOOR_OFF` and
    `RESET` use. The door itself is not made yet, so it never moves.
  - The world reads five kinds when it loads a level:
    - `PLAYER` (the start);
    - `POWERUP`: a crate on every one, which only a test script without
      the interpreter keeps. With the interpreter on, the crates are the
      script's `POWERUP_ON`s', as in the original;
    - `SPRAY`, `BOMBSHOP` and `BOMBSHOP_COST` (the shops).

## The tables

GTA's `MISSION.INI`, single-player sections. `No.` is the original's
number. In the gfa-sim column, `yes` marks a command the VM carries out or
a declaration kind the world reads.

<!-- BEGIN gfa-tool mission coverage --markdown -->
Sections [1] [2] [102] [103] [202] [203].

Commands: 129 of the original's 150 in use, on 20130 lines. gfa-sim runs 14 of them, 8075 of the 20130 lines (40.1%), and 8 more in part (3189 lines).

Declarations: 50 of the original's 72 kinds in use, on 4568 lines. gfa-sim reads 14 of them, 2566 of the 4568 lines (56.1%).

| No. | Command                    | Lines | [1] | [2] | [102] | [103] | [202] | [203] | gfa-sim |
| --: | -------------------------- | ----: | --: | --: | ----: | ----: | ----: | ----: | ------- |
|   4 | `SURVIVE`                  |  1924 | 287 | 410 |   341 |   269 |   257 |   360 | yes     |
|  18 | `DISABLE`                  |  1425 | 141 | 226 |   214 |   244 |   267 |   333 | yes     |
|  89 | `KICKSTART`                |  1174 |  78 | 104 |   198 |   153 |   326 |   315 | yes     |
|  64 | `POWERUP_ON`               |  1092 | 175 | 159 |   215 |   212 |   163 |   168 | part    |
| 111 | `MOBILE_BRIEF`             |  1059 | 125 | 164 |   176 |   145 |   281 |   168 | yes     |
|  36 | `ARROW_OFF`                |  1041 |  99 | 141 |   213 |   150 |   282 |   156 | yes     |
|  69 | `CHANGE_PED_TYPE`          |   970 |  33 |  52 |    73 |   115 |   219 |   478 |         |
| 105 | `IS_GOAL_DEAD`             |   925 |  83 |  84 |    61 |   120 |   163 |   414 |         |
|  19 | `ENABLE`                   |   793 |  95 | 115 |   121 |   143 |   156 |   163 | part    |
| 116 | `MESSAGE_BRIEF`            |   669 |  75 |  94 |   138 |   104 |   117 |   141 |         |
| 129 | `RESET`                    |   497 |  43 |  61 |    85 |    50 |   164 |    94 | part    |
|   1 | `DESTROY`                  |   452 |   4 |   6 |    67 |    58 |   130 |   187 |         |
|  20 | `DECCOUNT`                 |   435 |  39 |  35 |    59 |    62 |    87 |   153 | yes     |
| 101 | `KILL_SPEC_PROC`           |   418 |  16 |  20 |    85 |    47 |   160 |    90 |         |
|  26 | `ARROW`                    |   393 |  72 |  93 |    91 |    62 |    39 |    36 | part    |
|  31 | `PED_ON`                   |   392 |  23 |  39 |    54 |    98 |    85 |    93 |         |
| 128 | `COMPARE`                  |   331 |  64 |  67 |    11 |    12 |    65 |   112 | yes     |
| 110 | `SPEECH_BRIEF`             |   313 |  36 |  54 |    49 |    30 |    57 |    87 |         |
| 142 | `RESET_KF`                 |   280 |  21 |  21 |    34 |    34 |    80 |    90 |         |
|   3 | `STEAL`                    |   255 |  21 |  35 |    59 |    24 |    90 |    26 |         |
| 106 | `GENERAL_ONSCREEN`         |   247 |  23 |  18 |     9 |    16 |    50 |   131 | part    |
|  52 | `ARROWCAR`                 |   245 |  35 |  57 |    31 |    33 |    60 |    29 |         |
|  67 | `CHANGE_TYPE`              |   242 |   0 |   0 |     9 |     1 |   205 |    27 |         |
|  90 | `IS_PED_IN_CAR`            |   223 |  38 |  56 |    43 |    17 |    46 |    23 |         |
|  98 | `KILL_SIDE_PROC`           |   179 |  79 | 100 |     0 |     0 |     0 |     0 | yes     |
|  51 | `ARROWPED`                 |   170 |  19 |  12 |    32 |    36 |    35 |    36 |         |
|  65 | `PLAIN_EXPL_BUILDING`      |   167 |  40 |   0 |     0 |    17 |    48 |    62 |         |
| 114 | `PED_WEAPON`               |   164 |   0 |   1 |     4 |    39 |    21 |    99 |         |
| 133 | `IS_POWERUP_DONE`          |   158 |  18 |  18 |    20 |    22 |    38 |    42 | yes     |
| 127 | `INCCOUNT`                 |   150 |  24 |  26 |    18 |    19 |    32 |    31 | yes     |
| 135 | `FRENZY_BRIEF`             |   140 |   7 |   7 |    13 |    13 |    47 |    53 |         |
|  88 | `PARKED_ON`                |   135 |   6 |  15 |    26 |    28 |    42 |    18 |         |
|  38 | `PED_SENDTO`               |   131 |   9 |  24 |     1 |    26 |    21 |    50 |         |
| 134 | `STOP_FRENZY`              |   122 |  21 |  21 |    18 |    18 |    20 |    24 |         |
|  91 | `CANCEL_BRIEFING`          |   117 |   8 |   7 |    55 |    15 |     5 |    27 |         |
| 119 | `DROP_WANTED_LEVEL`        |   114 |  11 |  16 |    18 |    20 |    27 |    22 |         |
| 130 | `KEEP_THIS_PROC`           |   113 |   6 |   6 |    36 |    22 |    21 |    22 | yes     |
|  37 | `P_BRIEF`                  |   111 |  12 |  14 |    28 |    29 |     0 |    28 |         |
| 141 | `KF_PROCESS`               |   102 |   7 |   7 |    12 |    12 |    30 |    34 |         |
|  70 | `DOOR_ON`                  |   101 |  14 |  38 |    24 |    25 |     0 |     0 | part    |
|  39 | `WAIT_FOR_PED`             |   100 |   9 |  24 |     1 |     6 |    13 |    47 |         |
|  45 | `MISSION_END`              |    91 |  11 |  14 |    18 |    16 |    16 |    16 |         |
| 132 | `DEAD_ARRESTED`            |    90 |   8 |   8 |    24 |    17 |    14 |    19 |         |
|  15 | `BRIEF`                    |    78 |   5 |   7 |    16 |    13 |    17 |    20 |         |
| 104 | `FRENZY_CHECK`             |    70 |   7 |   7 |    12 |    12 |    15 |    17 |         |
| 103 | `FRENZY_SET`               |    70 |   7 |   7 |    12 |    12 |    15 |    17 |         |
|  85 | `CHECK_CAR`                |    59 |  22 |  31 |     4 |     1 |     0 |     1 |         |
|   8 | `DUMMYON`                  |    55 |   0 |   8 |     6 |    17 |     9 |    15 |         |
| 109 | `LOCATE`                   |    55 |   0 |   0 |    21 |    18 |    15 |     1 |         |
|  71 | `DOOR_OFF`                 |    54 |   2 |  12 |     6 |     4 |    15 |    15 | part    |
| 138 | `KF_CANCEL_BRIEFING`       |    54 |   0 |   0 |    12 |    12 |    15 |    15 |         |
|  79 | `KILL_PED`                 |    54 |  10 |  12 |     1 |     3 |    12 |    16 |         |
|  23 | `PARK`                     |    51 |   7 |  18 |     6 |     6 |    12 |     2 |         |
|  17 | `DONOWT`                   |    48 |  16 |  27 |     2 |     1 |     1 |     1 | yes     |
|   2 | `ANSWER`                   |    47 |   5 |   5 |    16 |     8 |     5 |     8 |         |
|   9 | `SENDTO`                   |    46 |   0 |  17 |     3 |    13 |    10 |     3 |         |
|  12 | `EXPLODE`                  |    45 |  20 |  14 |     0 |    11 |     0 |     0 |         |
|  33 | `CAR_ON`                   |    42 |   0 |   7 |     0 |    15 |     7 |    13 |         |
|  68 | `P_BRIEF_TIMED`            |    42 |   6 |   7 |    13 |     7 |     2 |     7 |         |
|  27 | `LOCK_DOOR`                |    41 |   0 |   2 |    14 |    10 |     3 |    12 |         |
|  78 | `GET_DRIVER_INFO`          |    40 |   0 |   3 |     6 |    11 |    10 |    10 |         |
|  75 | `PED_OUT_OF_CAR`           |    40 |   0 |   3 |     6 |    11 |    10 |    10 |         |
|  49 | `RETURN_CONTROL`           |    37 |   0 |   5 |     6 |     8 |     9 |     9 |         |
|  24 | `SETBOMB`                  |    35 |   7 |   6 |     1 |     1 |    10 |    10 |         |
|  21 | `GOTO`                     |    33 |   0 |   0 |    13 |     6 |    12 |     2 |         |
| 139 | `KF_BRIEF_GENERAL`         |    32 |   0 |   0 |     0 |     0 |    15 |    17 |         |
| 140 | `KF_CANCEL_GENERAL`        |    32 |   0 |   0 |     0 |     0 |    15 |    17 |         |
| 100 | `POWERUP_OFF`              |    32 |   0 |   0 |     0 |     0 |    15 |    17 | yes     |
|  74 | `CLOSE_DOOR`               |    31 |   3 |  14 |     3 |     4 |     6 |     1 |         |
| 120 | `IS_PED_ARRESTED`          |    27 |   4 |   6 |     1 |     4 |     6 |     6 |         |
|  28 | `UNLOCK_DOOR`              |    27 |   0 |   3 |     6 |     5 |     3 |    10 |         |
|  66 | `CHANGE_BLOCK`             |    26 |   0 |   0 |     7 |     1 |     9 |     9 |         |
| 136 | `ADD_A_LIFE`               |    24 |   0 |   0 |    12 |    12 |     0 |     0 |         |
|  22 | `CRANE`                    |    24 |   4 |   4 |     4 |     4 |     4 |     4 |         |
|  53 | `REMAP_PED`                |    24 |   1 |   4 |     1 |     4 |     7 |     7 |         |
|  95 | `EXPL_NO_FIRE`             |    23 |   0 |   0 |    12 |    11 |     0 |     0 |         |
| 148 | `INC_HEADS`                |    22 |   0 |   2 |     0 |     0 |    15 |     5 |         |
| 137 | `KF_BRIEF_TIMED`           |    22 |   0 |   0 |     6 |     6 |     5 |     5 |         |
|  46 | `EXPLODE_CAR`              |    21 |   0 |   2 |     9 |     4 |     0 |     6 |         |
|  43 | `KILL_OBJ`                 |    21 |   1 |   4 |    10 |     2 |     4 |     0 |         |
|  73 | `OPEN_DOOR`                |    21 |   0 |   6 |     8 |     7 |     0 |     0 |         |
| 124 | `SET_PED_SPEED`            |    20 |   2 |   2 |     5 |     1 |     2 |     8 |         |
| 118 | `PED_POLICE`               |    19 |   0 |   0 |     6 |    10 |     3 |     0 |         |
|  40 | `PED_BACK`                 |    18 |   4 |   4 |     2 |     0 |     4 |     4 |         |
|  81 | `BANK_ALARM_OFF`           |    16 |   9 |   1 |     1 |     0 |     5 |     0 |         |
|  11 | `MAKEOBJ`                  |    16 |   0 |   0 |    10 |     2 |     4 |     0 |         |
| 113 | `SET_NO_COLLIDE`           |    15 |   0 |   0 |     0 |     5 |     5 |     5 |         |
|  83 | `DUMMY_DRIVE_ON`           |    14 |   0 |   0 |     6 |     5 |     3 |     0 |         |
| 149 | `IS_PED_STUNNED`           |    14 |   0 |   0 |     8 |     1 |     0 |     5 |         |
|  99 | `SET_KILLTRIG`             |    14 |   0 |   0 |     3 |     0 |     7 |     4 |         |
|  94 | `UNFREEZE_ENTER`           |    14 |   5 |   4 |     0 |     5 |     0 |     0 |         |
|  76 | `BANK_ROBBERY`             |    13 |   3 |   3 |     1 |     5 |     1 |     0 |         |
|  48 | `DO_MODEL`                 |    12 |   0 |   1 |     2 |     3 |     3 |     3 |         |
|  87 | `EXPL_PED`                 |    12 |   0 |   0 |     6 |     3 |     1 |     2 |         |
|  16 | `MPHONE`                   |    12 |   1 |   1 |     2 |     2 |     3 |     3 | part    |
|  47 | `START_MODEL`              |    12 |   0 |   1 |     2 |     3 |     3 |     3 |         |
| 107 | `GET_CAR_INFO`             |     9 |   0 |   0 |     3 |     4 |     1 |     1 |         |
| 121 | `HELL_ON`                  |     9 |   3 |   2 |     0 |     0 |     4 |     0 |         |
|  96 | `KILL_PROCESS`             |     9 |   0 |   0 |     9 |     0 |     0 |     0 |         |
|  97 | `NEXT_KICK`                |     8 |   0 |   0 |     7 |     0 |     1 |     0 |         |
| 144 | `RED_ARROW`                |     8 |   2 |   2 |     1 |     1 |     1 |     1 |         |
| 102 | `SCORE_CHECK`              |     8 |   1 |   1 |     2 |     2 |     1 |     1 |         |
|  10 | `END`                      |     7 |   2 |   1 |     1 |     1 |     1 |     1 |         |
|  93 | `FREEZE_ENTER`             |     7 |   2 |   1 |     0 |     4 |     0 |     0 |         |
|  92 | `FREEZE_TIMED`             |     7 |   1 |   0 |     0 |     3 |     2 |     1 |         |
|  58 | `MODEL_HUNT`               |     7 |   0 |   0 |     0 |     0 |     4 |     3 |         |
|  50 | `KILL_CAR`                 |     6 |   1 |   3 |     1 |     0 |     0 |     1 |         |
| 112 | `PIXEL_CAR_ON`             |     6 |   0 |   0 |     0 |     5 |     0 |     1 |         |
|  60 | `STARTUP`                  |     6 |   1 |   1 |     1 |     1 |     1 |     1 | yes     |
|  80 | `BANK_ALARM_ON`            |     5 |   2 |   1 |     1 |     0 |     1 |     0 |         |
| 146 | `RED_ARROW_OFF`            |     5 |   2 |   1 |     0 |     0 |     2 |     0 |         |
|  34 | `ARMEDMESS`                |     4 |   2 |   2 |     0 |     0 |     0 |     0 |         |
|  32 | `KILL_DROP`                |     4 |   2 |   2 |     0 |     0 |     0 |     0 |         |
|  35 | `DISARMMESS`               |     3 |   2 |   1 |     0 |     0 |     0 |     0 |         |
|  44 | `DO_GTA`                   |     3 |   2 |   1 |     0 |     0 |     0 |     0 |         |
|  30 | `DROP_ON`                  |     3 |   1 |   2 |     0 |     0 |     0 |     0 |         |
|  57 | `GOTO_DROPOFF`             |     3 |   1 |   2 |     0 |     0 |     0 |     0 |         |
|   6 | `HUNTOFF`                  |     3 |   0 |   0 |     0 |     0 |     2 |     1 |         |
|   5 | `HUNTON`                   |     3 |   0 |   0 |     0 |     0 |     2 |     1 |         |
| 125 | `IS_PLAYER_ON_TRAIN`       |     3 |   1 |   2 |     0 |     0 |     0 |     0 |         |
|  42 | `DO_REPO`                  |     2 |   0 |   2 |     0 |     0 |     0 |     0 |         |
| 115 | `FREEUP_CAR`               |     2 |   0 |   2 |     0 |     0 |     0 |     0 |         |
|  13 | `OBTAIN`                   |     2 |   0 |   2 |     0 |     0 |     0 |     0 |         |
| 117 | `PARKED_PIXELS_ON`         |     2 |   0 |   0 |     0 |     2 |     0 |     0 |         |
|  84 | `PLAYER_ARE_BOTH_ONSCREEN` |     2 |   1 |   0 |     0 |     0 |     0 |     1 |         |
| 145 | `RESET_WITH_BRIEFS`        |     2 |   0 |   0 |     2 |     0 |     0 |     0 |         |
|  41 | `SETUP_REPO`               |     2 |   0 |   2 |     0 |     0 |     0 |     0 |         |
|  14 | `THROW`                    |     2 |   0 |   2 |     0 |     0 |     0 |     0 |         |
| 126 | `WRECK_A_TRAIN`            |     2 |   1 |   1 |     0 |     0 |     0 |     0 |         |

In the original's table but used by none of these sections: `LOCATE_STOPPED`, `DRIVEON`, `ESCORT`, `EXPL_LAST`, `REMAP_CAR`, `ARE_BOTH_ONSCREEN`, `THROW_TO_POINT`, `MODEL_FUTURE`, `CHECK_PEDBACK`, `START_HIRED_ESC`, `DUMMY_OFF`, `WRECK_CURR_TRAIN`, `DELAY_CRIME`, `GARAGE_SEND`, `WAIT_FOR_PLAYER`, `START_CHOPPER`, `XXXX`, `SET_COLLIDE`, `WAIT_FOR_PLAYERS`, `IS_A_TRAIN_WRECKED`.

| No. | Declaration              | Lines | [1] | [2] | [102] | [103] | [202] | [203] | gfa-sim |
| --: | ------------------------ | ----: | --: | --: | ----: | ----: | ----: | ----: | ------- |
|  50 | `POWERUP`                |  1009 | 151 | 151 |   201 |   198 |   150 |   158 | yes     |
|   7 | `TELEPHONE`              |   691 | 128 | 128 |    90 |    89 |   128 |   128 | yes     |
|  24 | `FUTUREPED`              |   424 |  31 |  49 |    57 |    98 |    85 |   104 |         |
|   8 | `TRIGGER`                |   420 |  55 |  79 |    80 |    76 |    55 |    75 | yes     |
|  15 | `DUMMY`                  |   388 |  95 |  48 |    38 |    50 |    89 |    68 |         |
|  51 | `BLOCK_INFO`             |   253 |   0 |   1 |    16 |     2 |   206 |    28 |         |
|  29 | `FUTURECAR`              |   227 |  11 |  36 |    28 |    43 |    67 |    42 |         |
|  10 | `TARGET`                 |   171 |  13 |  25 |    15 |    47 |    20 |    51 | yes     |
|  12 | `COUNTER`                |   145 |  11 |  11 |    17 |    19 |    34 |    53 | yes     |
|   9 | `DOOR`                   |   143 |  15 |  31 |    31 |    33 |    12 |    21 |         |
|   5 | `PARKED`                 |    74 |   9 |  12 |     8 |     9 |    19 |    17 |         |
|  25 | `CARTRIGGER`             |    71 |   7 |  11 |     6 |     9 |    19 |    19 |         |
|  42 | `DUM_MISSION_TRIG`       |    68 |   1 |  38 |     2 |    13 |    11 |     3 |         |
|  44 | `CORRECT_CAR_TRIG`       |    55 |   0 |   0 |    13 |     9 |    26 |     7 |         |
|  16 | `SPRAY`                  |    50 |  15 |  15 |     5 |     5 |     5 |     5 | yes     |
|   1 | `PED`                    |    39 |   0 |   0 |     1 |     1 |    14 |    23 |         |
|  20 | `BOMBSHOP`               |    28 |   7 |   7 |     3 |     3 |     4 |     4 | yes     |
|  11 | `FUTURE`                 |    25 |   3 |   4 |    10 |     2 |     5 |     1 |         |
|  17 | `BARRIER`                |    24 |   0 |   8 |     8 |     8 |     0 |     0 |         |
|  13 | `CRANE`                  |    24 |   4 |   4 |     4 |     4 |     4 |     4 |         |
|  59 | `CARDESTROY_TRIG`        |    23 |   0 |   0 |     1 |     4 |     9 |     9 |         |
|  58 | `GUN_SCREEN_TRIG`        |    23 |   0 |   0 |     3 |     6 |     2 |    12 |         |
|  60 | `CARWAIT_TRIG`           |    22 |   0 |   0 |     6 |     8 |     2 |     6 |         |
|  57 | `GUN_TRIG`               |    20 |   3 |   3 |     0 |     0 |     5 |     9 |         |
|  40 | `SPECIFIC_DOOR`          |    19 |   0 |   0 |     4 |     2 |    11 |     2 |         |
|  53 | `MOVING_TRIG_HIRED`      |    14 |   0 |   0 |     3 |     5 |     4 |     2 |         |
|  34 | `MPHONES`                |    12 |   1 |   1 |     2 |     2 |     3 |     3 | yes     |
|  39 | `PHONE_TOGG`             |    12 |   1 |   1 |     2 |     2 |     3 |     3 | yes     |
|  55 | `CARBOMB_TRIG`           |     9 |   0 |   0 |     2 |     1 |     3 |     3 |         |
|  66 | `MISSION_COUNTER`        |     8 |   2 |   2 |     1 |     1 |     1 |     1 | yes     |
|  32 | `MOVING_TRIG`            |     8 |   0 |   0 |     0 |     4 |     0 |     4 |         |
|  27 | `HELLS`                  |     7 |   4 |   3 |     0 |     0 |     0 |     0 |         |
|  68 | `MISSION_TOTAL`          |     7 |   1 |   1 |     1 |     2 |     1 |     1 | yes     |
|  36 | `GTA_DEMAND`             |     6 |   3 |   3 |     0 |     0 |     0 |     0 |         |
|   3 | `PLAYER`                 |     6 |   1 |   1 |     1 |     1 |     1 |     1 | yes     |
|  67 | `SECRET_MISSION_COUNTER` |     6 |   1 |   1 |     1 |     1 |     1 |     1 | yes     |
|  41 | `TARGET_SCORE`           |     6 |   1 |   1 |     1 |     1 |     1 |     1 |         |
|  69 | `CARSTUCK_TRIG`          |     5 |   0 |   0 |     0 |     0 |     3 |     2 |         |
|  70 | `BASIC_BARRIER`          |     4 |   0 |   0 |     0 |     4 |     0 |     0 |         |
|  64 | `PARKED_PIXELS`          |     4 |   0 |   0 |     0 |     0 |     2 |     2 |         |
|  63 | `DUM_PED_BLOCK_TRIG`     |     3 |   0 |   0 |     0 |     3 |     0 |     0 |         |
|  65 | `CHOPPER_ENDPOINT`       |     2 |   0 |   0 |     0 |     0 |     1 |     1 |         |
|  43 | `CORRECT_MOD_TRIG`       |     2 |   0 |   0 |     1 |     0 |     0 |     1 |         |
|  56 | `DAMAGE_TRIG`            |     2 |   0 |   0 |     0 |     2 |     0 |     0 |         |
|  31 | `MODEL_BARRIER`          |     2 |   0 |   0 |     2 |     0 |     0 |     0 |         |
|  61 | `PEDCAR_TRIG`            |     2 |   0 |   0 |     1 |     0 |     0 |     1 |         |
|  33 | `SPECIFIC_BARR`          |     2 |   0 |   0 |     2 |     0 |     0 |     0 |         |
|  71 | `ALT_DAMAGE_TRIG`        |     1 |   0 |   0 |     1 |     0 |     0 |     0 |         |
|  38 | `BOMBSHOP_COST`          |     1 |   0 |   1 |     0 |     0 |     0 |     0 | yes     |
|   2 | `OBJECT`                 |     1 |   1 |   0 |     0 |     0 |     0 |     0 |         |

In the original's table but used by none of these sections: `CAR`, `DRIVER`, `MODEL`, `CLOCK`, `ESCORT`, `CARBOMB`, `ESCORTED`, `STOPPED`, `INIT`, `FUTUREDROP`, `DRIVER_PARK`, `ONETRIGGER`, `MODEL_DOOR`, `MY_MODEL`, `CLOCK_START`, `CLOCK_STOP`, `MIDPOINT_MULTI`, `MID_MULTI_SETUP`, `FINAL_MULTI`, `SPECIFIC_DOOR_BOMB`, `SETUP_SPEED`, `CANNON_START`.
<!-- END gfa-tool mission coverage --markdown -->
