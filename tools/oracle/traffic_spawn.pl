#!/usr/bin/perl
# Apply gfa-sim's spawn rule to a copy of AmiGTA1's gta_traffic.c, so the
# oracle puts new cars where the game does (decision log, 2026-11-21).
#
# AmiGTA1 puts a new car on a block when its lane has room, without asking
# whether another car's body reaches into the block: the nose of one waiting
# at a corner, or one changing into that lane. The new car appeared inside
# it. gfa-sim also asks that no car's body touch the box the new car would
# take, at the block's middle and wide enough for the lane beside a kerb
# (Traffic::body_in_way). This makes the same change to the C.
#
#     perl traffic_spawn.pl < gta_traffic.c > patched/gta_traffic.c
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
    die "traffic_spawn.pl: $what matched $n times, not once\n" if $n != 1;
    $c =~ s/$re/$with->()/e;
}

my $fn = <<'C';
/* gfa-sim: would a new car at (x, y) touch a car's body (traffic_spawn.pl)? */
static int body_in_way(const gta_traffic *tr, long x, long y, int z, int ang,
                       const gta_car_info *ci)
{
    int hl = gta_car_world_len(ci) / 2 + 2;
    int hw = gta_car_world_wid(ci) / 2 + GTA_LANE_KERB;
    int i;

    for (i = 0; i < tr->n; i++) {
        const gta_car *o = &tr->cars[i];
        const gta_car_info *oi = &tr->tiles->cars[o->model];
        if (o->done || o->layer != z)
            continue;
        if (box_hit(x, y, ang, hl, hw, o->x, o->y, o->face,
                    gta_car_world_len(oi) / 2, gta_car_world_wid(oi) / 2))
            return 1;
    }
    return 0;
}

C
once('the helper', qr/^(static int box_hit_block\()/m, sub { "$fn$1" });

once('the spawn', qr/^(\s*)if \(claim_any\(tr, cx, cy, z\)\)\n\s*continue;\n/m,
     sub {
         my $i = $1;
         "${i}if (claim_any(tr, cx, cy, z))\n$i    continue;\n"
       . "${i}if (body_in_way(tr, px, py, z, ang, &tr->tiles->cars[cand]))\n$i    continue;\n"
     });

print $c;
