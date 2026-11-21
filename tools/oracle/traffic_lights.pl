#!/usr/bin/perl
# Apply gfa-sim's traffic-light rule to a copy of AmiGTA1's gta_traffic.c, so
# the oracle runs the lights the game runs (decision log, 2026-11-21).
#
# AmiGTA1 times a light from the 8x8-block neighbourhood of the block it is
# asked about. A crossing's stop lines lie either side of its junction box,
# so they often fall in different neighbourhoods, and then two approaches
# across each other have the green together. gfa-sim names the light by the
# crossing instead: the root of the first junction block within two blocks
# (Boxes::crossing), so every stop line of a crossing agrees, and a car at
# a stop line asks about the box ahead of it rather than its own block, so
# a second crossing just behind cannot answer. This makes the same change
# to the C.
#
#     perl traffic_lights.pl < gta_traffic.c > patched/gta_traffic.c
#
# Each edit must apply exactly once, or the build stops.
#
# Grand Fair Use Auto, MIT.
use strict;
use warnings;

local $/;
my $c = <STDIN>;

sub once {
    my ($what, $re, $with) = @_;
    my $n = 0;
    $n++ while $c =~ /$re/g;
    die "traffic_lights.pl: $what matched $n times, not once\n" if $n != 1;
    $c =~ s/$re/$with->()/e;
}

# The crossing's key, before the light that reads it.
my $key = <<'C';
/* gfa-sim: a light is its crossing's (traffic_lights.pl). The map the
 * fleet last ticked on, for the crowd's questions about the lights. */
static const gta_map *light_map;

static int light_key(int bx, int by)
{
    int r, dx, dy, z, rx, ry;

    if (light_map)
        for (r = 0; r <= 2; r++)
            for (dy = -r; dy <= r; dy++)
                for (dx = -r; dx <= r; dx++) {
                    if (dx != -r && dx != r && dy != -r && dy != r)
                        continue;
                    for (z = 0; z < GTA_MAP_LAYERS; z++)
                        if (is_junction(light_map, bx + dx, by + dy, z)) {
                            junction_root(light_map, bx + dx, by + dy, z,
                                          &rx, &ry);
                            return (rx >> 3) + (ry >> 3);
                        }
                }
    return (bx >> 3) + (by >> 3);
}

static int light_green_ahead(const gta_traffic *tr, int bx, int by, int angle)
{
    int dx, dy;

    heading_step(angle, &dx, &dy);
    return gta_traffic_light_green(tr, bx + dx, by + dy, (angle & 127) == 64);
}

C
once('the light',
     qr/^(int gta_traffic_light_green\(const gta_traffic \*tr, int bx, int by, int along_x\)\n\{\n    unsigned long cycle = \(unsigned long\)GTA_LIGHT_PHASE \* 2UL;\n    unsigned long off   = \(\(\(unsigned long\)\()\(bx >> 3\) \+ \(by >> 3\)\)/m,
     sub { "$key$1light_key(bx, by))" });

# A car at a stop line asks about the crossing ahead.
once('the stop line',
     qr/!gta_traffic_light_green\(tr, bx, by, \(c->angle & 127\) == 64\)\)/,
     sub { "!light_green_ahead(tr, bx, by, c->angle))" });

# The fleet's tick says which map it is on.
once('the tick',
     qr/^(void gta_traffic_tick\(gta_traffic \*tr, const gta_map \*m, long cam_x, long cam_y\)\n\{\n.*?\n    tr->tick\+\+;\n)/ms,
     sub { "$1    light_map = m;\n" });

print $c;
