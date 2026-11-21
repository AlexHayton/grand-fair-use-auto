/* fleet.h: the rest of the street, for the walk and drive harnesses.
 *
 * AmiGTA1's own traffic (gta_traffic.c), trains (gta_train.c) and people
 * on foot (gta_peds.c), run around the player the way gfa-sim's World runs
 * them, which is the order of AmiGTA1's main loop: after the player has
 * moved, the crowd and the fleet are told where he is, the people standing
 * on their feet go to the fleet as walkers, the fleet ticks with the view
 * on him, then the trains, then the score's streak clock, then the crowd,
 * then the weapons (gta_weapon.c: fires, explosions, write-offs in the
 * fleet burning and blowing up, and every shot in flight). The glue
 * AmiGTA1 keeps in gta_main.c (a walker held off a car, the lights as the
 * crowd asks about them, and the fleet as seen by the player's car) is
 * restated here.
 *
 * The fleet starts as World::new starts it: seed 12345, AmiGTA1's view of
 * 5 blocks, parked out to 8 blocks around the level start, and none of the
 * map's parked cars (AmiGTA1 has none). The crowd starts empty with seed
 * 777 and fills from the view's edge. After every tick the whole fleet and
 * every living ped are printed:
 *
 *     f <tick> <slot> <serial> <x> <y> <layer> <face> <angle> <speed> <model>
 *       <damage> <wrecked>
 *     p <tick> <slot> <x> <y> <layer> <angle> <mode> <sub> <speed> <frame>
 *       <corpse> <down> <pull> <shot> <fall> <burn>
 *     w <tick> <slot> <x> <y> <layer> <heading> <kind> <speed> <frame>
 *     x <tick> <slot> <x> <y> <frame>          (explosions)
 *     h <tick> <slot> <x> <y> <ticks>          (fires on the ground)
 *
 * Grand Fair Use Auto, MIT.
 */
#ifndef GFA_ORACLE_FLEET_H
#define GFA_ORACLE_FLEET_H

#include <stdio.h>

#include "gta_car.h"
#include "gta_map.h"
#include "gta_nav.h"
#include "gta_peds.h"
#include "gta_tiles.h"
#include "gta_traffic.h"
#include "gta_train.h"
#include "gta_trig.h"
#include "gta_score.h"
#include "gta_weapon.h"

#define FLEET_SEED      12345UL
#define FLEET_VIEW      5
#define FLEET_PED_MARGIN 3
#define CROWD_SEED      777UL
#define CROWD_VIEW_H    4

/* The drawing side of the modules linked, never called for anything a
 * trace prints. */
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
int gta_render_add_sprite_r(gta_view *v, long wx, long wy, int layer, int grid,
                            int index, int angle, int remap)
{ (void)v; (void)wx; (void)wy; (void)layer; (void)grid; (void)index; (void)angle;
  (void)remap; return 0; }
/* No crates on either side yet. */
int gta_pickups_open_at(gta_pickups *pk, long x, long y, int layer, int radius)
{ (void)pk; (void)x; (void)y; (void)layer; (void)radius; return 0; }

static gta_traffic fleet;
static gta_trains fleet_trains;
static gta_peds crowd;
static const gta_map *fleet_map;
static const gta_nav *fleet_nav;
static gta_weapons weapons;
static gta_score score;

/* The crowd asks the fleet about the lights (gta_main.c). */
static int crowd_light_green(void *ctx, int bx, int by, int along_x)
{
    return gta_traffic_light_green((const gta_traffic *)ctx, bx, by, along_x);
}

static void fleet_start(const gta_map *m, const gta_nav *nav,
                        const gta_tiles *t, int bx, int by)
{
    fleet_map = m;
    fleet_nav = nav;
    gta_weapons_init(&weapons, t);
    gta_score_init(&score);
    gta_score_set_city(m->style_number);
    gta_trains_init(&fleet_trains, m, t);
    gta_traffic_init(&fleet, t, FLEET_SEED);
    gta_traffic_set_nav(&fleet, nav);
    gta_traffic_set_view_blocks(&fleet, FLEET_VIEW);
    gta_traffic_park(&fleet, m, bx, by, 8, fleet.fleet_cap);
    gta_peds_init(&crowd, t, CROWD_SEED);
    gta_peds_set_lights(&crowd, crowd_light_green, &fleet);
    gta_peds_set_nav(&crowd, nav);
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

/* One tick of the street, with the player at (x, y): his speed, heading,
 * layer and half size as the fleet sees him, whether he is on foot (for
 * the trains and the crowd) and whether he is moving (for where the crowd
 * spawns). Returns 1 if a train ran him over. */
static int fleet_tick(long x, long y, long speed, int face, int layer,
                      int hl, int hw, int on_foot, int moving)
{
    int hit, i;
    gta_traffic_set_player(&fleet, 1, x, y, speed, face, layer, hl, hw);
    gta_peds_set_player(&crowd, x, y, layer, !on_foot, on_foot ? 0 : hl,
                        on_foot ? 0 : hw, on_foot ? 0 : face);
    /* Where a cop would take him and whether the car is slow enough: no
     * cops yet on either side (M6), so the door is where he is. */
    crowd.pl_door_x = x;
    crowd.pl_door_y = y;
    crowd.pl_slow = 0;
    gta_traffic_clear_walkers(&fleet);
    for (i = 0; i < GTA_MAX_PEDS; i++) {
        const gta_ped *p = &crowd.p[i];
        if (p->alive && p->down <= 0)
            gta_traffic_add_walker(&fleet, p->x, p->y, p->layer);
    }
    gta_traffic_set_view_blocks(&fleet, FLEET_VIEW);
    gta_traffic_tick(&fleet, fleet_map, x, y);
    gta_trains_tick(&fleet_trains, &fleet, &crowd, x, y, layer, on_foot, NULL, 0);
    hit = fleet_trains.pl_hit;
    fleet_trains.pl_hit = 0;
    gta_score_tick(&score);
    gta_peds_set_view(&crowd, FLEET_VIEW, CROWD_VIEW_H, face, moving);
    gta_peds_tick(&crowd, fleet_map, x, y);
    gta_weapons_tick(&weapons, fleet_nav, &crowd, &fleet, fleet.tiles, &score);
    return hit;
}

/* The player's car through the crowd, after the street has ticked; the
 * speed is gta_main.c's (the larger axis and half the smaller). Returns how
 * many it killed. */
static int crowd_ram(long x, long y, int face, int hl, int hw, int layer,
                     long vx, long vy)
{
    long avx = vx < 0 ? -vx : vx;
    long avy = vy < 0 ? -vy : vy;
    return gta_peds_ram(&crowd, x, y, face, hl, hw, layer,
                        avx > avy ? avx + avy / 2 : avy + avx / 2);
}

/* The whole fleet, after the player's line for the same tick. */
static void fleet_dump(long tick)
{
    int i;
    for (i = 0; i < fleet.n; i++) {
        const gta_car *c = &fleet.cars[i];
        printf("f %ld %d %lu %ld %ld %d %d %d %ld %d %d %d\n", tick, i,
               c->serial, c->x, c->y, c->layer, c->face, c->angle, c->speed,
               c->model, c->damage, c->wrecked ? 1 : 0);
    }
    for (i = 0; i < GTA_MAX_PEDS; i++) {
        const gta_ped *p = &crowd.p[i];
        if (!p->alive)
            continue;
        printf("p %ld %d %ld %ld %d %d %d %d %d %d %d %d %d %d %d %d\n", tick,
               i, p->x, p->y, p->layer, p->angle, p->mode, p->sub, p->speed,
               p->frame, p->corpse ? 1 : 0, p->down, p->pull, p->shot,
               p->fall, p->burn);
    }
    for (i = 0; i < GTA_MAX_BULLETS; i++) {
        const gta_bullet *b = &weapons.b[i];
        if (b->alive)
            printf("w %ld %d %ld %ld %d %d %d %d %d\n", tick, i, b->x, b->y,
                   b->layer, b->heading, b->kind, b->speed, b->frame);
    }
    for (i = 0; i < GTA_MAX_EXPLOSIONS; i++) {
        const gta_explosion *e = &weapons.x[i];
        if (e->frame)
            printf("x %ld %d %ld %ld %d\n", tick, i, e->x, e->y, e->frame);
    }
    for (i = 0; i < GTA_MAX_FIRES; i++) {
        const gta_fire *f = &weapons.f[i];
        if (f->ticks)
            printf("h %ld %d %ld %ld %d\n", tick, i, f->x, f->y, f->ticks);
    }
}

#endif
