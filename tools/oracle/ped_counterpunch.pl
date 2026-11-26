#!/usr/bin/perl
# Apply the original's punch counter to a copy of AmiGTA1's gta_peds.c, so the
# oracle runs the counter-punch measured from GTA8.EXE in M6b.
#
# AmiGTA1 has a wheel of 13 whose last three steps do nothing. GTA8.EXE's
# step 10 is the counter-punch and resets the wheel at once: the ped turns to
# face the puncher (and, in walktrace.c, the puncher goes down for about two
# seconds, `knocked`, with no harm done). So the wheel is 11 long, and
# gta_peds_punch returns -3 on the counter.
#
#     perl ped_counterpunch.pl < gta_peds.c > patched/gta_peds.c
#
# Each edit must apply exactly once, or the build stops.
#
# Grand Fair Use Auto, MIT.
use strict;
use warnings;

local $/;
my $c = <STDIN>;

sub edit {
    my ($name, $re, $with) = @_;
    my $n = 0;
    $n++ while $c =~ /$re/g;
    die "ped_counterpunch.pl: '$name' matched $n times, not once\n" if $n != 1;
    $c =~ s/$re/$with->()/e;
}

edit('wheel length',
     qr/ps->punch_wheel = \(ps->punch_wheel \+ 1\) % 13;/,
     sub { "ps->punch_wheel = (ps->punch_wheel + 1) % 11;" });
edit('counter',
     qr/if \(ps->punch_wheel >= 10\)\n\s*return -2;/,
     sub {
         "if (ps->punch_wheel >= 10) {\n"
       . "        gta_ped *q = &ps->p[hit];\n"
       . "        q->angle = (angle + 128) & 255;\n"
       . "        q->speed = 0;\n"
       . "        q->tx = q->ty = 0;\n"
       . "        q->mode = GTA_PED_MODE_IDLE;\n"
       . "        q->panic = 0;\n"
       . "        return -3;\n"
       . "    }";
     });

print $c;
