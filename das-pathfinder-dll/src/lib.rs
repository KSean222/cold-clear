use std::os::raw::*;
use libtetris::{ Board, Piece, PieceState, RotationState, FallingPiece, TspinStatus };
use std::collections::{ HashMap, VecDeque };

const BOARD_MOVES: [BoardMove; 7] = [
    BoardMove::DasLeft,
    BoardMove::DasRight,
    BoardMove::SonicDrop,
    BoardMove::Left,
    BoardMove::Right,
    BoardMove::RotLeft,
    BoardMove::RotRight
];

#[derive(Copy, Clone, Eq, PartialEq)]
enum BoardMove {
    DasLeft,
    DasRight,
    SonicDrop,
    Left,
    Right,
    RotLeft,
    RotRight
}

#[no_mangle]
pub extern "C" fn find_path(
    path: *mut c_char, field_ptr: *const c_int, piece: c_char,
    x: c_int, y: c_int, rot: c_int, tspin: c_char
    ) {
    let raw_field = unsafe { std::slice::from_raw_parts(field_ptr, 23) };
    let mut field = [[false; 10]; 40];
    for (y, &row) in raw_field.iter().rev().enumerate() {
        for x in 0..10 {
            field[y][x] = (row & (1 << x)) != 0;
        }
    }
    let mut board = Board::<u16>::new();
    board.set_field(field);
    let target = FallingPiece {
        kind: PieceState(Piece::from_char((piece as u8) as char), match rot {
            0 => RotationState::North,
            1 => RotationState::East,
            2 => RotationState::South,
            3 => RotationState::West,
            _ => unreachable!("Invalid rotation: {}", rot)
        }),
        x,
        y,
        tspin: match (tspin as u8) as char {
            ' ' => TspinStatus::None,
            't' => TspinStatus::Mini,
            'T' => TspinStatus::Full,
            '+' => TspinStatus::PersistentFull,
            tspin => unreachable!("Invalid T-Spin type: {}", tspin)
        }
    };
    if let Some(piece) = FallingPiece::spawn(target.kind.0, &board) {
        let mut queue = VecDeque::new();
        let mut nodes = HashMap::new();
        queue.push_back(piece);
        nodes.insert(piece, (piece, BoardMove::SonicDrop));
        while let Some(parent_pos) = queue.pop_front() {
            for &mv in &BOARD_MOVES {
                let mut child_pos = parent_pos;
                match mv {
                    BoardMove::DasLeft => while child_pos.shift(&board, -1, 0) {},
                    BoardMove::DasRight => while child_pos.shift(&board, 1, 0) {},
                    BoardMove::SonicDrop => while child_pos.shift(&board, 0, -1) {},
                    BoardMove::Left => { child_pos.shift(&board, -1, 0); },
                    BoardMove::Right => { child_pos.shift(&board, 1, 0); },
                    BoardMove::RotLeft => { child_pos.ccw(&board); },
                    BoardMove::RotRight => { child_pos.cw(&board); }
                }
                if child_pos != parent_pos {
                    nodes.entry(child_pos).or_insert_with(|| {
                        queue.push_back(child_pos);
                        (parent_pos, mv)
                    });
                    if child_pos == target {
                        let mut moves = Vec::new();
                        moves.push(mv);
                        let mut pos = child_pos;
                        loop {
                            let &(parent_pos, mv) = nodes.get(&pos).unwrap();
                            if pos == parent_pos {
                                break;
                            }
                            moves.push(mv);
                            pos = parent_pos;
                        }
                        let moves_len = moves.len();
                        if moves_len > 32 {
                            unreachable!("Path found could not fit in buffer.");
                        }
                        for (i, mv) in moves.into_iter().skip(1).rev().enumerate() {
                            unsafe {
                                *path.add(i) = (match mv {
                                    BoardMove::DasLeft => 'L',
                                    BoardMove::DasRight => 'R',
                                    BoardMove::SonicDrop => 'D',
                                    BoardMove::Left => 'l',
                                    BoardMove::Right => 'r',
                                    BoardMove::RotLeft => 'z',
                                    BoardMove::RotRight => 'c'
                                } as u8) as c_char;
                            }
                        }
                        unsafe {
                            *path.add(moves_len) = 0;
                        }
                        return;
                    }
                }
            }
        }
    }
    unsafe {
        *path = 0;
    }
}