/* fleet.h: the rest of the street, for the walk and drive harnesses.
 *
 * AmiGTA1's own traffic (gta_traffic.c) and trains (gta_train.c), run
 * around the player the way gfa-sim's World runs them, which is the order
 * of AmiGTA1's main loop: after the player has moved, the fleet is told
 * where he is and ticks with the view on him, then the trains tick, then
 * write-offs in the fleet burn (gta_weapon.c's sweep, without the blast,
 * which neither side has yet). The glue AmiGTA1 keeps in gta_main.c (a
 * walker held off a car, and the fleet as seen by the player's car) is
 * restated here.
 *
 * The fleet starts as World::new starts it: seed 12345, AmiGTA1's view of
 * 5 blocks, parked out to 8 blocks around the level start, and none of the
 * map's parked cars (AmiGTA1 has none). After every tick the whole fleet
 * is printed in traffictrace's format:
 *
 *     f <tick> <slot> <serial> <x> <y> <layer> <face> <angle> <speed> <model>
 *
 * Grand Fair Use Auto, MIT.
 */
#ifndef GFA_ORACLE_FLEET_H
#define GFA_ORACLE_FLEET_H

#include <stdio.h>

#include "gta_car.h"
#include "gta_map.h"
#include "gta_nav.h"
#include "gta_tiles.h"
#include "gta_traffic.h"
#include "gta_train.h"
#include "gta_trig.h"

#define FLEET_SEED      12345UL
#define FLEET_VIEW      5
#define FLEET_PED_MARGIN 3
#define FLEET_WRECKED   100
#define FLEET_FUSE      120

/* The drawing and pedestrian sides of the modules linked, never called
 * for anything a trace prints. */
int gta_tiles_sprite_base(const gta_tiles *t, int type) { (void)t; (void)type; return 0; }
int gta_tiles_sprite_count(const gta_tiles *t, int type) { (void)t; (void)type; return 295; }
int gta_tiles_wreck_sprite(const gta_tiles *t, int vtype) { (void)t; (void)vtype; return 0; }
int gta_render_add_sprite(gta_view *v, long wx, long wy, int layer, int grid,
                          int index, int angle)
{ (void)v; (void)wx; (void)wy; (void)layer; (void)grid; (void)index; (void)angle; return 0; }
int gta_render_add_sprite_dm(gta_view *v, long wx, long wy, int layer, int grid,
                             int index, int angle, int remap, int delta,
                             unsigned long delta_mask)
{ (void)v; (void)wx; (void)wy; (void)layer; (void)grid; (void)index; (void)angle;
  (void)remap; (void)delta; (void)delta_mask; return 0; }
int gta_render_sub_at(const gta_view *v, long wx, long wy, int grid)
{ (void)v; (void)wx; (void)wy; (void)grid; return 0; }
int gta_render_add_sprite_air(gta_view *v, long wx, long wy, int layer, int grid,
                              int sub, int index, int angle, int remap, int delta,
                              unsigned long mask)
{ (void)v; (void)wx; (void)wy; (void)layer; (void)grid; (void)sub; (void)index;
  (void)angle; (void)remap; (void)delta; (void)mask; return 0; }
void gta_peds_kill(gta_peds *ps, int i) { (void)ps; (void)i; }

static gta_traffic fleet;
static gta_trains fleet_trains;
static const gta_map *fleet_map;

static void fleet_start(const gta_map *m, const gta_nav *nav,
                        const gta_tiles *t, int bx, int by)
{
    fleet_map = m;
    gta_trains_init(&fleet_trains, m, t);
    gta_traffic_init(&fleet, t, FLEET_SEED);
    gta_traffic_set_nav(&fleet, nav);
    gta_traffic_set_view_blocks(&fleet, FLEET_VIEW);
    gta_traffic_park(&fleet, m, bx, by, 8, fleet.fleet_cap);
}

/* Is the point inside a car's body, grown by `margin` px? */
static int fleet_body_hit(long cx, long cy, int face, int hl, int hw,
                          long px, long py, int margin)
{
    long fx = gta_sin(face), fy = -gta_cos(face);
    long rx = gta_cos(face), ry = gta_sin(face);
    long dx = (px - cx) >> 16, dy = (py - cy) >> 16;
    long along = (dx * fx + dy * fy) >> 14;
    long across = (dx * rx + dy * ry) >> 14;
    if (along < 0) along = -along;
    if (across < 0) across = -across;
    return along <= hl + margin && across <= hw + margin;
}

static int fleet_car_at(long px, long py, int layer, int margin)
{
    int i;
    for (i = 0; i < fleet.n; i++) {
        const gta_car *c = &fleet.cars[i];
        const gta_car_info *ci;
        if (c->done || c->layer != layer)
            continue;
        ci = &fleet.tiles->cars[c->model];
        if (fleet_body_hit(c->x, c->y, c->face, gta_car_world_len(ci) / 2,
                           gta_car_world_wid(ci) / 2, px, py, margin))
            return i;
    }
    return -1;
}

/* A walker who stepped into a car goes back along whichever axis put him
 * there. (ox, oy) is where he was before the step. */
static void fleet_hold_walker(long *x, long *y, int layer, long ox, long oy)
{
    if (fleet_car_at(*x, *y, layer, FLEET_PED_MARGIN) < 0)
        return;
    if (fleet_car_at(ox, oy, layer, FLEET_PED_MARGIN) >= 0)
        return;
    if (fleet_car_at(*x, oy, layer, FLEET_PED_MARGIN) < 0)
        *y = oy;
    else if (fleet_car_at(ox, *y, layer, FLEET_PED_MARGIN) < 0)
        *x = ox;
    else {
        *x = ox;
        *y = oy;
    }
}

/* Write-offs catch fire and burn out (gta_weapon.c), without the blast. */
static void fleet_wreck_sweep(void)
{
    int i;
    for (i = 0; i < fleet.n; i++) {
        gta_car *c = &fleet.cars[i];
        if (c->done || c->wrecked || c->damage < FLEET_WRECKED)
            continue;
        if (c->fuse == 0) {
            c->fuse = FLEET_FUSE;
            c->dmg_bits |= GTA_DELTA_DMG_MASK;
            continue;
        }
        if (--c->fuse > 0)
            continue;
        c->wrecked = 1;
        c->abandoned = 1;
        c->speed = 0;
        c->knock = 0;
        c->fuse = 0;
        c->hold = 0;
    }
}

/* One tick of the street, with the player at (x, y): his speed, heading,
 * layer and half size as the fleet sees him, and whether he is on foot
 * (for the trains). Returns 1 if a train ran him over. */
static int fleet_tick(long x, long y, long speed, int face, int layer,
                      int hl, int hw, int on_foot)
{
    int hit;
    gta_traffic_set_player(&fleet, 1, x, y, speed, face, layer, hl, hw);
    gta_traffic_clear_walkers(&fleet);
    gta_traffic_set_view_blocks(&fleet, FLEET_VIEW);
    gta_traffic_tick(&fleet, fleet_map, x, y);
    gta_trains_tick(&fleet_trains, &fleet, NULL, x, y, layer, on_foot, NULL, 0);
    hit = fleet_trains.pl_hit;
    fleet_trains.pl_hit = 0;
    fleet_wreck_sweep();
    return hit;
}

/* The whole fleet, after the player's line for the same tick. */
static void fleet_dump(long tick)
{
    int i;
    for (i = 0; i < fleet.n; i++) {
        const gta_car *c = &fleet.cars[i];
        printf("f %ld %d %lu %ld %ld %d %d %d %ld %d\n", tick, i, c->serial,
               c->x, c->y, c->layer, c->face, c->angle, c->speed, c->model);
    }
}

#endif
