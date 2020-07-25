use rand_pcg::Pcg64Mcg;
use rand::prelude::*;
use std::collections::VecDeque;
use serde::{ Serialize, Deserialize };
use crate::{ Game, GameConfig, Event };
use libtetris::Controller;

pub struct Battle {
    pub player_1: Game,
    pub player_2: Game,
    p1_piece_quota: u32,
    p2_piece_quota: u32,
    p1_pieces_left: u32,
    p2_pieces_left: u32,
    p1_rng: Pcg64Mcg,
    p2_rng: Pcg64Mcg,
    garbage_rng: Pcg64Mcg,
    pub time: u32,
    pub replay: Replay
}

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum BattleMode {
    Realtime,
    TurnBased(u32, u32)
}

impl Default for BattleMode {
    fn default() -> Self {
        BattleMode::Realtime
    }
}

impl Battle {
    pub fn new(
        p1_config: GameConfig, p2_config: GameConfig,
        p1_seed: <Pcg64Mcg as SeedableRng>::Seed,
        p2_seed: <Pcg64Mcg as SeedableRng>::Seed,
        garbage_seed: <Pcg64Mcg as SeedableRng>::Seed,
        mode: BattleMode
    ) -> Self {
        let mut p1_rng = Pcg64Mcg::from_seed(p1_seed);
        let mut p2_rng = Pcg64Mcg::from_seed(p2_seed);
        let garbage_rng = Pcg64Mcg::from_seed(garbage_seed);
        let player_1 = Game::new(p1_config, &mut p1_rng);
        let player_2 = Game::new(p2_config, &mut p2_rng);
        let (p1_piece_quota, p2_piece_quota) = match mode {
            BattleMode::Realtime => (0, 0),
            BattleMode::TurnBased(p1, p2) => (p1, p2)
        };
        Battle {
            replay: Replay {
                p1_name: String::new(), p2_name: String::new(),
                p1_config, p2_config, p1_seed, p2_seed, garbage_seed,
                updates: VecDeque::new(),
                mode
            },
            player_1, player_2,
            p1_piece_quota,
            p2_piece_quota,
            p1_pieces_left: match mode {
                BattleMode::Realtime => u32::MAX,
                BattleMode::TurnBased(_, _) => p1_piece_quota + 1
            },
            p2_pieces_left: match mode {
                BattleMode::Realtime => u32::MAX,
                BattleMode::TurnBased(_, _) => 1
            },
            p1_rng, p2_rng, garbage_rng,
            time: 0,
        }
    }

    pub fn update(&mut self, p1: Controller, p2: Controller) -> BattleUpdate {
        self.time += 1;

        self.replay.updates.push_back((p1, p2));

        let p1_active = self.p1_pieces_left > 0;
        let p2_active = self.p2_pieces_left > 0;

        let p1_events = if p1_active {
            self.player_1.update(p1, &mut self.p1_rng, &mut self.garbage_rng)
        } else {
            Vec::new()
        };
        let p2_events = if p2_active {
            self.player_2.update(p2, &mut self.p2_rng, &mut self.garbage_rng)
        } else {
            Vec::new()
        };

        for event in &p1_events {
            match event {
                &Event::GarbageSent(amt) => self.player_2.garbage_queue += amt,
                Event::PieceHeld { prev, .. } => if prev.is_none() {
                    self.p1_pieces_left += 1;
                }
                Event::FrameBeforePieceSpawns { .. } => self.p1_pieces_left -= 1,
                _ => {}
            }
        }
        for event in &p2_events {
            match event {
                &Event::GarbageSent(amt) => self.player_1.garbage_queue += amt,
                Event::PieceHeld { prev, .. } => if prev.is_none() {
                    self.p2_pieces_left += 1;
                }
                Event::FrameBeforePieceSpawns { .. } => self.p2_pieces_left -= 1,
                _ => {}
            }
        }

        if p1_active && self.p1_pieces_left == 0 {
            self.p2_pieces_left = self.p2_piece_quota;
        }
        if p2_active && self.p2_pieces_left == 0 {
            self.p1_pieces_left = self.p1_piece_quota;
        }

        BattleUpdate {
            player_1: PlayerUpdate {
                events: p1_events,
                garbage_queue: self.player_1.garbage_queue
            },
            player_2: PlayerUpdate {
                events: p2_events,
                garbage_queue: self.player_2.garbage_queue
            },
            time: self.time
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BattleUpdate {
    pub player_1: PlayerUpdate,
    pub player_2: PlayerUpdate,
    pub time: u32
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PlayerUpdate {
    pub events: Vec<Event>,
    pub garbage_queue: u32
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Replay {
    pub p1_name: String,
    pub p2_name: String,
    pub p1_seed: <Pcg64Mcg as SeedableRng>::Seed,
    pub p2_seed: <Pcg64Mcg as SeedableRng>::Seed,
    pub garbage_seed: <Pcg64Mcg as SeedableRng>::Seed,
    pub p1_config: GameConfig,
    pub p2_config: GameConfig,
    pub mode: BattleMode,
    pub updates: VecDeque<(Controller, Controller)>
}