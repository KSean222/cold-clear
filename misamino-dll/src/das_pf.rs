use libtetris::*;
use std::collections::{ HashMap, VecDeque };

const BOARD_MOVES: [DasPieceMovement; 7] = [
    DasPieceMovement::DasLeft,
    DasPieceMovement::DasRight,
    DasPieceMovement::SonicDrop,
    DasPieceMovement::Left,
    DasPieceMovement::Right,
    DasPieceMovement::Ccw,
    DasPieceMovement::Cw
];

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum DasPieceMovement {
    DasLeft,
    DasRight,
    SonicDrop,
    Left,
    Right,
    Ccw,
    Cw
}

impl From<PieceMovement> for DasPieceMovement {
    fn from(mv: PieceMovement) -> DasPieceMovement {
        match mv {
            PieceMovement::SonicDrop => DasPieceMovement::SonicDrop,
            PieceMovement::Left => DasPieceMovement::Left,
            PieceMovement::Right => DasPieceMovement::Right,
            PieceMovement::Ccw => DasPieceMovement::Ccw,
            PieceMovement::Cw => DasPieceMovement::Cw
        }
    }
}

pub fn find_das_path(field: [[bool; 10]; 40], start: FallingPiece, target: FallingPiece) -> Option<Vec<DasPieceMovement>> {
    let mut board = Board::<u16>::new();
    board.set_field(field);
    let mut queue = VecDeque::new();
    let mut nodes = HashMap::new();
    queue.push_back(start);
    nodes.insert(start, (start, DasPieceMovement::SonicDrop));
    while let Some(parent_pos) = queue.pop_front() {
        for &mv in &BOARD_MOVES {
            let mut child_pos = parent_pos;
            match mv {
                DasPieceMovement::DasLeft => while child_pos.shift(&board, -1, 0) {},
                DasPieceMovement::DasRight => while child_pos.shift(&board, 1, 0) {},
                DasPieceMovement::SonicDrop => while child_pos.shift(&board, 0, -1) {},
                DasPieceMovement::Left => { child_pos.shift(&board, -1, 0); },
                DasPieceMovement::Right => { child_pos.shift(&board, 1, 0); },
                DasPieceMovement::Ccw => { child_pos.ccw(&board); },
                DasPieceMovement::Cw => { child_pos.cw(&board); }
            }
            if child_pos != parent_pos {
                nodes.entry(child_pos).or_insert_with(|| {
                    queue.push_back(child_pos);
                    (parent_pos, mv)
                });
                if child_pos == target {
                    let mut moves = Vec::new();
                    let mut pos = child_pos;
                    let mut skip = true;
                    loop {
                        let &(parent_pos, mv) = nodes.get(&pos).unwrap();
                        if pos == parent_pos {
                            break;
                        }
                        if mv != DasPieceMovement::SonicDrop {
                            skip = false;
                        }
                        if !skip {
                            moves.push(mv);
                        }
                        pos = parent_pos;
                    }
                    return Some(moves.into_iter().rev().collect());
                }
            }
        }
    }
    None
}