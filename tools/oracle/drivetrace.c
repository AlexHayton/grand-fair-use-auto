/* drivetrace: the AmiGTA1 oracle for the driven car.
 *
 * Links AmiGTA1's own modules from a sibling ../AmiGTA1 checkout (build.sh):
 * the physics, wall resolve and layer rule (gta_vehphys.c), the GRY reader
 * for the car table (gta_style.c), the map and nav grid, and the fleet,
 * trains and crowd around him (fleet.h). The few lines of per-tick glue that live in
 * AmiGTA1's 11,000-line main loop (the road flag, the jump off a ramp and
 * its landing, the layer judged at the nose, wall damage, stopping at a
 * fleet car and ramming it, and running people over) are restated here, in the order gta_main.c
 * runs them.
 *
 * Prints one line per tick in the format of `gfa-tool run --trace` while
 * driving, and the fleet after it, so the two can be diffed tick for tick.
 *
 *     drivetrace <map.cmp> <style.gry> <script.txt> <bx> <by>
 *
 * (bx, by) is the level start, which the fleet is parked around.
 *
 * The script uses the order format: `drive record bx by face` once (it
 * lasts a tick, as fixture orders do), then `run ticks up down steer
 * handbrake` and `wait ticks`. `#` lines are skipped.
 *
 * Grand Fair Use Auto, MIT.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "gta_map.h"
#include "gta_nav.h"
#include "gta_style.h"
#include "gta_tiles.h"
#include "gta_vehphys.h"
#include "fleet.h"

#define LIFT     (1L << 16)
#define GRAVITY  (1L << 16)
#define Z_MAX    (160L << 16)

static int stand_layer(const gta_nav *nav, int bx, int by)
{
    int z;
    for (z = 0; z < GTA_MAP_LAYERS; z++) {
        int g = gta_nav_ground(gta_nav_at(nav, bx, by, z));
        if (g == 2 || g == 3 || g == 4)
            return z;
    }
    return -1;
}

static gta_map map;
static gta_nav nav;
static gta_style style;
static gta_tiles tiles;
static gta_veh veh;
static int layer, air, lift;
static long az, avz, flown;
static long tick;
static int driving;

static void one_tick(int up, int down, int steer, int hb)
{
    long x0 = veh.x, y0 = veh.y, ox0 = veh.ox, oy0 = veh.oy, a0 = veh.ang16;
    int road = gta_nav_ground(gta_nav_at(&nav, (int)(veh.ox >> 21),
                                         (int)(veh.oy >> 21), layer)) == 2;
    gta_veh_step(&veh, up, down, steer, hb, road);

    if (!air) {
        int bx = (int)(veh.ox >> 21), by = (int)(veh.oy >> 21);
        int u = gta_map_slope_up_dir(&map, bx, by, layer);
        if (u >= 0) {
            static const int dxs[4] = { 0, 1, 0, -1 };
            static const int dys[4] = { -1, 0, 1, 0 };
            long vx = veh.vx < 0 ? -veh.vx : veh.vx;
            long vy = veh.vy < 0 ? -veh.vy : veh.vy;
            long sp = (vx > vy ? vx : vy) >> 16;
            int q = (u >> 6) & 3;
            int going = (dxs[q] > 0 && veh.vx > 0) || (dxs[q] < 0 && veh.vx < 0)
                     || (dys[q] > 0 && veh.vy > 0) || (dys[q] < 0 && veh.vy < 0);
            int ax = bx + dxs[q], ay = by + dys[q];
            int solid = stand_layer(&nav, ax, ay) >= layer;
            int ahead = gta_map_slope_up_dir(&map, ax, ay, layer) >= 0;
            if (going && !solid && !ahead && sp >= 4) {
                air = 1; lift = (int)sp * 2; avz = LIFT; az = 0; flown = 0;
                printf("car: jump from (%d,%d) layer %d at %ld px/step\n", bx, by, layer, sp);
            }
        }
    }
    if (air) {
        long dx = veh.ox - ox0, dy = veh.oy - oy0;
        az += avz;
        flown += (dx < 0 ? -dx : dx) + (dy < 0 ? -dy : dy);
        if (lift > 0) lift--; else avz -= GRAVITY;
        if (az > Z_MAX) { az = Z_MAX; avz = 0; }
        if (az <= 0 && avz <= 0) {
            int bx = (int)(veh.ox >> 21), by = (int)(veh.oy >> 21);
            int lz = stand_layer(&nav, bx, by);
            if (lz >= 0 && lz <= layer) {
                printf("car: landed at (%d,%d) layer %d after %ld px\n", bx, by, lz, flown >> 16);
                layer = lz; air = 0; az = 0; avz = 0;
            } else if (az < -(32L << 16)) {
                int z = layer, g = 0;
                printf("car: jump fell short at (%d,%d)\n", bx, by);
                air = 0; az = 0; avz = 0;
                /* What it fell into answers: the first surface under the
                 * layer it left. Water takes the car (gta_main.c), and the
                 * trace of it ends. */
                for (; z >= 0; z--) {
                    g = gta_nav_ground(gta_nav_at(&nav, bx, by, z));
                    if (g != 0)
                        break;
                }
                if (z >= 0 && g == 1) {
                    printf("car: into the water at (%d,%d) layer %d\n", bx, by, z);
                    driving = 0;
                    tick++;
                    return;
                }
            }
        }
    }
    if (!air) {
        long nx0, ny0, nx1, ny1;
        int nz, hit;
        gta_veh_nose(&veh, ox0, oy0, a0, &nx0, &ny0);
        gta_veh_nose(&veh, veh.ox, veh.oy, veh.ang16, &nx1, &ny1);
        nz = gta_veh_layer(&nav, layer, (int)(nx0 >> 21), (int)(ny0 >> 21),
                           (int)(nx1 >> 21), (int)(ny1 >> 21), nx1 - nx0, ny1 - ny0);
        if (nz != layer) {
            printf("car: layer %d -> %d at (%d,%d)\n", layer, nz,
                   (int)(veh.ox >> 21), (int)(veh.oy >> 21));
            layer = nz;
        }
        hit = gta_veh_wall(&veh, &nav, layer, x0, y0, ox0, oy0, a0);
        if (hit - 1 > 0) {
            veh.damage += hit - 1;
            printf("car: wall hit at %d px/step - damage %d\n", hit, veh.damage);
        }
    }
    /* The fleet: back to the last clear point of the move if it ended
     * inside a car, then the ram's share for this car. */
    {
        const gta_car_info *ci = &tiles.cars[veh.model];
        long nx = veh.ox, ny = veh.oy, na = veh.ang16;
        long rvx, rvy, ryaw, rpx, rpy;
        int nhit;
        if (gta_traffic_sweep_box(&fleet, ox0, oy0, a0, &nx, &ny, &na,
                                  gta_car_world_len(ci) / 2,
                                  gta_car_world_wid(ci) / 2, layer)) {
            veh.x += nx - veh.ox;
            veh.y += ny - veh.oy;
            veh.ox = nx;
            veh.oy = ny;
            veh.ang16 = na;
        }
        nhit = gta_traffic_ram(&fleet, veh.ox, veh.oy, gta_veh_angle(&veh),
                               veh.len / 2, veh.wid / 2, veh.vx, veh.vy,
                               veh.mass, layer, &rvx, &rvy, &ryaw, &rpx, &rpy);
        if (rvx || rvy || ryaw) {
            veh.vx += rvx;
            veh.vy += rvy;
            veh.ang16 = (veh.ang16 + ryaw) & 0xFFFFFFL;
        }
        veh.x += rpx; veh.ox += rpx;
        veh.y += rpy; veh.oy += rpy;
        if (nhit) {
            veh.damage += fleet.pl_damage;
            printf("car: ram x%d - player dv (%ld,%ld) damage %d (+%d this crash)\n",
                   nhit, rvx >> 16, rvy >> 16, veh.damage, fleet.pl_damage);
        }
    }
    tick++;
    printf("c %ld x %ld y %ld v %ld %ld a %ld w %ld s %ld o %ld %ld z %d air %ld d %d\n",
           tick, veh.x, veh.y, veh.vx, veh.vy, veh.ang16, veh.omega, veh.steer16,
           veh.ox, veh.oy, layer, air ? az : 0L, veh.damage);
    {
        long avx = veh.vx < 0 ? -veh.vx : veh.vx;
        long avy = veh.vy < 0 ? -veh.vy : veh.vy;
        int hits;
        fleet_tick(veh.ox, veh.oy, avx > avy ? avx : avy,
                   gta_veh_angle(&veh), layer, veh.len / 2, veh.wid / 2, 0,
                   veh.vx || veh.vy, fleet_weapons_clock_step());
        hits = crowd_ram(veh.ox, veh.oy, gta_veh_angle(&veh), veh.len / 2,
                         veh.wid / 2, layer, veh.vx, veh.vy);
        if (hits)
            printf("car: ran over %d - %ld so far\n", hits, crowd.stat_runover);
        fleet_dump(tick);
    }
}

int main(int argc, char **argv)
{
    FILE *f;
    char line[256];

    if (argc != 6) {
        fprintf(stderr, "usage: drivetrace <map.cmp> <style.gry> <script.txt> <bx> <by>\n");
        return 2;
    }
    if (gta_map_load(argv[1], &map) != 0 || gta_nav_build(&nav, &map) != 0)
        return 1;
    if (gta_style_load(argv[2], &style) != 0)
        return 1;
    tiles.cars = style.cars;
    tiles.n_cars = style.car_count;
    fleet_start(&map, &nav, &tiles, atoi(argv[4]), atoi(argv[5]));
    f = fopen(argv[3], "r");
    if (!f) {
        fprintf(stderr, "drivetrace: cannot open %s\n", argv[3]);
        return 1;
    }
    while (fgets(line, sizeof line, f)) {
        int m, bx, by, face, t, u, d, s, h, i;
        if (line[0] == '#' || line[0] == '\n' || line[0] == '\r')
            continue;
        if (sscanf(line, "drive %d %d %d %d", &m, &bx, &by, &face) == 4) {
            layer = stand_layer(&nav, bx, by);
            if (layer < 0)
                layer = 2;      /* nowhere to stand: the street layer */
            gta_veh_init(&veh, &tiles, m, ((long)bx * 32 + 16) << 16,
                         ((long)by * 32 + 16) << 16, face);
            air = 0;
            driving = 1;
            printf("car: in - model %d at (%d,%d) layer %d\n",
                   style.cars[m].model_id, bx, by, layer);
            /* Like every fixture order, it lasts one tick, keys up. */
            one_tick(0, 0, 0, 0);
        } else if (sscanf(line, "run %d %d %d %d %d", &t, &u, &d, &s, &h) == 5) {
            for (i = 0; i < (t < 1 ? 1 : t) && driving; i++)
                one_tick(u != 0, d != 0, (s > 0) - (s < 0), h != 0);
        } else if (sscanf(line, "wait %d", &t) == 1) {
            for (i = 0; i < (t < 1 ? 1 : t) && driving; i++)
                one_tick(0, 0, 0, 0);
        }
    }
    fclose(f);
    return 0;
}
