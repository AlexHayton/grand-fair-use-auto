#!/usr/bin/perl
# Apply the original's streak window and wreck pay to a copy of AmiGTA1's
# gta_score.c, so the oracle scores what M6b measured from GTA8.EXE
# (docs/M6B_TANK_TEST_PLAN.md, tank_chain).
#
# AmiGTA1's streak window is GTA_SCORE_STREAK_TICKS, 20 of its 50 Hz ticks
# (400 ms), shorter than one step of a chain (10 game frames, 429 ms). In
# GTA8.EXE it is 13 game frames (557 ms); fleet.h ticks the score only on the
# game frame. A car the player wrecked pays 100 when it blows up, doubling
# with each more inside the window (gta_score_wreck, its own streak type, as
# gfa-sim's score::kind::WRECK): 100, 200, 400, 800, 1600, 3200.
#
#     perl score_streak.pl < gta_score.c > patched/gta_score.c
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
    die "score_streak.pl: '$name' matched $n times, not once\n" if $n != 1;
    $c =~ s/$re/$with->()/e;
}

edit('window',
     qr/s->streak_timer = GTA_SCORE_STREAK_TICKS;/,
     sub { "s->streak_timer = 13;   /* gfa-sim: 13 game frames */" });

edit('wreck pay',
     qr/^(long gta_score_event\(gta_score \*s, int type, int reason\)\n)/m,
     sub {
         "/* gfa-sim: a car the player wrecked blowing up: 100, doubling with each\n"
       . " * more inside the window (at most 19 times). */\n"
       . "long gta_score_wreck(gta_score *s)\n"
       . "{\n"
       . "    long factor = 1;\n"
       . "    int k;\n"
       . "    if (s->streak_type == 0x80 && s->streak_timer > 0) {\n"
       . "        if (s->streak_count < 30)\n"
       . "            s->streak_count++;\n"
       . "    } else {\n"
       . "        s->streak_type = 0x80;\n"
       . "        s->streak_count = 1;\n"
       . "    }\n"
       . "    s->streak_timer = 13;\n"
       . "    for (k = 1; k < s->streak_count && k < 20; k++)\n"
       . "        factor *= 2;\n"
       . "    return gta_score_add(s, factor * 100);\n"
       . "}\n\n"
       . $1;
     });

print $c;
