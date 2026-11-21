#!/usr/bin/perl
# Apply gfa-sim's following rule to a copy of AmiGTA1's gta_traffic.c, so the
# oracle's queues behave as the game's do (decision log, 2026-11-21).
#
# AmiGTA1's car follows whatever is straight ahead of it, even a car beyond
# the corner its own route turns at. Round Liberty City's one-way loops those
# waits joined into a ring and the whole loop stood still. gfa-sim leaves
# out, where the route turns within four blocks, any car straight on past the
# corner (Traffic::turn_ahead). And halfway round a turn a car's nose points
# across the corner, so the car ahead in the street it is turning into fell
# outside the strip it watches; gfa-sim looks along that street too while it
# turns. This makes the same changes to the C.
#
#     perl traffic_follow.pl < gta_traffic.c > patched/gta_traffic.c
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
    die "traffic_follow.pl: $what matched $n times, not once\n" if $n != 1;
    $c =~ s/$re/$with->()/e;
}

my $fn = <<'C';
/* gfa-sim: blocks on along its heading to where car c's route turns, -1 if
 * it goes straight on for the next four or is turning (traffic_follow.pl). */
static int turn_ahead(const gta_car *c)
{
    int ax, ay, px, py, k = 0, i;

    if (c->turn != 0)
        return -1;
    heading_step(c->angle, &ax, &ay);
    px = (int)(c->x >> (FP + 5));
    py = (int)(c->y >> (FP + 5));
    for (i = c->path_i; i < c->path_n && i < c->path_i + 4; i++) {
        int nx = c->path[i].x, ny = c->path[i].y;
        if (nx == px && ny == py)
            continue;
        if (nx != px + ax || ny != py + ay)
            return k;
        px = nx;
        py = ny;
        k++;
    }
    return -1;
}

C
once('the helper', qr/^(static long gap_ahead\(const gta_traffic \*tr, int idx, long \*lead_speed,\n\s*int \*lead_i\)\n\{\n)/m,
     sub { "$fn$1" });

once('the corner',
     qr/^(static long gap_ahead\(const gta_traffic \*tr, int idx, long \*lead_speed,\n\s*int \*lead_i\)\n\{\n(?:.*\n)*?    long fx = gta_sin\(c->face\), fy = -gta_cos\(c->face\);\n)/m,
     sub { "$1    int kt = turn_ahead(c), cbx = (int)(c->x >> (FP + 5)),\n        cby = (int)(c->y >> (FP + 5)), hax, hay;\n"
             . "    int exit_a = (c->turn_from + (c->turn > 0 ? 64 : -64)) & 255;\n"
             . "    long gx = gta_sin(exit_a), gy = -gta_cos(exit_a);\n"
             . "    heading_step(c->angle, &hax, &hay);\n" });

# Mid-turn, the street it turns into too.
once('the exit',
     qr/^(\s*)if \(along <= 0 \|\| side > GTA_FOLLOW_SIDE\)\n\s*continue;\n/m,
     sub {
         my $i = $1;
         "${i}if (c->turn != 0 && (along <= 0 || side > GTA_FOLLOW_SIDE)) {\n"
       . "$i    along = ((ox >> 8) * gx + (oy >> 8) * gy) >> 6;\n"
       . "$i    side  = ((ox >> 8) * -gy + (oy >> 8) * gx) >> 6;\n"
       . "$i    if (side < 0) side = -side;\n"
       . "$i}\n"
       . "${i}if (along <= 0 || side > GTA_FOLLOW_SIDE)\n"
       . "$i    continue;\n"
     });

once('the filter',
     qr/^(\s*)if \(along > GTA_TRAFFIC_LOOKAHEAD\)\n\s*continue;\n/m,
     sub {
         my $i = $1;
         "${i}if (along > GTA_TRAFFIC_LOOKAHEAD)\n"
       . "$i    continue;\n"
       . "${i}if (kt >= 0 &&\n"
       . "$i    ((int)(o->x >> (FP + 5)) - cbx) * hax +\n"
       . "$i    ((int)(o->y >> (FP + 5)) - cby) * hay > kt)\n"
       . "$i    continue;\n"
     });

print $c;
