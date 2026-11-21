#!/usr/bin/perl
# Apply gfa-sim's sidestep fix to a copy of AmiGTA1's gta_peds.c, so the
# oracle runs the crowd the game runs (decision log, 2026-11-21).
#
# AmiGTA1 bends a crowded ped's heading a quarter turn for each tick of a
# sidestep, assuming his mode re-aims him every tick. A wanderer with no
# target is not re-aimed, so the bend piled up and he spun on the spot; and
# a side found walled was tried again the next tick, so a man along a kerb
# turned a quarter and back on alternate ticks. gfa-sim takes last tick's
# bend off before the mode aims him (`Ped::bent`), forgets it when the step
# turns him, and gives up the bend for the rest of a sidestep once that
# side is walled. This makes the same change to the C.
#
#     perl ped_sidestep.pl < gta_peds.c > patched/gta_peds.c
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
    die "ped_sidestep.pl: '$name' matched $n times, not once\n" if $n != 1;
    $c =~ s/$re/$with->()/e;
}

# The bend each ped carries into his next tick, by slot.
edit('declare', qr/^(static void ped_reset\()/m,
     sub { "static int gfa_bent[GTA_MAX_PEDS];\n\n$1" });
# A new ped carries none.
edit('reset', qr/^(\s*p->dodge = 0; p->dodge_side = 1;\n)/m,
     sub { "$1    gfa_bent[p - ps->p] = 0;\n" });
# Two locals for the tick loop: the bend taken off and the heading
# before the step.
edit('locals', qr/^([ \t]*)int bx, by, in_view;\n/m,
     sub { "$1int bx, by, in_view;\n$1int gfa_was = 0, gfa_a0 = -1;\n" });
# Last tick's bend, taken before anything else...
edit('take', qr/^([ \t]*)(if \(p->pull >= 0\) \{\n\s*\/\* DRAGGED OUT)/m,
     sub { "$1gfa_was = gfa_bent[i];\n$1gfa_bent[i] = 0;\n$1$2" });
# ...and off his heading once he is on his feet, before the brain.
edit('unbend', qr/^([ \t]*)(\/\* ---- the brain proper ---- \*\/)/m,
     sub { "$1p->angle = (p->angle - gfa_was) & 255;\n$1$2" });
# The bend, remembered; a walled side, given up.
edit('bend', qr/^([ \t]*)if \(p->dodge_side != 0 && p->speed > 0\n\s*&& step_free\(ps, p, a, 8\)\)\n\s*p->angle = a;\n/m,
     sub {
         my $s = $1;
         "${s}if (p->dodge_side != 0 && p->speed > 0) {\n"
         . "${s}    if (step_free(ps, p, a, 8)) {\n"
         . "${s}        p->angle = a;\n"
         . "${s}        gfa_bent[i] = PED_DODGE_TURN * p->dodge_side;\n"
         . "${s}    } else\n"
         . "${s}        p->dodge_side = 0;\n"
         . "${s}}\n"
     });
# The heading before the step...
edit('before step', qr/^([ \t]*)(\/\* THE STEP, and what refuses it\. \*\/)/m,
     sub { "$1gfa_a0 = p->angle;\n$1$2" });
# ...and a turn the step made is a new heading, not a bend.
edit('after step', qr/^([ \t]*)(stuck_log\(ps, p, i\);)/m,
     sub { "$1if (gfa_a0 >= 0 && p->angle != gfa_a0)\n$1    gfa_bent[i] = 0;\n$1$2" });

print $c;
