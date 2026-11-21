#!/usr/bin/perl
# Apply gfa-sim's rule for long vehicles at corners to a copy of AmiGTA1's
# gta_traffic.c, so the oracle's buses turn as the game's do (decision log,
# 2026-11-21).
#
# AmiGTA1 asks whether a vehicle fits lying along the way it is leaving, in
# the block it is in and in the next. At a corner whose far side is a
# building or grass, a bus is refused the move into it; and one that has
# turned in the corner, its tail still over the verge behind, is refused the
# way out. Either stands for good and blocks its street. gfa-sim asks only
# whether the block a car moves into takes it as it comes in
# (roads::fits_entering): lying along its way where it can go straight on,
# and at a corner, along the street it turns into. And a turn a car must
# make, at a corner with no way on, starts though its nose is not yet
# straight: nothing straightens it inside a junction box. This makes the same
# changes to the C.
#
#     perl traffic_fits.pl < gta_traffic.c > patched/gta_traffic.c
#
# Each edit must apply exactly once, or the build stops. Needs
# traffic_corner.pl's corner_way_on.
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
    die "traffic_fits.pl: $what matched $n times, not once\n" if $n != 1;
    $c =~ s/$re/$with->()/e;
}

my $fn = <<'C';
/* gfa-sim: does a vehicle fit coming into a block (traffic_fits.pl)? */
static int fits_entering(const gta_map *m, int bx, int by, int z, int dir,
                         int length)
{
    static const int ways[4] = { 0, 64, 128, 192 };
    int axis = ((dir & 127) == 64) ? 64 : 0, back = (dir + 128) & 255;
    int i, any = 0;

    if (dir_allowed(m, bx, by, z, dir))
        return fits(m, bx, by, z, axis, length);
    for (i = 0; i < 4; i++)
        if (ways[i] != dir && ways[i] != back &&
            dir_allowed(m, bx, by, z, ways[i]))
            any = 1;
    if (!any || ground_at(m, bx, by, z) != GROUND_ROAD)
        return fits(m, bx, by, z, axis, length);
    for (i = 0; i < 4; i++) {
        int d = ways[i], dx, dy, nz;
        if (d == dir || d == back || !dir_allowed(m, bx, by, z, d))
            continue;
        nz = nav_step_layer(m, bx, by, z, d);
        if (nz < 0)
            continue;
        heading_step(d, &dx, &dy);
        if (fits(m, bx + dx, by + dy, nz, ((d & 127) == 64) ? 64 : 0, length))
            return 1;
    }
    return 0;
}

C
once('the helper', qr/^(static int has_exit\(const gta_map \*m, int bx, int by, int z, int in, int length\)\n\{)/m,
     sub { "$fn$1" });

once('has_exit',
     qr/^(\s*)axis = \(\(dirs\[i\] & 127\) == 64\) \? 64 : 0;\n\s*if \(!fits\(m, bx, by, z, axis, length\)\) continue;\n\s*if \(!fits\(m, bx \+ dx, by \+ dy, nz, axis, length\)\) continue;\n/m,
     sub { "$1if (!fits_entering(m, bx + dx, by + dy, nz, dirs[i], length)) continue;\n" });

once('choose_heading',
     qr/^(\s*)axis = \(\(all\[i\] & 127\) == 64\) \? 64 : 0;\n\s*if \(!fits\(m, bx, by, z, axis, length\)\) continue;\n\s*if \(!fits\(m, bx \+ dx, by \+ dy, nz, axis, length\)\) continue;\n/m,
     sub { "$1if (!fits_entering(m, bx + dx, by + dy, nz, all[i], length)) continue;\n" });

once('the route',
     qr/^(\s*)axis = \(\(want_dir & 127\) == 64\) \? 64 : 0;\n\s*if \(!fits\(m, bx, by, c->layer, axis, gta_car_world_len\(info\)\) \|\|\n\s*!fits\(m, bx \+ tdx, by \+ tdy, c->layer, axis, gta_car_world_len\(info\)\)\) \{\n/m,
     sub { "$1if (!fits_entering(m, bx + tdx, by + tdy, c->layer, want_dir,\n$1                   gta_car_world_len(info))) {\n" });

once('the turn',
     qr/^(\s*)if \(dsq > \(GTA_STEER_SLOW << FP\) \|\|\n\s*dsq < -\(GTA_STEER_SLOW << FP\)\)\n\s*ready = 0;\n/m,
     sub {
         my $i = $1;
         "${i}if ((dsq > (GTA_STEER_SLOW << FP) ||\n"
       . "$i     dsq < -(GTA_STEER_SLOW << FP)) &&\n"
       . "$i    corner_way_on(m, c, bx, by, ahead, dxs, dys))\n"
       . "$i    ready = 0;\n"
     });

print $c;
