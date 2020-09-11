use libtetris::*;
use battle::{ Event, PieceMoveExecutor };
use game_util::glutin::VirtualKeyCode;
use gilrs::Gamepad;
use std::collections::HashSet;
use crate::input::InputSource;

pub fn cc_piece_to_mb(piece: Piece) -> minotetris::PieceType {
    match piece {
        Piece::I => minotetris::PieceType::I,
        Piece::O => minotetris::PieceType::O,
        Piece::T => minotetris::PieceType::T,
        Piece::L => minotetris::PieceType::L,
        Piece::J => minotetris::PieceType::J,
        Piece::S => minotetris::PieceType::S,
        Piece::Z => minotetris::PieceType::Z,
    }
}

pub fn cc_board_to_mb(board: &Board<impl Row>) -> minotetris::Board {
    let mut rows = [0u16; 40];
    for (src, dest) in board.get_field().iter().rev().zip(rows.iter_mut()) {
        for (x, &cell) in src.iter().enumerate() {
            let cell = if cell {
                minotetris::CellType::Garbage
            } else {
                minotetris::CellType::Empty
            };
            minotetris::Row::set(dest, x, cell);
        }
    }
    let mut minotetris_board = minotetris::Board::new();
    minotetris_board.set_field(rows);
    if let Some(piece) = board.hold_piece {
        minotetris_board.hold = Some(cc_piece_to_mb(piece));
    }
    minotetris_board
}

pub struct MinoBotInput {
    interface: minobot::BotHandle,
    executing: Option<(FallingPiece, PieceMoveExecutor)>,
    controller: Controller,
    speed_limit: u32
}

impl MinoBotInput {
    pub fn new(interface: minobot::BotHandle, speed_limit: u32) -> Self {
        interface.begin_thinking();
        MinoBotInput {
            interface,
            executing: None,
            controller: Default::default(),
            speed_limit
        }
    }
}

impl InputSource for MinoBotInput {
    fn controller(&self, _keys: &HashSet<VirtualKeyCode>, _gamepad: Option<Gamepad>) -> Controller {
        self.controller
    }

    fn update(
        &mut self, board: &Board<ColoredRow>, events: &[Event], _incoming: u32
    ) -> Option<cold_clear::Info> {
        for event in events {
            match event {
                Event::PieceSpawned { new_in_queue } => {
                    self.interface.add_piece(cc_piece_to_mb(*new_in_queue));
                }
                Event::GarbageAdded(_) => {
                    let queue = board.next_queue().map(cc_piece_to_mb).collect();
                    self.interface.reset(cc_board_to_mb(board), queue);
                }
                _ => {}
            }
        }
        if self.executing.is_none() {
            if let Some(mv) = self.interface.next_move() {
                self.interface.begin_thinking();
                let expected = FallingPiece {
                    kind: PieceState(match mv.mv.kind {
                        minotetris::PieceType::I => Piece::I,
                        minotetris::PieceType::O => Piece::O,
                        minotetris::PieceType::T => Piece::T,
                        minotetris::PieceType::L => Piece::L,
                        minotetris::PieceType::J => Piece::J,
                        minotetris::PieceType::S => Piece::S,
                        minotetris::PieceType::Z => Piece::Z
                    }, match mv.mv.r {
                        0 => RotationState::North,
                        1 => RotationState::East,
                        2 => RotationState::South,
                        3 => RotationState::West,
                        _ => unreachable!()
                    }),
                    x: mv.mv.x,
                    y: 39 - mv.mv.y,
                    tspin: match mv.mv.tspin {
                        minotetris::TspinType::None => TspinStatus::None,
                        minotetris::TspinType::Mini => TspinStatus::Mini,
                        minotetris::TspinType::Full => TspinStatus::Full
                    }
                };
                let inputs = mv.path
                    .into_iter()
                    .map(|mv| match mv {
                        minobot::pathfinder::PathfinderMove::Left => PieceMovement::Left,
                        minobot::pathfinder::PathfinderMove::Right => PieceMovement::Right,
                        minobot::pathfinder::PathfinderMove::RotLeft => PieceMovement::Ccw,
                        minobot::pathfinder::PathfinderMove::RotRight => PieceMovement::Cw,
                        minobot::pathfinder::PathfinderMove::SonicDrop => PieceMovement::SonicDrop,
                    })
                    .collect();
                self.executing = Some((expected, PieceMoveExecutor::new(mv.uses_hold, inputs, self.speed_limit)));
            }
        }
        if let Some((expected, ref mut executor)) = self.executing {
            if let Some(loc) = executor.update(&mut self.controller, board, events) {
                if loc != expected {
                    let queue = board.next_queue().map(cc_piece_to_mb).collect();
                    self.interface.reset(cc_board_to_mb(board), queue);
                }
                self.executing = None;
            }
        }
        None
    }
}
