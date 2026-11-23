#include "gfa_probe.h"

#include <algorithm>
#include <cctype>
#include <cerrno>
#include <cstdarg>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <sstream>
#include <string>
#include <sys/stat.h>
#include <vector>

#include "hardware.h"
#include "keyboard.h"
#include "logging.h"
#include "mem.h"
#include "pic.h"

namespace {

enum CmdKind {
    CMD_DOWN,
    CMD_UP,
    CMD_TAP,
    CMD_SHOT,
    CMD_BURST,
    CMD_MEMDUMP,
    CMD_LOADSTATE,
    CMD_SAVESTATE,
    CMD_QUIT
};

struct ProbeEvent {
    bool is_rel = false;
    uint64_t ms = 0;
    CmdKind kind = CMD_QUIT;
    KBD_KEYS key = KBD_NONE;
    uint64_t hold_ms = 50;
    uint32_t count = 0;
    uint64_t addr = 0;
    uint64_t len = 0;
    size_t slot = 0;
    std::string text;
};

struct PendingRelease {
    KBD_KEYS key;
    uint64_t release_at;
};

enum StateOp {
    STATE_NONE = 0,
    STATE_LOAD,
    STATE_SAVE
};

bool g_initialized = false;
bool g_active = false;
std::string g_out_dir = ".";
FILE *g_log_fp = nullptr;

std::vector<ProbeEvent> g_events;
size_t g_cursor = 0;
uint64_t g_next_target = 0;

bool g_held_keys[KBD_LAST] = {};
std::vector<PendingRelease> g_pending_releases;
std::vector<std::string> g_pending_shots;

std::string g_burst_prefix;
uint32_t g_burst_idx = 0;
uint32_t g_burst_remaining = 0;
uint64_t g_burst_every_ms = 0;
uint64_t g_burst_next_tick = 0;

StateOp g_pending_state_op = STATE_NONE;
size_t g_pending_state_slot = 0;

void probe_log(const char *fmt, ...) {
    char buf[1024];
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(buf, sizeof(buf), fmt, ap);
    va_end(ap);

    fprintf(stderr, "%s\n", buf);
    fflush(stderr);
    if (g_log_fp) {
        fprintf(g_log_fp, "%s\n", buf);
        fflush(g_log_fp);
    }
}

void fatal_probe(const char *msg) {
    fprintf(stderr, "probe FATAL: %s\n", msg);
    if (g_log_fp) {
        fprintf(g_log_fp, "probe FATAL: %s\n", msg);
        fflush(g_log_fp);
    }
    std::exit(2);
}

bool mkdir_recursive(const std::string &dir) {
    if (dir.empty()) return true;
    std::string cur;
    size_t i = 0;
    if (dir[0] == '/') {
        cur = "/";
        i = 1;
    }
    while (i <= dir.size()) {
        size_t slash = dir.find('/', i);
        std::string part = (slash == std::string::npos) ? dir.substr(i) : dir.substr(i, slash - i);
        if (!part.empty()) {
            if (!cur.empty() && cur.back() != '/') cur += '/';
            cur += part;
            if (mkdir(cur.c_str(), 0755) != 0 && errno != EEXIST) {
                return false;
            }
        }
        if (slash == std::string::npos) break;
        i = slash + 1;
    }
    return true;
}

bool safe_name(const std::string &s) {
    if (s.empty()) return false;
    for (unsigned char c : s) {
        if (!(std::isalnum(c) || c == '_' || c == '-' || c == '.')) return false;
    }
    return s != "." && s != ".." && s.find("..") == std::string::npos;
}

std::string to_lower_ascii(std::string s) {
    for (char &c : s) {
        c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
    }
    return s;
}

KBD_KEYS parse_key(const std::string &raw) {
    std::string k = to_lower_ascii(raw);
    struct Entry { const char *name; KBD_KEYS code; };
    static const Entry table[] = {
        {"up", KBD_up}, {"down", KBD_down}, {"left", KBD_left}, {"right", KBD_right},
        {"enter", KBD_enter}, {"return", KBD_enter}, {"space", KBD_space},
        {"tab", KBD_tab}, {"esc", KBD_esc}, {"escape", KBD_esc}, {"backspace", KBD_backspace},
        {"lctrl", KBD_leftctrl}, {"leftctrl", KBD_leftctrl}, {"ctrl", KBD_leftctrl},
        {"rctrl", KBD_rightctrl}, {"rightctrl", KBD_rightctrl},
        {"lshift", KBD_leftshift}, {"leftshift", KBD_leftshift}, {"shift", KBD_leftshift},
        {"rshift", KBD_rightshift}, {"rightshift", KBD_rightshift},
        {"lalt", KBD_leftalt}, {"leftalt", KBD_leftalt}, {"alt", KBD_leftalt},
        {"ralt", KBD_rightalt}, {"rightalt", KBD_rightalt},
        {"capslock", KBD_capslock}, {"scrolllock", KBD_scrolllock}, {"numlock", KBD_numlock},
        {"grave", KBD_grave}, {"minus", KBD_minus}, {"equals", KBD_equals},
        {"backslash", KBD_backslash}, {"leftbracket", KBD_leftbracket}, {"rightbracket", KBD_rightbracket},
        {"semicolon", KBD_semicolon}, {"quote", KBD_quote}, {"period", KBD_period},
        {"comma", KBD_comma}, {"slash", KBD_slash},
        {"printscreen", KBD_printscreen}, {"pause", KBD_pause},
        {"insert", KBD_insert}, {"home", KBD_home}, {"pageup", KBD_pageup},
        {"delete", KBD_delete}, {"end", KBD_end}, {"pagedown", KBD_pagedown},
        {"f1", KBD_f1}, {"f2", KBD_f2}, {"f3", KBD_f3}, {"f4", KBD_f4},
        {"f5", KBD_f5}, {"f6", KBD_f6}, {"f7", KBD_f7}, {"f8", KBD_f8},
        {"f9", KBD_f9}, {"f10", KBD_f10}, {"f11", KBD_f11}, {"f12", KBD_f12},
        {"kp0", KBD_kp0}, {"kp1", KBD_kp1}, {"kp2", KBD_kp2}, {"kp3", KBD_kp3}, {"kp4", KBD_kp4},
        {"kp5", KBD_kp5}, {"kp6", KBD_kp6}, {"kp7", KBD_kp7}, {"kp8", KBD_kp8}, {"kp9", KBD_kp9},
        {"kp_0", KBD_kp0}, {"kp_1", KBD_kp1}, {"kp_2", KBD_kp2}, {"kp_3", KBD_kp3}, {"kp_4", KBD_kp4},
        {"kp_5", KBD_kp5}, {"kp_6", KBD_kp6}, {"kp_7", KBD_kp7}, {"kp_8", KBD_kp8}, {"kp_9", KBD_kp9},
        {"kpdivide", KBD_kpdivide}, {"kp_divide", KBD_kpdivide},
        {"kpmultiply", KBD_kpmultiply}, {"kp_multiply", KBD_kpmultiply},
        {"kpminus", KBD_kpminus}, {"kp_minus", KBD_kpminus},
        {"kpplus", KBD_kpplus}, {"kp_plus", KBD_kpplus},
        {"kpenter", KBD_kpenter}, {"kp_enter", KBD_kpenter},
        {"kpperiod", KBD_kpperiod}, {"kp_period", KBD_kpperiod},
        {"0", KBD_0}, {"1", KBD_1}, {"2", KBD_2}, {"3", KBD_3}, {"4", KBD_4},
        {"5", KBD_5}, {"6", KBD_6}, {"7", KBD_7}, {"8", KBD_8}, {"9", KBD_9},
        {"a", KBD_a}, {"b", KBD_b}, {"c", KBD_c}, {"d", KBD_d}, {"e", KBD_e},
        {"f", KBD_f}, {"g", KBD_g}, {"h", KBD_h}, {"i", KBD_i}, {"j", KBD_j},
        {"k", KBD_k}, {"l", KBD_l}, {"m", KBD_m}, {"n", KBD_n}, {"o", KBD_o},
        {"p", KBD_p}, {"q", KBD_q}, {"r", KBD_r}, {"s", KBD_s}, {"t", KBD_t},
        {"u", KBD_u}, {"v", KBD_v}, {"w", KBD_w}, {"x", KBD_x}, {"y", KBD_y},
        {"z", KBD_z}
    };
    for (const auto &e : table) {
        if (k == e.name) return e.code;
    }
    return KBD_NONE;
}

bool parse_u64_dec(const std::string &s, uint64_t &out) {
    if (s.empty()) return false;
    for (unsigned char c : s) {
        if (!std::isdigit(c)) return false;
    }
    char *end = nullptr;
    errno = 0;
    unsigned long long v = std::strtoull(s.c_str(), &end, 10);
    if (errno != 0 || end == s.c_str() || *end != '\0') return false;
    out = static_cast<uint64_t>(v);
    return true;
}

bool parse_u64_hex(const std::string &s, uint64_t &out) {
    if (s.empty()) return false;
    char *end = nullptr;
    errno = 0;
    unsigned long long v = std::strtoull(s.c_str(), &end, 16);
    if (errno != 0 || end == s.c_str() || *end != '\0') return false;
    out = static_cast<uint64_t>(v);
    return true;
}

void set_key(KBD_KEYS k, bool pressed) {
    if (k > KBD_NONE && k < KBD_LAST) {
        g_held_keys[k] = pressed;
    }
    KEYBOARD_AddKey(k, pressed);
}

void release_all_held_keys() {
    for (int i = 1; i < KBD_LAST; ++i) {
        if (g_held_keys[i]) {
            KEYBOARD_AddKey(static_cast<KBD_KEYS>(i), false);
            g_held_keys[i] = false;
        }
    }
    g_pending_releases.clear();
}

void init_probe() {
    if (g_initialized) return;
    g_initialized = true;

    const char *script_path = std::getenv("GFA_PROBE_SCRIPT");
    if (!script_path || !*script_path) {
        g_active = false;
        return;
    }

    const char *out_env = std::getenv("GFA_PROBE_OUT");
    g_out_dir = (out_env && *out_env) ? out_env : ".";
    if (!mkdir_recursive(g_out_dir)) {
        fatal_probe("could not create GFA_PROBE_OUT directory");
    }

    std::string log_path = g_out_dir + "/probe.log";
    g_log_fp = std::fopen(log_path.c_str(), "w");
    if (!g_log_fp) {
        fatal_probe("could not open probe.log for writing");
    }

    std::ifstream in(script_path);
    if (!in.is_open()) {
        fatal_probe("could not open GFA_PROBE_SCRIPT");
    }

    std::string line;
    size_t lineno = 0;
    while (std::getline(in, line)) {
        lineno++;
        size_t hash = line.find('#');
        if (hash != std::string::npos) {
            line.erase(hash);
        }
        std::istringstream iss(line);
        std::vector<std::string> toks;
        std::string tok;
        while (iss >> tok) {
            toks.push_back(tok);
        }
        if (toks.empty()) continue;

        ProbeEvent ev;
        const std::string &t = toks[0];
        if (t[0] == '+') {
            ev.is_rel = true;
            if (!parse_u64_dec(t.substr(1), ev.ms)) {
                fatal_probe(("invalid relative timestamp at line " + std::to_string(lineno)).c_str());
            }
        } else {
            ev.is_rel = false;
            if (!parse_u64_dec(t, ev.ms)) {
                fatal_probe(("invalid timestamp at line " + std::to_string(lineno)).c_str());
            }
        }

        if (toks.size() < 2) {
            fatal_probe(("missing command at line " + std::to_string(lineno)).c_str());
        }
        std::string cmd = to_lower_ascii(toks[1]);
        if (cmd == "down" || cmd == "up") {
            if (toks.size() != 3) fatal_probe(("usage: <t> down|up <key> at line " + std::to_string(lineno)).c_str());
            ev.kind = (cmd == "down") ? CMD_DOWN : CMD_UP;
            ev.key = parse_key(toks[2]);
            ev.text = toks[2];
            if (ev.key == KBD_NONE) fatal_probe(("unknown key '" + toks[2] + "' at line " + std::to_string(lineno)).c_str());
        } else if (cmd == "tap") {
            if (toks.size() < 3 || toks.size() > 4) fatal_probe(("usage: <t> tap <key> [hold_ms] at line " + std::to_string(lineno)).c_str());
            ev.kind = CMD_TAP;
            ev.key = parse_key(toks[2]);
            ev.text = toks[2];
            if (ev.key == KBD_NONE) fatal_probe(("unknown key '" + toks[2] + "' at line " + std::to_string(lineno)).c_str());
            if (toks.size() == 4 && !parse_u64_dec(toks[3], ev.hold_ms)) {
                fatal_probe(("invalid tap hold_ms at line " + std::to_string(lineno)).c_str());
            }
        } else if (cmd == "shot") {
            if (toks.size() != 3 || !safe_name(toks[2])) {
                fatal_probe(("usage: <t> shot <name> at line " + std::to_string(lineno)).c_str());
            }
            ev.kind = CMD_SHOT;
            ev.text = toks[2];
        } else if (cmd == "burst") {
            if (toks.size() < 4 || toks.size() > 5 || !safe_name(toks[2])) {
                fatal_probe(("usage: <t> burst <prefix> <count> [every_ms] at line " + std::to_string(lineno)).c_str());
            }
            uint64_t cnt = 0;
            if (!parse_u64_dec(toks[3], cnt) || cnt == 0 || cnt > 100000) {
                fatal_probe(("invalid burst count at line " + std::to_string(lineno)).c_str());
            }
            ev.hold_ms = 0;
            if (toks.size() == 5 && !parse_u64_dec(toks[4], ev.hold_ms)) {
                fatal_probe(("invalid burst every_ms at line " + std::to_string(lineno)).c_str());
            }
            ev.kind = CMD_BURST;
            ev.text = toks[2];
            ev.count = static_cast<uint32_t>(cnt);
        } else if (cmd == "memdump") {
            if (toks.size() < 3 || toks.size() > 5 || toks.size() == 4 || !safe_name(toks[2])) {
                fatal_probe(("usage: <t> memdump <name> [hex_addr hex_len] at line " + std::to_string(lineno)).c_str());
            }
            ev.kind = CMD_MEMDUMP;
            ev.text = toks[2];
            if (toks.size() == 5) {
                if (!parse_u64_hex(toks[3], ev.addr) || !parse_u64_hex(toks[4], ev.len)) {
                    fatal_probe(("invalid memdump hex_addr/hex_len at line " + std::to_string(lineno)).c_str());
                }
            }
        } else if (cmd == "loadstate" || cmd == "savestate") {
            if (toks.size() != 3) {
                fatal_probe(("usage: <t> loadstate|savestate <slot> at line " + std::to_string(lineno)).c_str());
            }
            uint64_t slot1 = 0;
            if (!parse_u64_dec(toks[2], slot1) || slot1 < 1 || slot1 > SaveState::SLOT_COUNT * SaveState::MAX_PAGE) {
                fatal_probe(("invalid state slot (1..100) at line " + std::to_string(lineno)).c_str());
            }
            ev.kind = (cmd == "loadstate") ? CMD_LOADSTATE : CMD_SAVESTATE;
            ev.slot = static_cast<size_t>(slot1 - 1);
        } else if (cmd == "quit") {
            if (toks.size() != 2) fatal_probe(("usage: <t> quit at line " + std::to_string(lineno)).c_str());
            ev.kind = CMD_QUIT;
        } else {
            fatal_probe(("unknown command '" + cmd + "' at line " + std::to_string(lineno)).c_str());
        }

        g_events.push_back(ev);
    }

    g_active = true;
    g_cursor = 0;
    if (!g_events.empty()) {
        g_next_target = g_events[0].ms;
    }
    probe_log("probe: init script=%s out=%s events=%zu", script_path, g_out_dir.c_str(), g_events.size());
}

void write_ppm_rgb(const std::string &path, size_t w, size_t h, const std::vector<uint8_t> &rgb) {
    FILE *fp = std::fopen(path.c_str(), "wb");
    if (!fp) {
        probe_log("probe: ERROR failed to open shot path %s", path.c_str());
        return;
    }
    std::fprintf(fp, "P6\n%zu %zu\n255\n", w, h);
    if (!rgb.empty()) {
        std::fwrite(rgb.data(), 1, rgb.size(), fp);
    }
    std::fclose(fp);
}

void execute_memdump(const ProbeEvent &ev, uint64_t tick) {
    const uint8_t *base = GetMemBase();
    const size_t total = static_cast<size_t>(MEM_TotalPages()) * 4096u;
    if (!base || total == 0) {
        probe_log("probe: memdump %s SKIPPED (no guest RAM) tick=%llu", ev.text.c_str(), static_cast<unsigned long long>(tick));
        return;
    }
    if (ev.addr >= total) {
        probe_log("probe: memdump %s ERROR addr=0x%llx >= ram_size=0x%zx tick=%llu",
                  ev.text.c_str(), static_cast<unsigned long long>(ev.addr), total, static_cast<unsigned long long>(tick));
        return;
    }
    size_t len = (ev.len == 0) ? (total - static_cast<size_t>(ev.addr))
                               : std::min<size_t>(static_cast<size_t>(ev.len), total - static_cast<size_t>(ev.addr));
    std::string path = g_out_dir + "/" + ev.text + ".bin";
    FILE *fp = std::fopen(path.c_str(), "wb");
    if (!fp) {
        probe_log("probe: memdump %s ERROR failed to open %s", ev.text.c_str(), path.c_str());
        return;
    }
    std::fwrite(base + ev.addr, 1, len, fp);
    std::fclose(fp);
    probe_log("probe: memdump %s addr=0x%llx len=0x%zx tick=%llu",
              ev.text.c_str(), static_cast<unsigned long long>(ev.addr), len, static_cast<unsigned long long>(tick));
}

} // namespace

time_t gfa_probe_init_time(void) {
    const char *script_path = std::getenv("GFA_PROBE_SCRIPT");
    if (script_path && *script_path) {
        // 1998-01-01 12:00:00 UTC: fixed wall-clock seed/RTC for reproducible probes.
        return static_cast<time_t>(883656000);
    }
    return std::time(nullptr);
}

bool gfa_probe_seed_rand(void) {
    const char *script_path = std::getenv("GFA_PROBE_SCRIPT");
    if (script_path && *script_path) {
        static unsigned seq = 0;
        std::srand(883656000u + (++seq));
        return true;
    }
    return false;
}

void gfa_probe_poll_main(void) {
    if (!g_initialized) {
        init_probe();
    }
    if (!g_active || g_pending_state_op == STATE_NONE) {
        return;
    }

    StateOp op = g_pending_state_op;
    size_t slot = g_pending_state_slot;
    g_pending_state_op = STATE_NONE;

    uint64_t old_tick = static_cast<uint64_t>(PIC_Ticks);
    if (op == STATE_LOAD) {
        release_all_held_keys();
        g_pending_shots.clear();
        g_burst_remaining = 0;
        try {
            SaveState::instance().load(slot);
        } catch (const SaveState::Error &err) {
            fatal_probe(("loadstate failed: " + err).c_str());
        }
        uint64_t new_tick = static_cast<uint64_t>(PIC_Ticks);
        probe_log("probe: loadstate slot=%zu old_tick=%llu new_tick=%llu",
                  slot + 1, static_cast<unsigned long long>(old_tick), static_cast<unsigned long long>(new_tick));
        if (g_cursor < g_events.size()) {
            g_next_target = g_events[g_cursor].is_rel ? (new_tick + g_events[g_cursor].ms) : g_events[g_cursor].ms;
        }
    } else if (op == STATE_SAVE) {
        try {
            SaveState::instance().save(slot);
        } catch (const SaveState::Error &err) {
            fatal_probe(("savestate failed: " + err).c_str());
        }
        probe_log("probe: savestate slot=%zu tick=%llu", slot + 1, static_cast<unsigned long long>(old_tick));
        if (g_cursor < g_events.size()) {
            g_next_target = g_events[g_cursor].is_rel ? (old_tick + g_events[g_cursor].ms) : g_events[g_cursor].ms;
        }
    }
}

void gfa_probe_tick(Bitu pic_ticks) {
    if (!g_initialized) {
        init_probe();
    }
    if (!g_active) {
        return;
    }

    uint64_t now = static_cast<uint64_t>(pic_ticks);

    // Complete any scheduled tap releases due at this tick.
    for (size_t i = 0; i < g_pending_releases.size();) {
        if (now >= g_pending_releases[i].release_at) {
            set_key(g_pending_releases[i].key, false);
            g_pending_releases.erase(g_pending_releases.begin() + static_cast<ptrdiff_t>(i));
        } else {
            ++i;
        }
    }

    if (g_pending_state_op != STATE_NONE) {
        return;
    }

    while (g_cursor < g_events.size() && now >= g_next_target) {
        const ProbeEvent &ev = g_events[g_cursor];
        uint64_t fired_at = g_next_target;
        switch (ev.kind) {
            case CMD_DOWN:
                set_key(ev.key, true);
                probe_log("probe: down %s tick=%llu", ev.text.c_str(), static_cast<unsigned long long>(now));
                break;
            case CMD_UP:
                set_key(ev.key, false);
                probe_log("probe: up %s tick=%llu", ev.text.c_str(), static_cast<unsigned long long>(now));
                break;
            case CMD_TAP:
                set_key(ev.key, true);
                g_pending_releases.push_back(PendingRelease{ev.key, now + ev.hold_ms});
                probe_log("probe: tap %s hold=%llu tick=%llu",
                          ev.text.c_str(), static_cast<unsigned long long>(ev.hold_ms), static_cast<unsigned long long>(now));
                break;
            case CMD_SHOT:
                g_pending_shots.push_back(ev.text);
                probe_log("probe: queue shot %s tick=%llu", ev.text.c_str(), static_cast<unsigned long long>(now));
                break;
            case CMD_BURST:
                g_burst_prefix = ev.text;
                g_burst_idx = 0;
                g_burst_remaining = ev.count;
                g_burst_every_ms = ev.hold_ms;
                g_burst_next_tick = (ev.hold_ms == 0) ? 0 : now;
                probe_log("probe: start burst %s count=%u every_ms=%llu tick=%llu",
                          ev.text.c_str(), ev.count, static_cast<unsigned long long>(ev.hold_ms), static_cast<unsigned long long>(now));
                break;
            case CMD_MEMDUMP:
                execute_memdump(ev, now);
                break;
            case CMD_LOADSTATE:
                g_pending_state_op = STATE_LOAD;
                g_pending_state_slot = ev.slot;
                g_cursor++;
                return;
            case CMD_SAVESTATE:
                g_pending_state_op = STATE_SAVE;
                g_pending_state_slot = ev.slot;
                g_cursor++;
                return;
            case CMD_QUIT:
                release_all_held_keys();
                probe_log("probe: quit tick=%llu pending_shots=%zu burst_remaining=%u",
                          static_cast<unsigned long long>(now), g_pending_shots.size(), g_burst_remaining);
                if (g_log_fp) {
                    std::fclose(g_log_fp);
                    g_log_fp = nullptr;
                }
                throw int(1);
        }

        g_cursor++;
        if (g_cursor < g_events.size()) {
            const ProbeEvent &nx = g_events[g_cursor];
            g_next_target = nx.is_rel ? (fired_at + nx.ms) : nx.ms;
        }
    }
}

bool gfa_probe_want_frame(void) {
    if (!g_active) return false;
    if (!g_pending_shots.empty()) return true;
    if (g_burst_remaining > 0 && static_cast<uint64_t>(PIC_Ticks) >= g_burst_next_tick) return true;
    return false;
}

void gfa_probe_on_frame(Bitu width, Bitu height, Bitu bpp, Bitu pitch, Bitu flags, const uint8_t *data, const uint8_t *pal) {
    if (!g_active || !gfa_probe_want_frame()) {
        return;
    }
    if (!data || width == 0 || height == 0) {
        return;
    }

    uint64_t now = static_cast<uint64_t>(PIC_Ticks);
    bool take_burst = (g_burst_remaining > 0 && now >= g_burst_next_tick);
    bool take_shot = !g_pending_shots.empty();
    if (!take_burst && !take_shot) {
        return;
    }

    const bool dblw = (flags & CAPTURE_FLAG_DBLW) != 0;
    const bool dblh = (flags & CAPTURE_FLAG_DBLH) != 0;
    const size_t out_w = static_cast<size_t>(width) * (dblw ? 2u : 1u);
    const size_t out_h = static_cast<size_t>(height) * (dblh ? 2u : 1u);

    std::vector<uint8_t> rgb(out_w * out_h * 3u);
    for (size_t y = 0; y < out_h; ++y) {
        const size_t sy = dblh ? (y >> 1) : y;
        const uint8_t *src = data + sy * static_cast<size_t>(pitch);
        uint8_t *dst = rgb.data() + y * out_w * 3u;
        for (size_t x = 0; x < static_cast<size_t>(width); ++x) {
            uint8_t r = 0, g = 0, b = 0;
            switch (bpp) {
                case 8: {
                    if (pal) {
                        const uint8_t idx = src[x];
                        r = pal[idx * 4u + 0u];
                        g = pal[idx * 4u + 1u];
                        b = pal[idx * 4u + 2u];
                    }
                    break;
                }
                case 15: {
                    const uint16_t px = reinterpret_cast<const uint16_t *>(src)[x];
                    b = static_cast<uint8_t>(((px & 0x001fu) * 0x21u) >> 2);
                    g = static_cast<uint8_t>(((px & 0x03e0u) * 0x21u) >> 7);
                    r = static_cast<uint8_t>(((px & 0x7c00u) * 0x21u) >> 12);
                    break;
                }
                case 16: {
                    const uint16_t px = reinterpret_cast<const uint16_t *>(src)[x];
                    b = static_cast<uint8_t>(((px & 0x001fu) * 0x21u) >> 2);
                    g = static_cast<uint8_t>(((px & 0x07e0u) * 0x41u) >> 9);
                    r = static_cast<uint8_t>(((px & 0xf800u) * 0x21u) >> 13);
                    break;
                }
                case 24: {
                    b = src[x * 3u + 0u];
                    g = src[x * 3u + 1u];
                    r = src[x * 3u + 2u];
                    break;
                }
                case 32: {
                    b = src[x * 4u + 0u];
                    g = src[x * 4u + 1u];
                    r = src[x * 4u + 2u];
                    break;
                }
                default:
                    break;
            }
            if (dblw) {
                const size_t o = x * 6u;
                dst[o + 0] = r; dst[o + 1] = g; dst[o + 2] = b;
                dst[o + 3] = r; dst[o + 4] = g; dst[o + 5] = b;
            } else {
                const size_t o = x * 3u;
                dst[o + 0] = r; dst[o + 1] = g; dst[o + 2] = b;
            }
        }
    }

    if (take_shot) {
        std::vector<std::string> shots;
        shots.swap(g_pending_shots);
        for (const std::string &name : shots) {
            std::string path = g_out_dir + "/" + name + ".ppm";
            write_ppm_rgb(path, out_w, out_h, rgb);
            probe_log("probe: shot %s %zux%zu bpp=%lu tick=%llu",
                      name.c_str(), out_w, out_h, static_cast<unsigned long>(bpp), static_cast<unsigned long long>(now));
        }
    }

    if (take_burst) {
        char fname[64];
        std::snprintf(fname, sizeof(fname), "_%04u.ppm", g_burst_idx);
        std::string name = g_burst_prefix + fname;
        std::string path = g_out_dir + "/" + name;
        write_ppm_rgb(path, out_w, out_h, rgb);
        probe_log("probe: burst %s %zux%zu bpp=%lu tick=%llu",
                  name.c_str(), out_w, out_h, static_cast<unsigned long>(bpp), static_cast<unsigned long long>(now));
        g_burst_idx++;
        g_burst_remaining--;
        if (g_burst_remaining > 0) {
            if (g_burst_every_ms == 0) {
                g_burst_next_tick = 0;
            } else {
                g_burst_next_tick += g_burst_every_ms;
                if (g_burst_next_tick <= now) {
                    g_burst_next_tick = now + 1;
                }
            }
        }
    }
}
