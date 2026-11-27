#!/usr/bin/perl
# Apply the original's chain reaction and wreck pay to a copy of AmiGTA1's
# gta_weapon.c (after weapons_clock.pl), so the oracle runs what M6b measured
# from GTA8.EXE (docs/M6B_TANK_TEST_PLAN.md, tank_chain_view and tank_chain).
#
# In AmiGTA1 every write-off burns GTA_CAR_FUSE (120) ticks and every wreck
# pays 100, and a rocket hit pays 100 more. In GTA8.EXE:
#
#   - a car written off where it stands (hit, shot, crashed) blows up on the
#     next game frame: the sweep lights a fuse of 1 and burns it at once;
#   - a car caught in a blast is given a fuse of 10 game frames, about 430 ms;
#   - fuses burn only on the 23.333 Hz game frame (gfa_weapons_step);
#   - every fuse burns first, then the cars whose fuse ran out blow up, so a
#     chain steps ten frames a car whatever the cars' order in the fleet;
#   - a wreck pays 100, doubling along the score's streak (gta_score_wreck,
#     score_streak.pl), and only within 8 blocks (256 px) of the player on
#     both axes; the rocket hit pays nothing of its own.
#
# gfa-sim pays only for a wreck the player caused; AmiGTA1's fleet keeps no
# such mark, so here every wreck near him pays.
#
#     perl weapons_chain.pl < gta_weapon.c > patched/gta_weapon.c
#
# Each edit must apply exactly once, or the build stops.
#
# Grand Fair Use Auto, MIT.
use strict;
use warnings;

local $/;
my $c = <STDIN>;

sub edit {
    my ($name, $re, $with) = @_;
    my $n = 0;
    $n++ while $c =~ /$re/g;
    die "weapons_chain.pl: '$name' matched $n times, not once\n" if $n != 1;
    $c =~ s/$re/$with->()/e;
}

edit('blast fuse',
     qr/^([ \t]*)c->damage = GTA_CAR_WRECKED;\n[ \t]*c->dmg_bits \|= GTA_DELTA_DMG_MASK;\n/m,
     sub {
         "$1c->damage = GTA_CAR_WRECKED;\n"
       . "$1c->dmg_bits |= GTA_DELTA_DMG_MASK;\n"
       . "$1c->fuse = 10;      /* gfa-sim: ten game frames */\n"
       . "$1printf(\"gta: car %d (model %d) is a write-off - burning\\n\",\n"
       . "$1       i, c->model);\n";
     });

edit('two-pass sweep',
     qr/^static void wreck_sweep\(gta_weapons \*w, gta_peds \*peds, gta_traffic \*tr,\n[ \t]*const gta_tiles \*t, gta_score \*sc\)\n\{\n.*?\n\}\n/ms,
     sub {
         "long gta_score_wreck(gta_score *s);\n\n"
       . "/* gfa-sim: is (x,y) near enough the player for a wreck there to pay? */\n"
       . "static int wreck_pays(const gta_weapons *w, long x, long y)\n"
       . "{\n"
       . "    long dx, dy;\n"
       . "    if (!w->pl_active) return 1;\n"
       . "    dx = (w->pl_x - x) >> 16; if (dx < 0) dx = -dx;\n"
       . "    dy = (w->pl_y - y) >> 16; if (dy < 0) dy = -dy;\n"
       . "    return dx < 256 && dy < 256;\n"
       . "}\n\n"
       . "/* gfa-sim: every fuse burns on the game frame, then the cars whose fuse\n"
       . " * ran out blow up. */\n"
       . "static void wreck_sweep(gta_weapons *w, gta_peds *peds, gta_traffic *tr,\n"
       . "                        const gta_tiles *t, gta_score *sc, int frame)\n"
       . "{\n"
       . "    int i, k, n = 0;\n"
       . "    int due[GTA_MAX_CARS];\n"
       . "    for (i = 0; i < tr->n; i++) {\n"
       . "        gta_car *c = &tr->cars[i];\n"
       . "        if (c->done || c->wrecked || c->damage < GTA_CAR_WRECKED)\n"
       . "            continue;\n"
       . "        if (c->fuse == 0) {\n"
       . "            c->fuse = 1;\n"
       . "            c->dmg_bits |= GTA_DELTA_DMG_MASK;\n"
       . "            printf(\"gta: car %d (model %d) is a write-off - burning\\n\",\n"
       . "                   i, c->model);\n"
       . "        }\n"
       . "        if (!frame || --c->fuse > 0)\n"
       . "            continue;\n"
       . "        printf(\"gta: car %d (model %d) blew up at (%ld,%ld)\\n\", i, c->model,\n"
       . "               c->x >> 16, c->y >> 16);\n"
       . "        c->wrecked   = 1;\n"
       . "        c->abandoned = 1;\n"
       . "        c->speed     = 0;\n"
       . "        c->knock     = 0;\n"
       . "        c->fuse      = 0;\n"
       . "        c->hold      = 0;\n"
       . "        due[n++] = i;\n"
       . "    }\n"
       . "    for (k = 0; k < n; k++) {\n"
       . "        gta_car *c = &tr->cars[due[k]];\n"
       . "        gta_weapons_wreck_car(w, &t->cars[c->model], c->x, c->y, c->face,\n"
       . "                              c->layer, peds, tr, sc, 0);\n"
       . "        if (sc && wreck_pays(w, c->x, c->y))\n"
       . "            printf(\"gta: score: wreck +%ld\\n\", gta_score_wreck(sc));\n"
       . "    }\n"
       . "}\n";
     });

edit('sweep on the frame',
     qr/^([ \t]*)wreck_sweep\(w, peds, tr, t, sc\);\n/m,
     sub { "$1wreck_sweep(w, peds, tr, t, sc, gfa_weapons_step);\n" });

edit('rocket hit pays nothing',
     qr/^[ \t]*gta_score_add\(sc, 100\);\n([ \t]*gta_score_crime\(sc, GTA_CRIME_FIREARM\);\n[ \t]*if \(tr->cars\[ci\]\.model == GTA_COP_MODEL\)\n[ \t]*gta_score_force_level\(sc, 1\);\n[ \t]*\}\n[ \t]*printf\("gta: rocket burst on car)/m,
     sub { "$1" });

print $c;
