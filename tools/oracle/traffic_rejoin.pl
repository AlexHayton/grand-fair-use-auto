#!/usr/bin/perl
# Apply gfa-sim's lane rule to a copy of AmiGTA1's gta_traffic.c, so the
# oracle drives a shoved car as the game does (decision log, 2026-11-21).
#
# AmiGTA1 drives a car by the block under its middle. A car pushed a pixel
# over a lane line in a collision, into the lane beside going the other way
# or onto the kerb, then has nowhere legal to go: it stands at a "dead end"
# until it is out of sight. gfa-sim drives such a car by the block beside it
# on the nearer side, when that one goes its way (Traffic::lane_block), so
# its route, junctions and lane line are its own lane's and the lane keeper
# steers it back; failing the nearer side, the other, for a car pushed past
# the middle of the block beside its lane. This makes the same change to the
# C.
#
#     perl traffic_rejoin.pl < gta_traffic.c > patched/gta_traffic.c
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
    die "traffic_rejoin.pl: $what matched $n times, not once\n" if $n != 1;
    $c =~ s/$re/$with->()/e;
}

my $fn = <<'C';
/* gfa-sim: the block a car drives by (traffic_rejoin.pl). */
static void lane_block(const gta_map *m, const gta_car *c, int *pbx, int *pby)
{
    int bx = *pbx, by = *pby, z = c->layer, angle = c->angle;
    int along_x, s, nx, ny, k;
    long across;

    if (c->turn != 0 || c->swap != 0 || c->reverse != 0 || c->cop != 0 ||
        c->crossing)
        return;
    if (is_junction(m, bx, by, z) ||
        (ground_at(m, bx, by, z) == GROUND_ROAD &&
         dir_allowed(m, bx, by, z, angle)))
        return;
    along_x = (angle & 127) == 64;
    across = along_x ? c->y : c->x;
    s = ((across >> FP) & 31) >= 16 ? 1 : -1;
    for (k = 0; k < 2; k++, s = -s) {
        nx = along_x ? bx : bx + s;
        ny = along_x ? by + s : by;
        if (ground_at(m, nx, ny, z) == GROUND_ROAD &&
            dir_allowed(m, nx, ny, z, angle) && !is_junction(m, nx, ny, z)) {
            *pbx = nx;
            *pby = ny;
            return;
        }
    }
}

C
once('the helper', qr/^(static void drive_one\(gta_traffic \*tr, const gta_map \*m, int idx\)\n\{)/m,
     sub { "$fn$1" });

once('the block', qr/^(\s*)bx = \(int\)\(c->x >> \(FP \+ 5\)\);\n\s*by = \(int\)\(c->y >> \(FP \+ 5\)\);\n(\n\s*\/\* THE CHASE\.)/m,
     sub { "$1bx = (int)(c->x >> (FP + 5));\n$1by = (int)(c->y >> (FP + 5));\n$1lane_block(m, c, &bx, &by);\n$2" });

print $c;
