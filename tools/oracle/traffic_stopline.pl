#!/usr/bin/perl
# Apply gfa-sim's stop-line rule to a copy of AmiGTA1's gta_traffic.c, so the
# oracle stops for a red light where the game does (decision log,
# 2026-11-21).
#
# AmiGTA1 holds a car at a red light when its middle is on the stop line. A
# bus is then half a block into the crossing, and the traffic across clips
# its nose, shoving it sideways a pixel at a time onto the pavement. gfa-sim
# also holds a car once its nose would reach the box, as the box's own gate
# does (within its stopping reach of the box), where the block before the
# box is a stop line. This makes the same change to the C. Needs
# traffic_lights.pl's light_green_ahead.
#
#     perl traffic_stopline.pl < gta_traffic.c > patched/gta_traffic.c
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
    die "traffic_stopline.pl: $what matched $n times, not once\n" if $n != 1;
    $c =~ s/$re/$with->()/e;
}

my $fn = <<'C';
/* gfa-sim: is the light red for car c, at the stop line or with its nose
 * about to reach the box (traffic_stopline.pl)? */
static int light_red_ahead(const gta_traffic *tr, const gta_map *m,
                           const gta_car *c, int bx, int by, int jbx,
                           int jby, long ej, long stopr, int dxs, int dys)
{
    gta_block b;

    if (at_stop_line(m, c->hint_val, bx, by, c->layer, c->angle))
        return !light_green_ahead(tr, bx, by, c->angle);
    if (jbx < 0 || ej >= stopr)
        return 0;
    if (!gta_map_block(m, jbx - dxs, jby - dys, c->layer, &b) ||
        gta_block_traffic_hint(&b) != 1)
        return 0;
    return !gta_traffic_light_green(tr, jbx, jby, (c->angle & 127) == 64);
}

C
once('the helper', qr/^(static void drive_one\(gta_traffic \*tr, const gta_map \*m, int idx\)\n\{)/m,
     sub { "$fn$1" });

once('the light',
     qr/^(\s*)at_stop_line\(m, c->hint_val, bx, by, c->layer, c->angle\) &&\n\s*!light_green_ahead\(tr, bx, by, c->angle\)\) \{\n/m,
     sub {
         my $i = $1;
         "${i}light_red_ahead(tr, m, c, bx, by, jbx, jby, ej, stopr, dxs, dys)) {\n"
     });

print $c;
