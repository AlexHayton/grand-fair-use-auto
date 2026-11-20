/* traintrace: the AmiGTA1 oracle for the trains.
 *
 * Links AmiGTA1's own gta_train.c (with the map, nav and trig modules) from
 * a sibling ../AmiGTA1 checkout (build.sh), and runs the trains alone for
 * a number of ticks: no traffic, no pedestrians, nobody on foot. The calls
 * gta_train.c makes into those other modules are stood in for below. Prints
 * one line per tick in the format of `gfa-tool run --trace-trains`.
 *
 *     traintrace <map.cmp> <ticks>
 *
 * Grand Fair Use Auto, MIT.
 */
#include <stdio.h>
#include <stdlib.h>

#include "gta_map.h"
#include "gta_train.h"

int gta_tiles_sprite_base(const gta_tiles *t, int type) { (void)t; (void)type; return 0; }
int gta_tiles_sprite_count(const gta_tiles *t, int type) { (void)t; (void)type; return 5; }
int gta_render_sub_at(const gta_view *v, long wx, long wy, int grid)
{ (void)v; (void)wx; (void)wy; (void)grid; return 0; }
int gta_render_add_sprite_air(gta_view *v, long wx, long wy, int layer, int grid,
                              int sub, int index, int angle, int remap, int delta,
                              unsigned long mask)
{ (void)v; (void)wx; (void)wy; (void)layer; (void)grid; (void)sub; (void)index;
  (void)angle; (void)remap; (void)delta; (void)mask; return 0; }
unsigned long gta_traffic_abandon(gta_traffic *tr, int model, long x, long y,
                                  int face, int layer, int remap, int damage)
{ (void)tr; (void)model; (void)x; (void)y; (void)face; (void)layer; (void)remap;
  (void)damage; return 0; }
void gta_peds_kill(gta_peds *ps, int i) { (void)ps; (void)i; }

static gta_map map;
static gta_trains ts;
static gta_tiles tiles;

int main(int argc, char **argv)
{
    long tick, ticks;
    int i;
    if (argc != 3) {
        fprintf(stderr, "usage: traintrace <map.cmp> <ticks>\n");
        return 2;
    }
    if (gta_map_load(argv[1], &map) != 0)
        return 1;
    ticks = atol(argv[2]);
    gta_trains_init(&ts, &map, &tiles);
    for (tick = 1; tick <= ticks; tick++) {
        gta_trains_tick(&ts, NULL, NULL, 0, 0, 0, 0, NULL, NULL);
        printf("r %ld", tick);
        for (i = 0; i < ts.n; i++)
            printf(" | %ld %d %d %d %d %d", ts.t[i].s, ts.t[i].speed, ts.t[i].state,
                   ts.t[i].sub, ts.t[i].door, ts.t[i].lead_node);
        printf("\n");
    }
    return 0;
}
