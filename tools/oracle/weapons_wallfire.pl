#!/usr/bin/perl
# Apply the original's wall fire to a copy of AmiGTA1's gta_weapon.c, so the
# oracle runs the fire measured from GTA8.EXE in M6b.
#
# A rocket that bursts on a building wall leaves three fires in AmiGTA1, a
# step back along its line and one either side, burning GTA_FIRE_TICKS (250).
# In GTA8.EXE it leaves one, the middle one, burning about 22 s (1100 ticks):
# the same for a rocket and the tank's shell (docs/M6B_TANK_TEST_PLAN.md, B3
# and B5). The script explosion's fires keep GTA_FIRE_TICKS.
#
#     perl weapons_wallfire.pl < gta_weapon.c > patched/gta_weapon.c
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
    die "weapons_wallfire.pl: '$name' matched $n times, not once\n" if $n != 1;
    $c =~ s/$re/$with->()/e;
}

my $anchor = "/* A fire on the ground at (x,y): the next free slot, or the oldest. */\n";
edit('fire with its own time',
     qr/\Q$anchor\E/,
     sub {
         "/* gfa-sim: a fire burning `ticks` (fire_light's, with its time given). */\n"
       . "static void fire_light_ticks(gta_weapons *w, long x, long y, int layer, int ticks)\n"
       . "{\n"
       . "    int i, pick = 0;\n"
       . "    for (i = 0; i < GTA_MAX_FIRES; i++) {\n"
       . "        if (w->f[i].ticks == 0) { pick = i; break; }\n"
       . "        if (w->f[i].ticks < w->f[pick].ticks) pick = i;\n"
       . "    }\n"
       . "    w->f[pick].x = x;\n"
       . "    w->f[pick].y = y;\n"
       . "    w->f[pick].layer = layer;\n"
       . "    w->f[pick].ticks = ticks;\n"
       . "    w->f[pick].frame = 0;\n"
       . "    w->f[pick].frame_tick = 0;\n"
       . "    w->stat_fires++;\n"
       . "}\n\n"
       . $anchor;
     });
edit('one wall fire',
     qr/fire_light\(w, fx, fy, b->layer\);\n\s*fire_light\(w, fx \+ sx, fy \+ sy, b->layer\);\n\s*fire_light\(w, fx - sx, fy - sy, b->layer\);/,
     sub { "fire_light_ticks(w, fx, fy, b->layer, 1100);" });

print $c;
