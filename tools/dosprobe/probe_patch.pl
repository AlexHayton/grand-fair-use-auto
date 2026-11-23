#!/usr/bin/env perl
# Surgical patcher for DOSBox-X (tools/dosprobe/build/src).
# Fails loudly unless every expected edit matches exactly once.
use strict;
use warnings;

my $root = shift @ARGV or die "usage: probe_patch.pl <dosbox-x-src>\n";

sub patch_file {
    my ($rel, @edits) = @_;
    my $path = "$root/$rel";
    open my $fh, '<:raw', $path or die "open $path: $!\n";
    local $/;
    my $text = <$fh>;
    close $fh;

    for my $e (@edits) {
        my ($desc, $re, $fn) = @$e;
        my $n = ($text =~ s/$re/$fn->($&)/ge);
        die "$path: edit '$desc' matched $n times (expected 1)\n" unless $n == 1;
    }

    open my $out, '>:raw', $path or die "write $path: $!\n";
    print $out $text;
    close $out;
}

patch_file(
    'src/misc/Makefile.am',
    [
        'add gfa_probe.cpp to libmisc_a_SOURCES',
        qr{libmisc_a_SOURCES =[^\r\n]*?\bmkdir_p\.cpp},
        sub { "$_[0] gfa_probe.cpp" },
    ],
);

patch_file(
    'src/hardware/pic.cpp',
    [
        'include gfa_probe.h in pic.cpp',
        qr{#include "dosbox\.h"},
        sub { "$_[0]\n#include \"gfa_probe.h\"" },
    ],
    [
        'call gfa_probe_tick in TIMER_AddTick',
        qr{void TIMER_AddTick\(void\) \{\s*[^\n]*\n\s*PIC_Ticks\+\+;},
        sub { "$_[0]\n    gfa_probe_tick(PIC_Ticks);" },
    ],
);

patch_file(
    'src/gui/render.cpp',
    [
        'include gfa_probe.h in render.cpp',
        qr{#include "dosbox\.h"},
        sub { "$_[0]\n#include \"gfa_probe.h\"" },
    ],
    [
        'force fullFrame in RENDER_StartUpdate when probe wants frame',
        qr{if \(GCC_UNLIKELY\(CaptureState & \(CAPTURE_IMAGE\|CAPTURE_VIDEO\)\)\) \s*\n\s*render\.fullFrame = true;},
        sub { 'if (GCC_UNLIKELY((CaptureState & (CAPTURE_IMAGE|CAPTURE_VIDEO)) || gfa_probe_want_frame())) render.fullFrame = true;' },
    ],
    [
        'call CAPTURE_AddImage in RENDER_EndUpdate when probe wants frame',
        qr{if \(GCC_UNLIKELY\(CaptureState & \(CAPTURE_IMAGE\|CAPTURE_VIDEO\)\)\) \{},
        sub { 'if (GCC_UNLIKELY((CaptureState & (CAPTURE_IMAGE|CAPTURE_VIDEO)) || gfa_probe_want_frame())) {' },
    ],
);

patch_file(
    'src/hardware/hardware.cpp',
    [
        'include gfa_probe.h in hardware.cpp',
        qr{#include "dosbox\.h"},
        sub { "$_[0]\n#include \"gfa_probe.h\"" },
    ],
    [
        'invoke gfa_probe_on_frame at start of CAPTURE_AddImage',
        qr{void CAPTURE_AddImage\(Bitu width, Bitu height, Bitu bpp, Bitu pitch, Bitu flags, float fps, uint8_t \* data, uint8_t \* pal\) \{},
        sub { "$_[0]\n\tgfa_probe_on_frame(width, height, bpp, pitch, flags, data, pal);" },
    ],
);

patch_file(
    'src/dosbox.cpp',
    [
        'include gfa_probe.h in dosbox.cpp',
        qr{#include "dosbox\.h"},
        sub { "$_[0]\n#include \"gfa_probe.h\"" },
    ],
    [
        'poll main-loop probe actions in Normal_Loop',
        qr{static Bitu Normal_Loop\(void\) \{.*?try \{\s*\n\s*while \(1\) \{}s,
        sub { "$_[0]\n            gfa_probe_poll_main();" },
    ],
);

patch_file(
    'src/hardware/cmos.cpp',
    [
        'include gfa_probe.h in cmos.cpp',
        qr{#include "dosbox\.h"},
        sub { "$_[0]\n#include \"gfa_probe.h\"" },
    ],
    [
        'pin initial CMOS time when probe script is active',
        qr{cmos_sync_time\(time\(NULL\)\);},
        sub { 'cmos_sync_time(gfa_probe_init_time());' },
    ],
);

patch_file(
    'src/gui/sdlmain.cpp',
    [
        'include gfa_probe.h in sdlmain.cpp',
        qr{#include "dosbox\.h"},
        sub { "$_[0]\n#include \"gfa_probe.h\"" },
    ],
    [
        'pin host srand seed when probe script is active',
        qr{srand\(time\(NULL\)\);},
        sub { 'srand((unsigned int)gfa_probe_init_time());' },
    ],
);

patch_file(
    'src/dos/dos_files.cpp',
    [
        'include gfa_probe.h in dos_files.cpp',
        qr{#include "dosbox\.h"},
        sub { "$_[0]\n#include \"gfa_probe.h\"" },
    ],
    [
        'pin DOS_CreateTempFile RNG seed when probe script is active',
        qr{void initRand\(\) \{},
        sub { "$_[0]\n    if (gfa_probe_seed_rand()) return;" },
    ],
);

print "probe_patch.pl: all 8 files patched cleanly\n";
