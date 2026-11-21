//! Trains: Liberty City's elevated line, Vice City's, San Andreas' trams.
//!
//! Ported from AmiGTA1 `gta_train.c` (MIT). No mission declares a train.
//! At load the railway blocks are walked into closed loops, every station
//! (traffic hint 6 or 7) is recorded, and a four-carriage train is put down
//! at each station that is also a spawn point (hint 7), at most six. A
//! train on a road block is a tram, and trams never move, as in the
//! original.
//!
//! The bogies follow straight runs between block centres, joined at every
//! corner by a quarter circle tangent to both: half a block of radius, or a
//! block and a half where the map marks a wide curve (hints 4 and 5).
//! Corners closer than their radii allow share the room between them.
//!
//! The logic runs at the original's 25 Hz (every other tick); movement is
//! spread over both ticks. Trains stop at stations, open and close their
//! doors, wait, and leave when the next station is clear (a block signal).
//! A bogie meeting another train's wrecks both; one over a parked car
//! dents it and halves the train's speed; one over the player on foot runs
//! him over. The player can board a stopped train, ride it, get off where
//! there is pavement beside it, and take the controls with FIRE.
//!
//! One difference from AmiGTA1: its train random numbers come from one
//! static seed shared by every level it plays; here each level starts from
//! the same seed, so a replay does not depend on what was played before.

use crate::clock::Half;
use crate::hash::{SimHash, StateHasher};
use crate::map::{LAYERS, SimMap, ground};
use gfa_fixed::rng::{NrLcg, TRAIN_SEED};
use gfa_fixed::trig::{cos, dir16, sin};

const FP: u32 = 16;
/// World px a block.
const BLK: i64 = 32;
/// Carriage pitch, 126 original px.
const CAR_PITCH: i64 = 63;
/// Bogies sit 20 original px in from each end.
const BOGIE_IN: i64 = 10;
const BOGIE_GAP: i64 = CAR_PITCH - 2 * BOGIE_IN;
const MAX_TRAINS: usize = 6;
pub const CARRIAGES: usize = 4;
const MAX_NODES: usize = 900;
const MAX_LOOPS: usize = 4;
const MAX_STATIONS: usize = 16;

/// The twelve walking steps: x, y and layer change (+1 up).
const ST_DX: [i32; 13] = [0, 1, 1, 1, -1, -1, -1, 0, 0, 0, 0, 0, 0];
const ST_DY: [i32; 13] = [0, 0, 0, 0, 0, 0, 0, -1, -1, -1, 1, 1, 1];
const ST_DL: [i32; 13] = [0, 1, -1, 0, 1, -1, 0, 1, -1, 0, 1, -1, 0];
/// Candidate steps per heading group; the last that is rail wins.
const CAND: [[usize; 9]; 4] = [
    [11, 8, 10, 7, 9, 12, 1, 2, 3],
    [8, 11, 7, 10, 12, 9, 4, 5, 6],
    [5, 2, 4, 1, 3, 6, 7, 8, 9],
    [5, 2, 4, 1, 3, 6, 10, 11, 12],
];

/// Train states, the original's numbers.
pub mod state {
    pub const STATION: u8 = 2;
    pub const GO: u8 = 3;
    pub const BRAKE: u8 = 4;
    pub const WAIT_TO_LEAVE: u8 = 10;
    pub const STOPPED: u8 = 11;
    pub const RESUME: u8 = 12;
}

/// A block of track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackNode {
    pub x: u8,
    pub y: u8,
    pub layer: u8,
    pub hint: u8,
}

/// A straight run or a quarter circle of a loop's path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Piece {
    /// Line: its start. Arc: the corner block's centre.
    x0: i64,
    y0: i64,
    /// Where it starts along the loop, and its length (16.16 px).
    s0: i64,
    len: i64,
    /// Line: its direction. Arc: the way in, then the way out.
    d1: (i64, i64),
    d2: (i64, i64),
    /// Arc radius, px.
    r: i64,
    /// Line: the corner it leaves. Arc: its corner.
    node: usize,
    /// Line: the radius of the corner it leaves.
    r_from: i64,
    arc: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Loop {
    first: usize,
    len: usize,
    piece0: usize,
    pieces: usize,
    total: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Station {
    node: usize,
    loop_: usize,
    busy: bool,
}

/// One train.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Train {
    pub tram: bool,
    loop_: usize,
    pub state: u8,
    after_brake: u8,
    pub sub: u8,
    sub_first: bool,
    /// The player has the controls.
    pub driven: bool,
    /// Running backwards (the original's sense 7).
    pub reversed: bool,
    /// 8 shut, 1..7 opening and closing.
    pub door: u8,
    pub wrecked: bool,
    /// The original's units: speed / 10 original px a frame.
    pub speed: i16,
    pub max: i16,
    wait: i16,
    drop: i16,
    cur_st: usize,
    next_st: usize,
    last_st_node: usize,
    /// The lead bogie, 16.16 px along the loop.
    pub s: i64,
    lead_node: usize,
}

impl Train {
    /// Carriages in this train.
    pub fn carriages(&self) -> usize {
        if self.tram { 1 } else { CARRIAGES }
    }
}

/// A carriage as the renderer draws it: its body centre and the two bogies
/// (16.16 px), layer, and heading (256 steps).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Carriage {
    pub body: (i64, i64),
    pub front: (i64, i64),
    pub back: (i64, i64),
    pub layer: i32,
    pub angle: i32,
    /// First and last carriage: they get the end pieces.
    pub first: bool,
    pub last: bool,
}

/// Something a train did this tick, for the world to act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrainEvent {
    /// Ran the player over (he was on foot).
    RanOver,
    /// Train `n` was wrecked; its carriages become wrecks here.
    Wrecked(usize, Vec<(i64, i64, i32, i32)>),
    /// A bogie went over parked car `i`: 20 points of damage.
    Crushed(usize),
    /// Ran over ped `i` of the list it was given (it may come twice).
    KilledPed(usize),
    /// Train `n` has stopped at a station.
    Stopped(usize),
}

/// Every train on the map.
#[derive(Debug, Clone)]
pub struct Trains {
    nodes: Vec<TrackNode>,
    loops: Vec<Loop>,
    pieces: Vec<Piece>,
    node_s: Vec<i64>,
    stations: Vec<Station>,
    pub trains: Vec<Train>,
    half: Half,
    rng: NrLcg,
    /// The train the player rides.
    pub rider: Option<usize>,
}

fn rail_at(map: &SimMap, x: i32, y: i32, l: i32) -> Option<u8> {
    if !(0..256).contains(&x) || !(0..256).contains(&y) || !(0..LAYERS).contains(&l) {
        return None;
    }
    let b = map.block(x, y, l)?;
    b.is_railway().then(|| b.traffic_hint())
}

impl Trains {
    /// Walk the rail into loops, find the stations, put the trains down.
    /// An open line (or more than four loops) means no trains, as in the
    /// original.
    pub fn new(map: &SimMap) -> Trains {
        let mut ts = Trains {
            nodes: Vec::new(),
            loops: Vec::new(),
            pieces: Vec::new(),
            node_s: Vec::new(),
            stations: Vec::new(),
            trains: Vec::new(),
            // The logic runs on the second tick of each pair.
            half: Half::new(false),
            rng: NrLcg::new(TRAIN_SEED),
            rider: None,
        };
        // Top-left first, the top layer first within a column.
        for y in 0..256 {
            for x in 0..256 {
                for l in (0..LAYERS).rev() {
                    if rail_at(map, x, y, l).is_none() || ts.seen(x, y, l) {
                        continue;
                    }
                    let first = ts.nodes.len();
                    if ts.loops.len() >= MAX_LOOPS || !ts.walk(map, x, y, l) {
                        return ts.none();
                    }
                    ts.loops.push(Loop {
                        first,
                        len: ts.nodes.len() - first,
                        ..Loop::default()
                    });
                }
            }
        }
        ts.node_s = vec![0; ts.nodes.len()];
        for i in 0..ts.loops.len() {
            if !ts.build_path(i) {
                return ts.none();
            }
        }
        for (i, lp) in ts.loops.clone().iter().enumerate() {
            for k in 0..lp.len {
                let nd = lp.first + k;
                if matches!(ts.nodes[nd].hint, 6 | 7) && ts.stations.len() < MAX_STATIONS {
                    ts.stations.push(Station {
                        node: nd,
                        loop_: i,
                        busy: false,
                    });
                }
            }
        }
        for i in 0..ts.stations.len() {
            if ts.trains.len() >= MAX_TRAINS {
                break;
            }
            let (nd, l) = (ts.stations[i].node, ts.stations[i].loop_);
            let n = ts.nodes[nd];
            if n.hint != 7 {
                continue;
            }
            let tram = map.ground_type(n.x.into(), n.y.into(), n.layer.into()) == ground::ROAD;
            let mut s = ts.node_s[nd] + ((2 * CAR_PITCH - BOGIE_IN - 16) << FP);
            if s >= ts.loops[l].total {
                s -= ts.loops[l].total;
            }
            let lead_node = ts.point_at(l, s).3;
            ts.trains.push(Train {
                tram,
                loop_: l,
                state: state::STATION,
                after_brake: 0,
                sub: 0,
                sub_first: false,
                driven: false,
                reversed: false,
                door: 8,
                wrecked: false,
                speed: 0,
                max: if tram { 30 } else { 60 },
                wait: 0,
                drop: 0,
                cur_st: i,
                next_st: i,
                last_st_node: nd,
                s,
                lead_node,
            });
            ts.stations[i].busy = true;
        }
        ts
    }

    fn none(mut self) -> Trains {
        self.loops.clear();
        self.trains.clear();
        self.stations.clear();
        self
    }

    /// A summary for the run log.
    pub fn describe(&self) -> String {
        let trams = self.trains.iter().filter(|t| t.tram).count();
        format!(
            "trains: {} loops, {} nodes, {} stations, {} trains, {} trams",
            self.loops.len(),
            self.nodes.len(),
            self.stations.len(),
            self.trains.len() - trams,
            trams
        )
    }

    fn seen(&self, x: i32, y: i32, l: i32) -> bool {
        self.nodes
            .iter()
            .any(|n| i32::from(n.x) == x && i32::from(n.y) == y && i32::from(n.layer) == l)
    }

    /// Follow the rail from `(sx, sy, sl)` until it comes back. False for a
    /// dead end or a loop too long.
    fn walk(&mut self, map: &SimMap, sx: i32, sy: i32, sl: i32) -> bool {
        let (mut x, mut y, mut l) = (sx, sy, sl);
        let mut head = 3usize;
        let mut hint = rail_at(map, x, y, l).unwrap_or(0);
        let mut guard = 0;
        loop {
            if self.nodes.len() >= MAX_NODES {
                return false;
            }
            self.nodes.push(TrackNode {
                x: x as u8,
                y: y as u8,
                layer: l as u8,
                hint,
            });
            let g = match head {
                0..=3 => 0,
                4..=6 => 1,
                7..=9 => 2,
                _ => 3,
            };
            let best = CAND[g]
                .iter()
                .copied()
                .rev()
                .find(|&c| rail_at(map, x + ST_DX[c], y + ST_DY[c], l + ST_DL[c]).is_some());
            let Some(best) = best else {
                return false;
            };
            x += ST_DX[best];
            y += ST_DY[best];
            l += ST_DL[best];
            head = best;
            hint = rail_at(map, x, y, l).unwrap_or(0);
            if (x, y, l) == (sx, sy, sl) {
                return true;
            }
            guard += 1;
            if guard > MAX_NODES {
                return false;
            }
        }
    }

    /// Direction from node `k` to node `k + 1` of a loop, each axis -1..1.
    fn node_dir(&self, lp: usize, k: i64) -> (i64, i64) {
        let Loop { first, len, .. } = self.loops[lp];
        let n = len as i64;
        let a = self.nodes[first + k.rem_euclid(n) as usize];
        let b = self.nodes[first + (k + 1).rem_euclid(n) as usize];
        let d = |p: u8, q: u8| (i64::from(q) - i64::from(p)).clamp(-1, 1);
        (d(a.x, b.x), d(a.y, b.y))
    }

    fn centre(&self, nd: usize) -> (i64, i64) {
        let n = self.nodes[nd];
        (
            (i64::from(n.x) * BLK + 16) << FP,
            (i64::from(n.y) * BLK + 16) << FP,
        )
    }

    /// The loop's straights and quarter circles. False if it has no corner.
    fn build_path(&mut self, lp: usize) -> bool {
        let Loop {
            first: f, len: n, ..
        } = self.loops[lp];
        let mut cn: Vec<usize> = Vec::new();
        let mut cr: Vec<i64> = Vec::new();
        for i in 0..n {
            if cn.len() >= MAX_NODES {
                break;
            }
            if self.node_dir(lp, i as i64 - 1) != self.node_dir(lp, i as i64) {
                let hp = self.nodes[f + (i + n - 1) % n].hint;
                let hn = self.nodes[f + (i + 1) % n].hint;
                cn.push(i);
                cr.push(if matches!(hp, 4 | 5) || matches!(hn, 4 | 5) {
                    48
                } else {
                    16
                });
            }
        }
        let nc = cn.len();
        if nc == 0 {
            return false;
        }
        let gap = |k: usize, k2: usize| -> i64 {
            if nc == 1 {
                n as i64 * BLK
            } else {
                ((cn[k2] + n - cn[k]) % n) as i64 * BLK
            }
        };
        for k in 0..nc {
            let k2 = (k + 1) % nc;
            let d = gap(k, k2);
            if cr[k] + cr[k2] > d {
                if cr[k] > d / 2 {
                    cr[k] = d / 2;
                }
                if cr[k2] > d / 2 {
                    cr[k2] = d / 2;
                }
            }
        }
        self.loops[lp].piece0 = self.pieces.len();
        let mut s0 = 0i64;
        for k in 0..nc {
            if self.pieces.len() + 2 > MAX_NODES {
                break;
            }
            let k2 = (k + 1) % nc;
            let i2 = cn[k2];
            let (cx, cy) = self.centre(f + cn[k]);
            let dist = gap(k, k2);
            let (dx, dy) = self.node_dir(lp, cn[k] as i64);
            let line = Piece {
                x0: cx + ((dx * cr[k]) << FP),
                y0: cy + ((dy * cr[k]) << FP),
                s0,
                len: (dist - cr[k] - cr[k2]) << FP,
                d1: (dx, dy),
                d2: (0, 0),
                r: 0,
                node: cn[k],
                r_from: cr[k],
                arc: false,
            };
            self.pieces.push(line);
            let between = (cn[k2] + n - cn[k]) % n;
            let mut i = 1;
            while i < between || (nc == 1 && i < n) {
                let nd = (cn[k] + i) % n;
                self.node_s[f + nd] = s0 + ((i as i64 * BLK - cr[k]) << FP);
                i += 1;
            }
            s0 += line.len;
            let d1 = self.node_dir(lp, i2 as i64 - 1);
            let d2 = self.node_dir(lp, i2 as i64);
            let (ax, ay) = self.centre(f + i2);
            let mut len = ((cr[k2] * 25736) >> 14) << FP;
            if len <= 0 {
                len = 1 << FP;
            }
            self.pieces.push(Piece {
                x0: ax,
                y0: ay,
                s0,
                len,
                d1,
                d2,
                r: cr[k2],
                node: i2,
                r_from: 0,
                arc: true,
            });
            self.node_s[f + i2] = s0 + len / 2;
            s0 += len;
        }
        self.loops[lp].pieces = self.pieces.len() - self.loops[lp].piece0;
        self.loops[lp].total = s0;
        true
    }

    /// The point `s` along a loop: x, y, layer, and the node it is in.
    fn point_at(&self, lp: usize, s: i64) -> (i64, i64, i32, usize) {
        let l = self.loops[lp];
        let s = s.rem_euclid(l.total.max(1));
        let (mut lo, mut hi) = (l.piece0, l.piece0 + l.pieces - 1);
        while lo < hi {
            let mid = (lo + hi + 1) >> 1;
            if self.pieces[mid].s0 <= s {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        let p = self.pieces[lo];
        let u = s - p.s0;
        let n = l.len;
        let (x, y, nd) = if !p.arc {
            let nd = (p.node + (((u >> FP) + p.r_from + 16) / BLK) as usize) % n;
            (p.x0 + p.d1.0 * u, p.y0 + p.d1.1 * u, nd)
        } else {
            let ph = ((u >> 8) * 64) / (p.len >> 8).max(1);
            let phi = ph.min(64) as i32;
            let (c, sn) = (i64::from(cos(phi)), i64::from(sin(phi)));
            let r = p.r;
            let ox = p.x0 + (((p.d2.0 - p.d1.0) * r) << FP);
            let oy = p.y0 + (((p.d2.1 - p.d1.1) * r) << FP);
            let x = ox + ((-p.d2.0 * r * c + p.d1.0 * r * sn) << 2);
            let y = oy + ((-p.d2.1 * r * c + p.d1.1 * r * sn) << 2);
            (x, y, p.node)
        };
        (
            x,
            y,
            i32::from(self.nodes[l.first + nd].layer),
            l.first + nd,
        )
    }

    /// Carriage `j`'s two bogies and its layer.
    fn bogies(&self, t: &Train, j: usize) -> ((i64, i64), (i64, i64), i32) {
        let s = t.s - ((j as i64 * CAR_PITCH) << FP);
        let (fx, fy, layer, _) = self.point_at(t.loop_, s);
        let (bx, by, _, _) = self.point_at(t.loop_, s - (BOGIE_GAP << FP));
        ((fx, fy), (bx, by), layer)
    }

    /// Carriage `j` of train `t`: between its bogies, at their angle.
    pub fn carriage(&self, t: usize, j: usize) -> Carriage {
        let train = &self.trains[t];
        let (front, back, layer) = self.bogies(train, j);
        let body = (
            front.0 + ((back.0 - front.0) >> 1),
            front.1 + ((back.1 - front.1) >> 1),
        );
        let angle = if front == back {
            0
        } else {
            (dir16(
                ((front.0 - back.0) >> 8) as i32,
                ((front.1 - back.1) >> 8) as i32,
            ) >> 16)
                & 255
        };
        Carriage {
            body,
            front,
            back,
            layer,
            angle,
            first: j == 0,
            last: j + 1 == train.carriages(),
        }
    }

    fn station_index(&self, node: usize) -> Option<usize> {
        self.stations.iter().position(|s| s.node == node)
    }

    fn next_station(&self, st: usize) -> usize {
        let lp = self.stations[st].loop_;
        let k = st + 1;
        if k < self.stations.len() && self.stations[k].loop_ == lp {
            return k;
        }
        self.stations
            .iter()
            .position(|s| s.loop_ == lp)
            .unwrap_or(st)
    }

    /// One tick. `player` is where the player is on foot, if he is.
    /// `cars` are the parked cars' positions and layers.
    pub fn tick(
        &mut self,
        player: Option<(i64, i64, i32)>,
        cars: &[(i64, i64, i32)],
        peds: &[(i64, i64, i32)],
    ) -> Vec<TrainEvent> {
        let logic = self.half.step();
        let mut events = Vec::new();
        for i in 0..self.trains.len() {
            let braking = self.trains[i].state == state::BRAKE;
            self.one(i, logic, player, cars, peds, &mut events);
            if braking && self.trains[i].state == state::STATION {
                events.push(TrainEvent::Stopped(i));
            }
        }
        events
    }

    fn one(
        &mut self,
        ti: usize,
        logic: bool,
        player: Option<(i64, i64, i32)>,
        cars: &[(i64, i64, i32)],
        peds: &[(i64, i64, i32)],
        events: &mut Vec<TrainEvent>,
    ) {
        let t = self.trains[ti];
        if t.tram || t.wrecked {
            return;
        }
        if logic {
            self.logic(ti);
        }
        let t = &mut self.trains[ti];
        // speed / 10 original px a frame is half that here, over two ticks.
        let step = i64::from(t.speed / 10) << (FP - 2);
        if step == 0 {
            return;
        }
        let total = self.loops[t.loop_].total;
        t.s = (t.s + if t.reversed { -step } else { step }).rem_euclid(total);
        let (s, lp) = (t.s, t.loop_);
        let node = self.point_at(lp, s).3;
        let t = self.trains[ti];
        if node != t.lead_node {
            self.trains[ti].lead_node = node;
            // The lead bogie on a new block: a station stops it.
            if !t.driven
                && t.state == state::GO
                && self.station_index(node).is_some()
                && node != t.last_st_node
            {
                let t = &mut self.trains[ti];
                t.last_st_node = node;
                t.after_brake = state::STATION;
                t.state = state::BRAKE;
                t.sub = 5;
            }
        }
        if logic {
            self.crush(ti, player, cars, peds, events);
        }
    }

    fn logic(&mut self, ti: usize) {
        let mut t = self.trains[ti];
        match t.state {
            state::STATION => match t.sub {
                5 => {
                    t.door = 1;
                    t.sub = 1;
                }
                1 => {
                    if t.door < 7 {
                        t.door += 1;
                    } else {
                        t.sub = 3;
                        t.sub_first = true;
                    }
                }
                3 => {
                    if t.sub_first {
                        t.drop = (self.rng.next16() % 16) as i16;
                        t.wait = 100;
                        t.sub_first = false;
                    } else if t.drop == 0 || {
                        t.wait -= 1;
                        t.wait <= 0
                    } {
                        t.sub = 2;
                        t.sub_first = true;
                    } else if t.wait % 5 == 0 {
                        // A passenger gets off (no ped is made yet).
                        t.drop -= 1;
                    }
                }
                2 => {
                    if t.sub_first {
                        t.wait = 100;
                        t.sub_first = false;
                    } else {
                        t.wait -= 1;
                        if t.wait <= 0 {
                            t.sub = 4;
                        }
                    }
                }
                4 => {
                    if t.door >= 2 {
                        t.door -= 1;
                    } else {
                        t.door = 8;
                        t.sub = 0;
                    }
                }
                _ => {
                    t.state = state::WAIT_TO_LEAVE;
                    t.next_st = self.next_station(t.cur_st);
                    t.sub = 5;
                }
            },
            state::GO => {
                if t.speed < t.max {
                    t.speed += 1;
                }
                t.speed = t.speed.min(t.max);
            }
            state::BRAKE => {
                t.speed -= 1;
                if t.speed <= 0 {
                    t.speed = 0;
                    t.state = t.after_brake;
                }
            }
            state::WAIT_TO_LEAVE => {
                // The block signal: go when the next station is clear.
                if t.next_st == t.cur_st || !self.stations[t.next_st].busy {
                    self.stations[t.cur_st].busy = false;
                    self.stations[t.next_st].busy = true;
                    t.cur_st = t.next_st;
                    t.state = state::GO;
                }
            }
            state::RESUME => {
                t.state = state::STATION;
                t.sub = 5;
            }
            _ => {}
        }
        if t.state != state::STATION && t.door < 8 {
            t.door -= 1;
            if t.door < 2 {
                t.door = 8;
            }
        }
        self.trains[ti] = t;
    }

    fn crush(
        &mut self,
        ti: usize,
        player: Option<(i64, i64, i32)>,
        cars: &[(i64, i64, i32)],
        peds: &[(i64, i64, i32)],
        events: &mut Vec<TrainEvent>,
    ) {
        let near = |a: (i64, i64), b: (i64, i64), r: i64| {
            ((a.0 - b.0) >> FP).abs() < r && ((a.1 - b.1) >> FP).abs() < r
        };
        let t = self.trains[ti];
        for j in 0..t.carriages() {
            let (front, back, l) = self.bogies(&t, j);
            for p in [front, back] {
                // Another train's bogie: both are wrecked.
                for k in 0..self.trains.len() {
                    let other = self.trains[k];
                    if k == ti || other.wrecked {
                        continue;
                    }
                    for jj in 0..CARRIAGES {
                        let (o, _, ol) = self.bogies(&other, jj);
                        if ol == l && near(o, p, 8) {
                            self.wreck(ti, events);
                            self.wreck(k, events);
                            return;
                        }
                    }
                }
                // A car: dented, and the train halves its speed.
                for (i, &(cx, cy, cl)) in cars.iter().enumerate() {
                    if cl == l && ((cx - p.0) >> FP).abs() <= 12 && ((cy - p.1) >> FP).abs() <= 12 {
                        events.push(TrainEvent::Crushed(i));
                        self.trains[ti].speed /= 2;
                    }
                }
                // People, and the player on foot, above 1 px a frame.
                if self.trains[ti].speed / 10 > 1 {
                    for (k, &(qx, qy, ql)) in peds.iter().enumerate() {
                        if ql == l && near((qx, qy), p, 10) {
                            events.push(TrainEvent::KilledPed(k));
                        }
                    }
                    if let Some((px, py, pl)) = player
                        && pl == l
                        && near((px, py), p, 10)
                    {
                        events.push(TrainEvent::RanOver);
                    }
                }
            }
        }
    }

    /// Wreck train `ti`: it stops for good, and the world turns its
    /// carriages into wrecks (`TrainEvent::Wrecked`).
    pub fn wreck(&mut self, ti: usize, events: &mut Vec<TrainEvent>) {
        if self.trains[ti].wrecked {
            return;
        }
        let cars = (0..self.trains[ti].carriages())
            .map(|j| {
                let c = self.carriage(ti, j);
                (c.body.0, c.body.1, c.angle, c.layer)
            })
            .collect();
        let t = &mut self.trains[ti];
        t.wrecked = true;
        t.speed = 0;
        t.state = state::STOPPED;
        t.driven = false;
        if self.rider == Some(ti) {
            self.rider = None;
        }
        events.push(TrainEvent::Wrecked(ti, cars));
    }

    /// A stopped train with a carriage within `reach` px of `(x, y)` on
    /// `layer`.
    pub fn board(&self, x: i64, y: i64, layer: i32, reach: i64) -> Option<usize> {
        (0..self.trains.len()).find(|&i| {
            let t = &self.trains[i];
            !t.wrecked
                && t.speed == 0
                && (0..t.carriages()).any(|j| {
                    let c = self.carriage(i, j);
                    c.layer == layer
                        && ((c.body.0 - x) >> FP).abs() <= reach
                        && ((c.body.1 - y) >> FP).abs() <= reach
                })
        })
    }

    /// Where the rider stands, carried by train `t`: its second carriage
    /// (a tram's only one).
    pub fn rider_spot(&self, t: usize) -> (i64, i64, i32) {
        let j = if self.trains[t].tram { 0 } else { 1 };
        let c = self.carriage(t, j);
        (c.body.0, c.body.1, c.layer)
    }

    /// May the rider get off train `t` now? Where he stands if so: 14 px
    /// to the side with pavement 30 px out, at a standstill.
    pub fn get_off(&self, map: &SimMap, t: usize) -> Option<(i64, i64, i32)> {
        let train = &self.trains[t];
        if train.speed != 0 {
            return None;
        }
        let c = self.carriage(t, if train.tram { 0 } else { 1 });
        for side in [64, 192] {
            let a = (c.angle + side) & 255;
            let (sx, sy) = (i64::from(sin(a)), -i64::from(cos(a)));
            let (tx, ty) = (c.body.0 + sx * 30 * 4, c.body.1 + sy * 30 * 4);
            if map.ground_type((tx >> 21) as i32, (ty >> 21) as i32, c.layer) == ground::PAVEMENT {
                return Some((c.body.0 + sx * 14 * 4, c.body.1 + sy * 14 * 4, c.layer));
            }
        }
        None
    }

    /// FIRE aboard: take the controls (true), or hand them back (false).
    pub fn hijack(&mut self, ti: usize) -> bool {
        let t = &mut self.trains[ti];
        if t.wrecked || t.tram {
            return false;
        }
        if !t.driven {
            t.driven = true;
            t.max = 80;
            if matches!(t.state, state::STATION | state::WAIT_TO_LEAVE) {
                if t.state == state::STATION && t.door < 8 {
                    t.door = 8;
                }
                t.state = state::STOPPED;
                t.speed = 0;
            } else {
                t.after_brake = state::STOPPED;
                t.state = state::BRAKE;
            }
            return true;
        }
        t.driven = false;
        t.max = 60;
        if t.speed == 0 {
            t.state = state::RESUME;
        } else {
            t.after_brake = state::RESUME;
            t.state = state::BRAKE;
        }
        false
    }

    /// The rider's keys while he drives train `ti`: up forward, down back;
    /// the other key brakes first.
    pub fn drive(&mut self, ti: usize, up: bool, down: bool) {
        let t = &mut self.trains[ti];
        if !t.driven || t.wrecked {
            return;
        }
        let want_reverse = match (up, down) {
            (true, _) => false,
            (false, true) => true,
            _ => return,
        };
        if t.reversed == want_reverse {
            t.state = state::GO;
        } else if t.speed == 0 {
            t.reversed = want_reverse;
            t.state = state::GO;
        } else if t.state != state::BRAKE {
            t.after_brake = state::STOPPED;
            t.state = state::BRAKE;
        }
    }
}

impl SimHash for Trains {
    fn sim_hash(&self, h: &mut StateHasher) {
        h.u32(self.trains.len() as u32);
        for t in &self.trains {
            for v in [t.state, t.after_brake, t.sub, t.door] {
                h.u8(v);
            }
            for v in [t.tram, t.sub_first, t.driven, t.reversed, t.wrecked] {
                h.bool(v);
            }
            for v in [t.speed, t.max, t.wait, t.drop] {
                h.i32(v.into());
            }
            for v in [t.cur_st, t.next_st, t.last_st_node, t.lead_node, t.loop_] {
                h.u32(v as u32);
            }
            h.bytes(&t.s.to_le_bytes());
        }
        for s in &self.stations {
            h.bool(s.busy);
        }
        h.u32(self.rng.seed);
        h.bool(self.half.phase());
        h.u32(self.rider.map_or(u32::MAX, |r| r as u32));
    }
}

/// One line of the per-tick train trace, for the oracle.
pub fn trace_line(tick: u32, ts: &Trains) -> String {
    let mut s = format!("r {tick}");
    for t in &ts.trains {
        s.push_str(&format!(
            " | {} {} {} {} {} {}",
            t.s, t.speed, t.state, t.sub, t.door, t.lead_node
        ));
    }
    s
}
