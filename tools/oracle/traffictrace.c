/* traffictrace: the AmiGTA1 oracle for traffic.
 *
 * Links AmiGTA1's own traffic, route, nav and map code from a sibling
 * ../AmiGTA1 checkout (build.sh) and runs the fleet as its `gtadump drive`
 * does: cars parked out to 8 blocks around a block, then ticked with the
 * camera held on that block and nobody wanted. Prints the whole fleet
 * after the placement and after every tick, one line a car:
 *
 *     f <tick> <slot> <serial> <x> <y> <layer> <face> <angle> <speed> <model>
 *       <damage> <wrecked>
 *
 * with x, y and speed in raw 16.16, so gfa-sim's port can be diffed with
 * it line for line.
 *
 *     traffictrace <map.cmp> <style.gry> <bx> <by> <ticks> <seed> [view]
 *
 * AmiGTA1's drive_one reads two locals before setting them; build.sh
 * compiles this harness with them zeroed so the run is repeatable.
 *
 * Grand Fair Use Auto, MIT.
 */
#include <stdio.h>
#include <stdlib.h>

#include "gta_map.h"
#include "gta_nav.h"
#include "gta_style.h"
#include "gta_tiles.h"
#include "gta_traffic.h"

/* The drawing side of gta_traffic.c, which this harness never calls. */
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

static gta_map map;
static gta_nav nav;
static gta_style style;
static gta_tiles tiles;
static gta_traffic tr;

static void dump(long tick)
{
    int i;
    for (i = 0; i < tr.n; i++) {
        const gta_car *c = &tr.cars[i];
        printf("f %ld %d %lu %ld %ld %d %d %d %ld %d %d %d\n", tick, i,
               c->serial, c->x, c->y, c->layer, c->face, c->angle, c->speed,
               c->model, c->damage, c->wrecked ? 1 : 0);
    }
}

int main(int argc, char **argv)
{
    long t, ticks, cam_x, cam_y;
    int bx, by, view;
    unsigned long seed;

    if (argc < 7) {
        fprintf(stderr, "usage: traffictrace map.cmp style.gry bx by ticks seed [view]\n");
        return 2;
    }
    if (gta_map_load(argv[1], &map) != 0) {
        fprintf(stderr, "traffictrace: cannot load %s\n", argv[1]);
        return 1;
    }
    if (gta_style_load(argv[2], &style) != 0) {
        fprintf(stderr, "traffictrace: cannot load %s\n", argv[2]);
        return 1;
    }
    tiles.cars = style.cars;
    tiles.n_cars = style.car_count;
    if (gta_nav_build(&nav, &map) != 0) {
        fprintf(stderr, "traffictrace: no memory for the nav grid\n");
        return 1;
    }
    bx = atoi(argv[3]);
    by = atoi(argv[4]);
    ticks = atol(argv[5]);
    seed = strtoul(argv[6], 0, 10);
    view = argc > 7 ? atoi(argv[7]) : 5;

    gta_traffic_init(&tr, &tiles, seed);
    gta_traffic_set_nav(&tr, &nav);
    gta_traffic_set_view_blocks(&tr, view);
    gta_traffic_park(&tr, &map, bx, by, 8, tr.fleet_cap);
    dump(0);

    cam_x = ((long)bx * 32 + 16) << 16;
    cam_y = ((long)by * 32 + 16) << 16;
    for (t = 1; t <= ticks; t++) {
        gta_traffic_tick(&tr, &map, cam_x, cam_y);
        dump(t);
    }
    return 0;
}
