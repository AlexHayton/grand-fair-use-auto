#!/usr/bin/perl
# Apply gfa-sim's dead-end rule to a copy of AmiGTA1's gta_traffic.c, so the
# oracle runs the fleet the game runs (decision log, 2026-11-21).
#
# A car with nowhere legal to go (a long vehicle in a street it cannot
# leave) waits, and AmiGTA1 deletes it after GTA_TRAFFIC_GIVEUP ticks
# wherever it is, in view or not. gfa-sim deletes it only once it is out of
# sight, as AmiGTA1 already does for a car abandoned in a queue, so nothing
# vanishes where the player can see it. This makes the same change to the C.
#
#     perl traffic_giveup.pl < gta_traffic.c > patched/gta_traffic.c
#
# The edit must apply exactly once, or the build stops.
#
# Grand Fair Use Auto, MIT.
use strict;
use warnings;

local $/;
my $c = <STDIN>;

my $re = qr/^([ \t]*)if \(c->wait > GTA_TRAFFIC_GIVEUP\)\n/m;
my $n = 0;
$n++ while $c =~ /$re/g;
die "traffic_giveup.pl: matched $n times, not once\n" if $n != 1;
$c =~ s/$re/$1if (c->wait > GTA_TRAFFIC_GIVEUP && !in_view(tr, bx, by))\n/;

print $c;
