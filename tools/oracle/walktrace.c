/* walktrace: the AmiGTA1 oracle for the player on foot.
 *
 * Links AmiGTA1's own portable modules (gta_map.c, gta_nav.c,
 * gta_player.c, gta_trig.c), compiled from a sibling ../AmiGTA1 checkout by
 * build.sh, and plays a walk script through them. It prints one line per
 * tick in the format of `gfa-tool run --trace`, so the two can be diffed
 * tick for tick, plus gtadump-style leg lines.
 *
 * Unlike `gtadump walk`, it builds the navigation grid first, so fences
 * (thin walls) are in force as they are in AmiGTA1's game.
 *
 *     walktrace <map.cmp> <script.txt>
 *
 * The script is the walk format: `start bx by`, then `turn forward walk
 * ticks` legs. `#` lines and blank lines are skipped.
 *
 * Grand Fair Use Auto, MIT.
 */
#include <stdio.h>
#include <string.h>

#include "gta_map.h"
#include "gta_nav.h"
#include "gta_player.h"

/* The player asks the tile set where the ped sprites start; a trace has no
 * use for sprites, so these stand in for gta_tiles.c. */
int gta_tiles_sprite_base(const gta_tiles *t, int type)
{
    (void)t;
    (void)type;
    return 0;
}

int gta_tiles_sprite_count(const gta_tiles *t, int type)
{
    (void)t;
    (void)type;
    return 295;
}

static const char *const ground_name[8] = {
    "air", "water", "road", "pavement", "field", "building", "?6", "?7"
};

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
    static gta_tiles tiles;
    gta_player pl;
    FILE *script;
    char line[256];
    int sx = 64, sy = 64, leg = 0;
    long tick = 0;

    if (argc != 3) {
        fprintf(stderr, "usage: walktrace <map.cmp> <script.txt>\n");
        return 2;
    }
    if (gta_map_load(argv[1], &map) != 0)
        return 1;
    if (gta_nav_build(&nav, &map) != 0) {
        fprintf(stderr, "walktrace: no memory for the nav grid\n");
        return 1;
    }
    script = fopen(argv[2], "r");
    if (!script) {
        fprintf(stderr, "walktrace: cannot open %s\n", argv[2]);
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
    gta_player_init(&pl, &map, &tiles, sx, sy);
    printf("player: start (%d,%d) layer %d angle %d ground %s\n", sx, sy,
           pl.layer, pl.angle, ground_name[pl.ground & 7]);
    trace(tick, &pl);

    while (fgets(line, sizeof line, script)) {
        int turn, fwd, walk, ticks, i;
        if (line[0] == '#' || line[0] == '\n' || line[0] == '\r')
            continue;
        if (sscanf(line, "%d %d %d %d", &turn, &fwd, &walk, &ticks) != 4)
            continue;
        for (i = 0; i < ticks; i++) {
            gta_player_update(&pl, &map, turn, fwd, walk);
            trace(++tick, &pl);
        }
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
