#!/usr/bin/perl
# Match gfa-sim's fleet overlap correction. The closing axis still
# determines the impulse, while positions separate along the shallowest axis.
#
# Grand Fair Use Auto, MIT.
use strict;
use warnings;

local $/;
my $c = <STDIN>;
my $start = index($c, 'static void fleet_collide(');
my $end = index($c, 'static void fleet_collisions(', $start);
die "traffic_contact.pl: could not locate fleet_collide\n" if $start < 0 || $end < 0;

my $separation = <<'C';
        long sep_nx, sep_ny, sep_depth, sep, mvx, mvy;
        box_mtv(a->x, a->y, a->face, ahl, ahw,
                b->x, b->y, b->face, bhl, bhw, 0, 0,
                &sep_nx, &sep_ny, &sep_depth);
        sep = sep_depth > GTA_SEP_SLOP ? sep_depth - GTA_SEP_SLOP : 0;
C
my $collision = substr($c, $start, $end - $start);
my $block_pattern = qr/^        long sep = depth > GTA_SEP_SLOP.*?(?=^        b->x \+=)/ms;
my $n = $collision =~ s{$block_pattern}{
    my $block = $&;
    $block =~ s/^        long sep = .*\n        long mvx, mvy;\n/$separation/;
    $block =~ s/(?<!sep_)\b(nx|ny|depth)\b/sep_$1/g;
    $block;
}e;
die "traffic_contact.pl: separation matched $n times, expected one\n" if $n != 1;
substr($c, $start, $end - $start, $collision);
print $c;
