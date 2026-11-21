/* walktrace: the AmiGTA1 oracle for the player on foot.
 *
 * Links AmiGTA1's own portable modules (gta_map.c, gta_nav.c,
 * gta_player.c, gta_trig.c), compiled from a sibling ../AmiGTA1 checkout by
 * build.sh, and plays a walk script through them. It prints one line per
 * tick in the format of `gfa-tool run --trace`, so the two can be diffed
 * tick for tick, plus gtadump-style leg lines.
 *
 * Unlike `gtadump walk`, it builds the navigation grid first, so fences
 * (thin walls) are in force as they are in AmiGTA1's game. It also restates
 * the walk into water and the drowning from gta_main.c's main loop, and
 * runs the fleet, the trains and the crowd around him (fleet.h), with
 * every car held off him as he walks. The style is read for the fleet's car table.
 * His fire key is restated from gta_main.c too: the punch (its standing
 * animation and the blow on its third state), the guns' cooldown, the shot
 * and the panic it starts, the ammunition, and what the weapons do to him
 * (bullets, blasts, fire) down to WASTED.
 *
 *     walktrace <map.cmp> <style.gry> <script.txt>
 *
 * The script is the walk format: `start bx by`, then `turn forward walk
 * ticks` legs, which keep the fire key as it was; `fire ticks`, which holds
 * it on top of the leg's keys; `wait ticks`, which lets every key go;
 * `weapon w rounds`, which hands him a gun; and the fixtures `ped dx dy
 * angle` (somebody put down beside him, the farthest of a full crowd making
 * room) and `park model dx dy face` (a car left standing beside him), as
 * gta_main.c does them, each lasting a tick. `#` lines and blank lines are
 * skipped. After each tick's player and street lines comes
 *
 *     s <tick> <score> <heat> <level> <health> <weapon> <ammo>
 *
 * Grand Fair Use Auto, MIT.
 */
#include <stdio.h>
#include <string.h>

#include "gta_map.h"
#include "gta_nav.h"
#include "gta_player.h"
#include "gta_style.h"
#include "gta_trig.h"
#include "fleet.h"

/* gta_main.c's fire latch and the player's guns. */
static int fire_held, weapon, punch_left, fire_cool, burning;
static int ammo[GTA_WEAPON_COUNT];
static int ammo_sub[GTA_WEAPON_COUNT] = { 0, 0, GTA_AMMO_PER_UNIT, 0,
                                          GTA_AMMO_PER_UNIT };

static const char *const ground_name[8] = {
    "air", "water", "road", "pavement", "field", "building", "?6", "?7"
};

/* The layer of the water under this one with only air between, or -1
 * (gta_main.c's water_below). */
static int water_below(const gta_map *m, int bx, int by, int z)
{
    for (; z >= 0; z--) {
        int g = gta_map_ground_type(m, bx, by, z);
        if (g != GTA_GROUND_AIR)
            return g == GTA_GROUND_WATER ? z : -1;
    }
    return -1;
}

static void trace(long tick, const gta_player *p)
{
    printf("t %ld x %08lx y %08lx z %d a %d anim %d f %d %d b %d%d g %d\n",
           tick, (unsigned long)p->x & 0xFFFFFFFFUL,
           (unsigned long)p->y & 0xFFFFFFFFUL, p->layer, p->angle,
           (int)p->anim, p->frame, p->frame_tick, p->blocked_x ? 1 : 0,
           p->blocked_y ? 1 : 0, p->ground);
}

int main(int argc, char **argv)
{
    static gta_map map;
    static gta_nav nav;
    static gta_style style;
    static gta_tiles tiles;
    gta_player pl;
    FILE *script;
    char line[256];
    int sx = 64, sy = 64, leg = 0, drown = 0, health = 100;
    int turn = 0, fwd = 0, walk = 0;
    long tick = 0;

    if (argc != 4) {
        fprintf(stderr, "usage: walktrace <map.cmp> <style.gry> <script.txt>\n");
        return 2;
    }
    if (gta_map_load(argv[1], &map) != 0)
        return 1;
    if (gta_nav_build(&nav, &map) != 0) {
        fprintf(stderr, "walktrace: no memory for the nav grid\n");
        return 1;
    }
    if (gta_style_load(argv[2], &style) != 0)
        return 1;
    tiles.cars = style.cars;
    tiles.n_cars = style.car_count;
    script = fopen(argv[3], "r");
    if (!script) {
        fprintf(stderr, "walktrace: cannot open %s\n", argv[3]);
        return 1;
    }
    while (fgets(line, sizeof line, script)) {
        if (line[0] == '#' || line[0] == '\n' || line[0] == '\r')
            continue;
        if (sscanf(line, "start %d %d", &sx, &sy) == 2)
            break;
        fprintf(stderr, "walktrace: the first line must be `start bx by`\n");
        return 1;
    }
    fleet_start(&map, &nav, &tiles, sx, sy);
    gta_player_init(&pl, &map, &tiles, sx, sy);
    printf("player: start (%d,%d) layer %d angle %d ground %s\n", sx, sy,
           pl.layer, pl.angle, ground_name[pl.ground & 7]);
    trace(tick, &pl);

    while (fgets(line, sizeof line, script)) {
        int ticks, i, a, b, is_leg = 0;
        if (line[0] == '#' || line[0] == '\n' || line[0] == '\r')
            continue;
        if (sscanf(line, "fire %d", &ticks) == 1) {
            fire_held = 1;
        } else if (sscanf(line, "wait %d", &ticks) == 1) {
            turn = fwd = walk = 0;
            fire_held = 0;
        } else if (sscanf(line, "ped %d %d %d", &a, &b, &i) == 3) {
            int fi, worst = -1;
            long worstd = -1;
            for (fi = 0; fi < GTA_MAX_PEDS; fi++) {
                long dx, dy, d;
                if (!crowd.p[fi].alive) { worst = -1; break; }
                dx = (crowd.p[fi].x - pl.x) >> 16;
                dy = (crowd.p[fi].y - pl.y) >> 16;
                d = dx * dx + dy * dy;
                if (d > worstd) { worstd = d; worst = fi; }
            }
            if (worst >= 0)
                crowd.p[worst].alive = 0;
            gta_peds_drop(&crowd, pl.x + ((long)a << 16), pl.y + ((long)b << 16),
                          pl.layer, i & 255, -1, 0);
            ticks = 1;
        } else if (sscanf(line, "park %d %d %d %d", &a, &b, &i, &ticks) == 4) {
            gta_traffic_abandon(&fleet, a, pl.x + ((long)b << 16),
                                pl.y + ((long)i << 16), ticks & 255, pl.layer,
                                -1, 0);
            ticks = 1;
        } else if (sscanf(line, "weapon %d %d", &a, &b) == 2) {
            if (a >= 0 && a < GTA_WEAPON_COUNT) {
                weapon = a;
                ammo[a] = b;
            }
            ticks = 1;
        } else if (sscanf(line, "%d %d %d %d", &a, &b, &i, &ticks) == 4) {
            turn = a; fwd = b; walk = i;
            is_leg = 1;
        } else
            continue;
        if (ticks < 1 && !is_leg)
            ticks = 1;
        for (i = 0; i < ticks; i++) {
            long speed;
            int hb, hc, bl, bu;
            if (!drown && punch_left > 0 && fwd == 0) {
                /* THE STANDING PUNCH, a state of its own; he may turn. */
                pl.anim = GTA_ANIM_PUNCH;
                pl.frame = (GTA_PUNCH_TICKS * GTA_PED_PUNCH_FRAMES - punch_left)
                         / GTA_PUNCH_TICKS;
                pl.angle = (pl.angle + turn * 5) & 255;
            } else if (!drown) {
                long ox = pl.x, oy = pl.y;
                gta_player_update(&pl, &map, turn, fwd, walk);
                fleet_hold_walker(&pl.x, &pl.y, pl.layer, ox, oy);
                /* WATER IS NOT A WALL TO A MAN WHO WALKS INTO IT: the probe
                 * from gta_main.c's main loop, 10 px ahead while UP is held. */
                if (fwd > 0) {
                    long ax = pl.x + (long)gta_sin(pl.angle) * 40;
                    long ay = pl.y - (long)gta_cos(pl.angle) * 40;
                    int wz = water_below(&map, (int)(ax >> 21), (int)(ay >> 21),
                                         pl.layer);
                    if (wz >= 0) {
                        pl.x = ax;
                        pl.y = ay;
                        pl.layer = wz;
                        drown = 1;
                        printf("player: in the water at (%d,%d)\n",
                               (int)(ax >> 21), (int)(ay >> 21));
                    }
                }
            }
            /* THE FIRE LATCH (gta_main.c), on his feet only. */
            if (fire_cool > 0)
                fire_cool--;
            if (punch_left > 0) {
                punch_left--;
                if (punch_left == GTA_PUNCH_TICKS * (GTA_PED_PUNCH_FRAMES - 2))
                    gta_peds_punch(&crowd, pl.x, pl.y, pl.angle, pl.layer);
            }
            if (fire_held && !drown) {
                if (weapon == 0) {
                    if (punch_left == 0)
                        punch_left = GTA_PUNCH_TICKS * GTA_PED_PUNCH_FRAMES;
                } else if (fire_cool == 0 && ammo[weapon] > 0
                           && gta_weapons_fire(&weapons, weapon, pl.x, pl.y,
                                               pl.layer, pl.angle,
                                               pl.anim == GTA_ANIM_RUN, -1)) {
                    fire_cool = gta_weapons_cooldown(weapon);
                    gta_peds_panic(&crowd, pl.x, pl.y, pl.layer);
                    if (ammo_sub[weapon] > 0) {
                        if (--ammo_sub[weapon] == 0) {
                            ammo_sub[weapon] = GTA_AMMO_PER_UNIT;
                            ammo[weapon]--;
                        }
                    } else
                        ammo[weapon]--;
                    if (ammo[weapon] <= 0) {
                        ammo[weapon] = 0;
                        weapon = ammo[GTA_WEAPON_PISTOL] > 0
                               ? GTA_WEAPON_PISTOL : GTA_WEAPON_FIST;
                    }
                }
            }
            speed = pl.anim == GTA_ANIM_RUN || pl.anim == GTA_ANIM_WALK
                  ? GTA_RUN_SPEED_FP : 0;
            if (fleet_tick(pl.x, pl.y, speed, pl.angle, pl.layer, 3, 3, 1,
                           pl.anim == GTA_ANIM_RUN || pl.anim == GTA_ANIM_WALK)) {
                printf("player: run over by a train\n");
                health = 0;
            }
            /* WHERE THE WEAPONS FIND HIM next tick, and what they did to
             * him this one (gta_main.c). */
            gta_weapons_set_player(&weapons, pl.x, pl.y, pl.layer, 0, 0, 0);
            gta_weapons_player_damage(&weapons, &hb, &hc, &bl, &bu);
            for (; hb > 0; hb--)
                health -= 10;
            if (bl > 0)
                health = 0;
            if (bu > 0 && burning < 100)
                burning = 100;
            if (burning > 0) {
                burning--;
                health--;
            }
            /* gta_main.c: 3 health every other tick under water. */
            if (drown && (drown++ & 1))
                health -= 3;
            if (health <= 0) {
                /* Wasted: the guns and the heat go. AmiGTA1 shows the card
                 * and starts again at a hospital; gfa-sim, until the card
                 * comes, starts again where the script began, and so does
                 * this. */
                printf("player: wasted\n");
                gta_player_init(&pl, &map, &tiles, sx, sy);
                drown = 0;
                health = 100;
                burning = 0;
                for (a = 1; a < GTA_WEAPON_COUNT; a++)
                    ammo[a] = 0;
                weapon = 0;
                gta_score_clear_heat(&score);
            }
            trace(++tick, &pl);
            fleet_dump(tick);
            printf("s %ld %ld %d %d %d %d %d\n", tick, score.score, score.heat,
                   score.level, health, weapon, ammo[weapon]);
        }
        if (!is_leg)
            continue;
        printf("leg %2d: turn %2d fwd %2d walk %d x%3d  ->  block (%3d,%3d) "
               "+%2ld.%02ld,%2ld.%02ld  layer %d  angle %3d  ground %-8s%s%s\n",
               leg, turn, fwd, walk, ticks, (int)(pl.x >> 21), (int)(pl.y >> 21),
               (pl.x >> 16) & 31, ((pl.x & 0xFFFF) * 100) >> 16,
               (pl.y >> 16) & 31, ((pl.y & 0xFFFF) * 100) >> 16, pl.layer,
               pl.angle, ground_name[pl.ground & 7],
               pl.blocked_x ? "  BLOCKED-X" : "", pl.blocked_y ? "  BLOCKED-Y" : "");
        leg++;
    }
    fclose(script);
    gta_nav_free(&nav);
    gta_map_free(&map);
    return 0;
}
