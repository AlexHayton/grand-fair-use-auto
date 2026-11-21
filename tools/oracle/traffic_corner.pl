#!/usr/bin/perl
# Apply gfa-sim's corner rule to a copy of AmiGTA1's gta_traffic.c, so the
# oracle turns corners as the game does (decision log, 2026-11-21).
#
# Two things sent cars straight past a corner and off the road where a
# one-way street turns and there is no way on (Liberty City's loop by
# (106,114), where they jammed it). A turn begun a block early asked whether
# the block beside the CAR had room, which is the far lane of the street it
# was turning into, not the street; and a car refused its turn for a busy
# exit carried on as if straight on were open, which only a junction makes
# true. gfa-sim asks about the block beside the turning block, and a car
# refused its turn where it cannot go straight on waits for the exit with
# the give-way brake AmiGTA1 keeps for a car on a long route, for as long as
# the exit is full rather than AmiGTA1's half second before forcing its way
# in. This makes the same changes to the C.
#
#     perl traffic_corner.pl < gta_traffic.c > patched/gta_traffic.c
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
    die "traffic_corner.pl: $what matched $n times, not once\n" if $n != 1;
    $c =~ s/$re/$with->()/e;
}

# The street the turn leads into: beside the turning block.
once('the exit block',
     qr/^(\s*)nx = bx \+ twx;\n\s*ny = by \+ twy;\n(\s*\/\* AND NOT TWICE INSIDE ONE CROSSING)/m,
     sub { "$1nx = bx + (ahead ? dxs : 0) + twx;\n$1ny = by + (ahead ? dys : 0) + twy;\n$2" });

# Can a car go straight on from its turning block? Before the fleet's
# driving, which asks.
my $way_on = <<'C';
/* gfa-sim: from the block a car turns at, a way straight on
 * (traffic_corner.pl). */
static int corner_way_on(const gta_map *m, const gta_car *c, int bx, int by,
                         int ahead, int dxs, int dys)
{
    int tx = bx + (ahead ? dxs : 0), ty = by + (ahead ? dys : 0);

    return dir_allowed(m, tx, ty, c->layer, c->angle) &&
           ground_at(m, tx + dxs, ty + dys, c->layer) == GROUND_ROAD;
}

C
once('the drive', qr/^(static void drive_one\(gta_traffic \*tr, const gta_map \*m, int idx\)\n\{)/m,
     sub { "$way_on$1" });

# Refused where there is no way straight on: wait for the corner, as long as
# it takes.
once('the patience',
     qr/^(\s*)if \(tr->opt_keepclear && exit_busy &&\n\s*c->wait <= GTA_SWEEP_PATIENCE\) \{\n/m,
     sub {
         my $i = $1;
         "${i}if (tr->opt_keepclear && exit_busy &&\n"
       . "$i    (c->wait <= GTA_SWEEP_PATIENCE ||\n"
       . "$i     !corner_way_on(m, c, bx, by, ahead, dxs, dys))) {\n"
     });
once('the refused corner',
     qr/^(\s*)if \(c->mdest_on && c->path_i < c->path_n\)\n\s*give_way = 1;\n/m,
     sub {
         my $i = $1;
         "${i}if (c->mdest_on && c->path_i < c->path_n)\n"
       . "$i    give_way = 1;\n"
       . "${i}/* gfa-sim: a corner with no way on (traffic_corner.pl). */\n"
       . "${i}if (!corner_way_on(m, c, bx, by, ahead, dxs, dys))\n"
       . "$i    give_way = 1;\n"
     });

print $c;
