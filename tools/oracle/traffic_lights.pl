#!/usr/bin/perl
# Use the measured GTA8.EXE cycle in the AmiGTA1 harnesses.
# See docs/M6B_PROBES.md, Traffic light cycle.
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
    die "traffic_lights.pl: $what matched $n times, expected one\n" if $n != 1;
    $c =~ s/$re/$with->()/e;
}

my $state = <<'C';
    /* Two 176-frame halves. DOS runs seven frames per fifteen fleet ticks. */
    unsigned long p = (tr->tick / 15UL * 7UL + tr->tick % 15UL * 7UL / 15UL)
                      % 352UL;
    (void)bx;
    (void)by;
    if (along_x)
        p = (p + 176UL) % 352UL;
    if (p < 96UL)
        return 1;
    return (p < 136UL || p >= 312UL) ? 2 : 0;
C
once('lamp state',
     qr/^(int gta_traffic_light_state\([^\n]+\)\n\{\n).*?^\}/ms,
     sub { "$1$state}" });

my $green = <<'C';
    return gta_traffic_light_state(tr, bx, by, along_x) == 1;
}
C
once('green check',
     qr/^(int gta_traffic_light_green\([^\n]+\)\n\{\n).*?^\}/ms,
     sub { "$1$green" });

print $c;
