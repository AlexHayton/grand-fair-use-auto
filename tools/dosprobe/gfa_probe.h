#ifndef DOSBOX_GFA_PROBE_H
#define DOSBOX_GFA_PROBE_H

#include <cstdint>
#include <ctime>
#include "dosbox.h"

time_t gfa_probe_init_time(void);
bool gfa_probe_seed_rand(void);
void gfa_probe_tick(Bitu pic_ticks);
void gfa_probe_poll_main(void);
bool gfa_probe_want_frame(void);
void gfa_probe_on_frame(Bitu width, Bitu height, Bitu bpp, Bitu pitch, Bitu flags, const uint8_t *data, const uint8_t *pal);

#endif
