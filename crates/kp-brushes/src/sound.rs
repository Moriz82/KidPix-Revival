//! Tool sound rules (RE_BRUSHES §2.4): `v18` (mouse down), `v20` (every tick), `v19`
//! (mouse up) and the per-event special effect.

use crate::assets::{BrushAssets, WACKY_SOUNDS_MHK};
use crate::config::SoundParams;
use std::collections::HashMap;

/// A request to the engine's audio mixer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoundEvent {
    /// Archive path relative to the game root (e.g. `Data/Tools/WackyBrushSounds.MHK`).
    pub archive: &'static str,
    /// `tWAV` id.
    pub id: u16,
    /// Play it looping (the tWAV's own `loopCount == 65535`). Ignored for `stop`.
    pub looping: bool,
    /// `true` = stop this sound if it is playing; `false` = (re)start it.
    pub stop: bool,
}

impl SoundEvent {
    pub fn play(archive: &'static str, id: u16, looping: bool) -> SoundEvent {
        SoundEvent { archive, id, looping, stop: false }
    }
    pub fn stop(archive: &'static str, id: u16) -> SoundEvent {
        SoundEvent { archive, id, looping: false, stop: true }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dir {
    North,
    East,
    South,
    West,
}

/// Per-tool sound state; lives as long as the tool (survives strokes, so a stroke can
/// stop the previous stroke's `stop` sound as the original does).
#[derive(Clone, Debug)]
pub struct SoundTracker {
    pub params: SoundParams,
    /// id → time (ms, tool clock) at which it stops playing; `u64::MAX` = looping.
    playing: HashMap<u16, u64>,
    last_pt: Option<(i32, i32)>,
    last_move: u64,
    dir: Option<Dir>,
}

/// Mouse still for longer than this stops a non-continuous loop (§2.4) [O].
pub const STILL_MS: u64 = 50;

impl SoundTracker {
    pub fn new(params: SoundParams) -> SoundTracker {
        SoundTracker { params, playing: HashMap::new(), last_pt: None, last_move: 0, dir: None }
    }

    fn is_playing(&self, id: u16, now: u64) -> bool {
        id != 0 && self.playing.get(&id).is_some_and(|&t| now < t)
    }

    fn play(&mut self, id: u16, now: u64, assets: &BrushAssets, assume_loop: bool, out: &mut Vec<SoundEvent>) {
        if id == 0 {
            return;
        }
        let info = assets.sound_info(id);
        // Without the sound file assume loop sounds loop and others are short.
        let looping = info.map(|i| i.looping).unwrap_or(assume_loop);
        let until = if looping { u64::MAX } else { now + info.map(|i| i.duration_ms as u64).unwrap_or(0) };
        self.playing.insert(id, until);
        out.push(SoundEvent::play(WACKY_SOUNDS_MHK, id, looping));
    }

    fn stop(&mut self, id: u16, now: u64, out: &mut Vec<SoundEvent>) {
        if id != 0 && self.is_playing(id, now) {
            self.playing.remove(&id);
            out.push(SoundEvent::stop(WACKY_SOUNDS_MHK, id));
        }
    }

    /// `v18`: mouse down.
    pub fn mouse_down(&mut self, p: (i32, i32), now: u64, assets: &BrushAssets, out: &mut Vec<SoundEvent>) {
        self.last_pt = Some(p);
        self.last_move = now;
        self.dir = None;
        if self.params.start != 0 {
            let (lp, fx, st) = (self.params.loop_, self.params.fx, self.params.start);
            self.stop(lp, now, out);
            self.stop(fx, now, out);
            self.play(st, now, assets, false, out);
        }
    }

    /// `v20(point)`: every tick while the button is held.
    pub fn tick(&mut self, p: (i32, i32), now: u64, assets: &BrushAssets, out: &mut Vec<SoundEvent>) {
        let sp = self.params;
        self.stop(sp.stop, now, out);
        let prev = self.last_pt.unwrap_or(p);
        let moved = prev != p;
        if moved {
            self.last_move = now;
        }
        if sp.loop_ != 0 && !self.is_playing(sp.start, now) {
            let want = sp.continuous || moved || now.saturating_sub(self.last_move) <= STILL_MS;
            if want {
                if !self.is_playing(sp.loop_, now) {
                    self.play(sp.loop_, now, assets, true, out);
                }
            } else {
                self.stop(sp.loop_, now, out);
            }
        }
        let (dx, dy) = (p.0 - prev.0, p.1 - prev.1);
        let dir = if dx.abs() > dy.abs() {
            if dx > 0 {
                Some(Dir::East)
            } else {
                Some(Dir::West)
            }
        } else if dy > 0 {
            Some(Dir::North) // sic: "North" fires when moving down the screen [O]
        } else if dy < 0 {
            Some(Dir::South)
        } else {
            None
        };
        if let Some(d) = dir {
            if self.dir != Some(d) {
                self.dir = Some(d);
                let id = self.dir_id(d);
                if id != 0 {
                    for o in [Dir::North, Dir::East, Dir::South, Dir::West] {
                        let oid = self.dir_id(o);
                        if o != d && oid != id {
                            self.stop(oid, now, out);
                        }
                    }
                    self.play(id, now, assets, false, out);
                }
            }
        }
        self.last_pt = Some(p);
    }

    fn dir_id(&self, d: Dir) -> u16 {
        match d {
            Dir::North => self.params.north,
            Dir::East => self.params.east,
            Dir::South => self.params.south,
            Dir::West => self.params.west,
        }
    }

    /// `v19`: mouse up.
    pub fn mouse_up(&mut self, now: u64, assets: &BrushAssets, out: &mut Vec<SoundEvent>) {
        let sp = self.params;
        if !sp.lingers {
            for id in [sp.north, sp.east, sp.south, sp.west, sp.start, sp.loop_, sp.fx] {
                self.stop(id, now, out);
            }
        }
        self.play(sp.stop, now, assets, false, out);
        self.dir = None;
    }

    /// The brush's special effect (each brick, drip, letter…), only if not already playing.
    pub fn fx(&mut self, now: u64, assets: &BrushAssets, out: &mut Vec<SoundEvent>) {
        let fx = self.params.fx;
        if fx != 0 && !self.is_playing(fx, now) {
            self.play(fx, now, assets, false, out);
        }
    }

    /// Stop everything (tool switch).
    pub fn stop_all(&mut self, now: u64, out: &mut Vec<SoundEvent>) {
        let ids: Vec<u16> = self.playing.keys().copied().collect();
        for id in ids {
            self.stop(id, now, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loop_follows_movement() {
        let a = BrushAssets::empty();
        let mut t = SoundTracker::new(SoundParams { loop_: 7000, stop: 7001, north: 7002, east: 7003, ..Default::default() });
        let mut ev = Vec::new();
        t.mouse_down((0, 0), 0, &a, &mut ev);
        assert!(ev.is_empty());
        t.tick((5, 0), 10, &a, &mut ev);
        assert!(ev.contains(&SoundEvent::play(WACKY_SOUNDS_MHK, 7000, true)));
        assert!(ev.iter().any(|e| e.id == 7003 && !e.stop));
        ev.clear();
        t.tick((5, 0), 40, &a, &mut ev);
        assert!(ev.is_empty());
        t.tick((5, 0), 100, &a, &mut ev);
        assert_eq!(ev, vec![SoundEvent::stop(WACKY_SOUNDS_MHK, 7000)]);
        ev.clear();
        t.tick((5, 9), 110, &a, &mut ev);
        assert!(ev.iter().any(|e| e.id == 7002 && !e.stop)); // moving down = "North"
        ev.clear();
        t.mouse_up(120, &a, &mut ev);
        assert!(ev.contains(&SoundEvent::stop(WACKY_SOUNDS_MHK, 7000)));
        assert!(ev.iter().any(|e| e.id == 7001 && !e.stop));
    }
}
