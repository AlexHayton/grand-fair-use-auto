#!/usr/bin/perl
# Apply gfa-sim's 23.333 Hz weapons clock and M6b 5-frame splat lifetime to a
# copy of AmiGTA1's gta_weapon.c, so the oracle runs the same gun cooldowns,
# projectile flight and splat lifetime measured from GTA8.EXE in M6b.
#
# Gun cooldowns (10/1/20/1), projectile steps/lifetimes (BULLET_SPEED 15,
# BULLET_LIFE 20, ROCKET_SPEED 10..15, FLAME_FRAMES 10), and bullet splats
# (5 frames) step at 23.333 Hz (Ratio 7/15 via gfa_weapons_set_step). Fires,
# blasts and car wreck fuses keep AmiGTA1's 50 Hz-scaled constants.
#
#     perl weapons_clock.pl < gta_weapon.c > patched/gta_weapon.c
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
    die "weapons_clock.pl: '$name' matched $n times, not once\n" if $n != 1;
    $c =~ s/$re/$with->()/e;
}

# Measured bullet splat lifetime: 5 frames @ 23.333 Hz (~214 ms).
edit('splat ticks',
     qr/^([ \t]*)s->ticks = GTA_SPLAT_TICKS;\n/m,
     sub { "$1s->ticks = 5;\n" });

# Inject external step gate controlled by fleet.h / walktrace / drivetrace.
edit('step setter',
     qr/^(void gta_weapons_tick\()/m,
     sub { "static int gfa_weapons_step = 1;\nvoid gfa_weapons_set_step(int s) { gfa_weapons_step = s; }\n\n$1" });

# Gate splat countdown inside gta_weapons_tick (not gta_weapons_draw_ground).
edit('splats gate',
     qr/^([ \t]*)(for \(i = 0; i < GTA_MAX_SPLATS; i\+\+\)\n[ \t]*if \(w->s\[i\]\.ticks > 0\)\n[ \t]*w->s\[i\]\.ticks--;)/m,
     sub { "$1if (gfa_weapons_step) {\n$1$2\n$1}" });

# Gate projectile flight inside gta_weapons_tick (after wreck_sweep to function end).
edit('bullets gate',
     qr/^([ \t]*wreck_sweep\(w, peds, tr, t, sc\);\n\n)([ \t]*for \(i = 0; i < GTA_MAX_BULLETS; i\+\+\) \{\n.*?\n[ \t]*\}\n)(\}\n\nvoid gta_weapons_draw_ground)/ms,
     sub { "$1    if (gfa_weapons_step) {\n$2    }\n$3" });

print $c;
