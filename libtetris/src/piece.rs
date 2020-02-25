use enumset::{ EnumSet, EnumSetType, enum_set };
use enum_map::Enum;
use serde::{ Serialize, Deserialize };

use crate::{ Board, Row };

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct FallingPiece {
    pub kind: PieceState,
    pub x: i32,
    pub y: i32,
    pub tspin: TspinStatus
}

const CCW_ROT: [(i32, i32); 22] = [(0, 0), (1, 0), (0, -1), (1, -1), (0, -2), (1, -2), (2, 0), (2, -1), (2, -2), (-1, 0), (-1, -1), (0, 1), (1, 1), (2, 1), (-1, -2), (-2, 0), (0, 2), (1, 2), (2, 2), (-2, -1), (-2, -2), (-1, 1)];
const CW_ROT: [(i32, i32); 22] = [(0, 0), (-1, 0), (0, -1), (-1, -1), (0, -2), (-1, -2), (-2, 0), (-2, -1), (-2, -2), (1, 0), (1, -1), (0, 1), (-1, 1), (-2, 1), (1, -2), (2, 0), (0, 2), (-1, 2), (-2, 2), (2, -1), (2, -2), (1, 1)];
impl FallingPiece {
    pub fn spawn<R: Row>(piece: Piece, board: &Board<R>) -> Option<FallingPiece> {
        let mut this = FallingPiece {
            kind: PieceState(piece, RotationState::North),
            x: 4, y: 20,
            tspin: TspinStatus::None
        };

        if board.obstructed(&this) {
            None
        } else {
            this.y -= 1;
            if board.obstructed(&this) {
                this.y += 1;
            }

            Some(this)
        }
    }

    pub fn cells(&self) -> [(i32, i32, EnumSet<Direction>); 4] {
        let mut cells = self.kind.cells();
        for (dx, dy, _) in cells.iter_mut() {
            *dx += self.x;
            *dy += self.y;
        }
        cells
    }

    pub fn shift<R: Row>(&mut self, board: &Board<R>, dx: i32, dy: i32) -> bool {
        self.x += dx;
        self.y += dy;
        if board.obstructed(self) {
            self.x -= dx;
            self.y -= dy;
            false
        } else {
            self.tspin = TspinStatus::None;
            true
        }
    }

    pub fn sonic_drop<R: Row>(&mut self, board: &Board<R>) -> bool {
        let drop_by = self.cells()
            .iter()
            .map(|&(x, y, _)| y - board.column_heights()[x as usize])
            .min().unwrap();
        if drop_by > 0 {
            self.tspin = TspinStatus::None;
            self.y -= drop_by;
            true
        } else if drop_by < 0 {
            let mut fell = false;
            loop {
                self.y -= 1;
                if board.obstructed(self) {
                    self.y += 1;
                    break
                }
                fell = true;
                self.tspin = TspinStatus::None;
            }
            fell
        } else {
            false
        }
    }

    fn rotate<R: Row>(&mut self, target: PieceState, board: &Board<R>, is_ccw: bool) -> bool {
        let initial = *self;
        self.kind = target;
        let kicks = if is_ccw { CCW_ROT } else { CW_ROT };
        for &(dx, dy) in &kicks {
            self.x = initial.x + dx;
            self.y = initial.y + dy;
            if !board.obstructed(self) {
                let mut piece = *self;
                if !piece.shift(board, -1, 0) && !piece.shift(board, 1, 0) && !piece.shift(board, 0, 1) {
                    self.tspin = TspinStatus::Full;
                }
                return true
            }
        }
        
        *self = initial;
        false
    }

    pub fn cw<R: Row>(&mut self, board: &Board<R>) -> bool {
        let mut target = self.kind;
        target.cw();
        self.rotate(target, board, false)
    }

    pub fn ccw<R: Row>(&mut self, board: &Board<R>) -> bool {
        let mut target = self.kind;
        target.ccw();
        self.rotate(target, board, true)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum CellColor {
    I, O, T, L, J, S, Z,
    Garbage,
    Unclearable,
    Empty
}

#[derive(Debug, Hash, EnumSetType, Enum, Serialize, Deserialize)]
pub enum Piece {
    I, O, T, L, J, S, Z
}

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum RotationState {
    North, South, East, West
}

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct PieceState(pub Piece, pub RotationState);

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum TspinStatus {
    None,
    Mini,
    Full,
    PersistentFull
}

impl RotationState {
    pub fn cw(&mut self) {
        use RotationState::*;
        match self {
            North => *self = East,
            East  => *self = South,
            South => *self = West,
            West  => *self = North
        }
    }
    
    pub fn ccw(&mut self) {
        use RotationState::*;
        match self {
            North => *self = West,
            West  => *self = South,
            South => *self = East,
            East  => *self = North
        }
    }

    pub fn mini_tspin_corners(self) -> [(i32, i32); 2] {
        use RotationState::*;
        match self {
            North => [(-1, 1),  (1, 1)],
            East  => [(1, 1),   (1, -1)],
            South => [(1, -1),  (-1, -1)],
            West  => [(-1, -1), (-1, 1)]
        }
    }

    pub fn non_mini_tspin_corners(self) -> [(i32, i32); 2] {
        use RotationState::*;
        match self {
            South => [(-1, 1),  (1, 1)],
            West  => [(1, 1),   (1, -1)],
            North => [(1, -1),  (-1, -1)],
            East  => [(-1, -1), (-1, 1)]
        }
    }
}

impl PieceState {
    pub fn cw(&mut self) {
        self.1.cw()
    }

    pub fn ccw(&mut self) {
        self.1.ccw()
    }

    /// Returns the cells this piece and orientation occupy relative to rotation point 1, as well
    /// as the connection directions, in no particular order.
    pub fn cells(&self) -> [(i32, i32, EnumSet<Direction>); 4] {
        use Piece::*;
        use RotationState::*;
        use Direction::*;
        
        const CELLS: [[(i32, i32, EnumSet<Direction>); 4]; 28] = [
            [
                (-1, 0, enum_set!(Right)),
                (0, 0, enum_set!(Left | Right)),
                (1, 0, enum_set!(Left | Right)),
                (2, 0, enum_set!(Left))
            ],
            [
                (0, -1, enum_set!(Up)),
                (0, 0, enum_set!(Up | Down)),
                (0, 1, enum_set!(Up | Down)),
                (0, 2, enum_set!(Down))
            ],
            [
                (-2, 0, enum_set!(Right)),
                (-1, 0, enum_set!(Left | Right)),
                (0, 0, enum_set!(Left | Right)),
                (1, 0, enum_set!(Left))
            ],
            [
                (0, -2, enum_set!(Up)),
                (0, -1, enum_set!(Up | Down)),
                (0, 0, enum_set!(Up | Down)),
                (0, 1, enum_set!(Down))
            ],
            
            [
                (0, 0, enum_set!(Up | Right)),
                (0, 1, enum_set!(Down | Right)),
                (1, 0, enum_set!(Up | Left)),
                (1, 1, enum_set!(Down | Left))
            ],
            [
                (0, 0, enum_set!(Up | Right)),
                (0, -1, enum_set!(Down | Right)),
                (1, 0, enum_set!(Up | Left)),
                (1, -1, enum_set!(Down | Left))
            ],
            [
                (0, 0, enum_set!(Up | Right)),
                (0, -1, enum_set!(Down | Right)),
                (-1, 0, enum_set!(Up | Left)),
                (-1, -1, enum_set!(Down | Left))
            ],
            [
                (0, 0, enum_set!(Up | Right)),
                (0, 1, enum_set!(Down | Right)),
                (-1, 0, enum_set!(Up | Left)),
                (-1, 1, enum_set!(Down | Left))
            ],

            [
                (-1, 0, enum_set!(Right)),
                (0, 0, enum_set!(Left | Right | Up)),
                (1, 0, enum_set!(Left)),
                (0, 1, enum_set!(Down))
            ],
            [
                (0, 1, enum_set!(Down)),
                (0, 0, enum_set!(Up | Down | Right)),
                (0, -1, enum_set!(Up)),
                (1, 0, enum_set!(Left))
            ],
            [
                (1, 0, enum_set!(Left)),
                (0, 0, enum_set!(Left | Right | Down)),
                (-1, 0, enum_set!(Right)),
                (0, -1, enum_set!(Up))
            ],
            [
                (0, -1, enum_set!(Up)),
                (0, 0, enum_set!(Left | Up | Down)),
                (0, 1, enum_set!(Down)),
                (-1, 0, enum_set!(Right))
            ],

            [
                (-1, 0, enum_set!(Right)),
                (0, 0, enum_set!(Left | Right)),
                (1, 0, enum_set!(Left | Up)),
                (1, 1, enum_set!(Down))
            ],
            [
                (0, 1, enum_set!(Down)),
                (0, 0, enum_set!(Up | Down)),
                (0, -1, enum_set!(Up | Right)),
                (1, -1, enum_set!(Left))
            ],
            [
                (1, 0, enum_set!(Left)),
                (0, 0, enum_set!(Left | Right)),
                (-1, 0, enum_set!(Right | Down)),
                (-1, -1, enum_set!(Up))
            ],
            [
                (0, -1, enum_set!(Up)),
                (0, 0, enum_set!(Up | Down)),
                (0, 1, enum_set!(Down | Left)),
                (-1, 1, enum_set!(Right))
            ],

            [
                (-1, 0, enum_set!(Right | Up)),
                (0, 0, enum_set!(Left | Right)),
                (1, 0, enum_set!(Left)),
                (-1, 1, enum_set!(Down))
            ],
            [
                (0, 1, enum_set!(Down | Right)),
                (0, 0, enum_set!(Up | Down)),
                (0, -1, enum_set!(Up)),
                (1, 1, enum_set!(Left))
            ],
            [
                (1, 0, enum_set!(Down | Left)),
                (0, 0, enum_set!(Left | Right)),
                (-1, 0, enum_set!(Right)),
                (1, -1, enum_set!(Up))
            ],
            [
                (0, -1, enum_set!(Left | Up)),
                (0, 0, enum_set!(Up | Down)),
                (0, 1, enum_set!(Down)),
                (-1, -1, enum_set!(Right))
            ],

            [
                (0, 0, enum_set!(Left | Up)),
                (0, 1, enum_set!(Down | Right)),
                (-1, 0, enum_set!(Right)),
                (1, 1, enum_set!(Left))
            ],
            [
                (0, 0, enum_set!(Right | Up)),
                (1, 0, enum_set!(Down | Left)),
                (0, 1, enum_set!(Down)),
                (1, -1, enum_set!(Up))
            ],
            [
                (0, -1, enum_set!(Left | Up)),
                (0, 0, enum_set!(Down | Right)),
                (-1, -1, enum_set!(Right)),
                (1, 0, enum_set!(Left))
            ],
            [
                (-1, 0, enum_set!(Right | Up)),
                (0, 0, enum_set!(Down | Left)),
                (-1, 1, enum_set!(Down)),
                (0, -1, enum_set!(Up))
            ],

            [
                (0, 0, enum_set!(Up | Right)),
                (0, 1, enum_set!(Down | Left)),
                (-1, 1, enum_set!(Right)),
                (1, 0, enum_set!(Left))
            ],
            [
                (0, 0, enum_set!(Right | Down)),
                (1, 0, enum_set!(Left | Up)),
                (1, 1, enum_set!(Down)),
                (0, -1, enum_set!(Up))
            ],
            [
                (0, -1, enum_set!(Up | Right)),
                (0, 0, enum_set!(Down | Left)),
                (-1, 0, enum_set!(Right)),
                (1, -1, enum_set!(Left))
            ],
            [
                (-1, 0, enum_set!(Right | Down)),
                (0, 0, enum_set!(Left | Up)),
                (0, 1, enum_set!(Down)),
                (-1, -1, enum_set!(Up))
            ],
        ];

        let piece_index = match self.0 {
            I => 0,
            O => 1,
            T => 2,
            L => 3,
            J => 4,
            S => 5,
            Z => 6
        };
        let rotation_index = match self.1 {
            North => 0,
            East => 1,
            South => 2,
            West => 3
        };
        let index = piece_index * 4 + rotation_index;
        CELLS[index]
    }
}

impl rand::distributions::Distribution<Piece> for rand::distributions::Standard {
    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> Piece {
        match rng.gen_range(0, 7) {
            0 => Piece::I,
            1 => Piece::T,
            2 => Piece::O,
            3 => Piece::L,
            4 => Piece::J,
            5 => Piece::S,
            6 => Piece::Z,
            _ => unreachable!()
        }
    }
}

impl Piece {
    pub fn to_char(self) -> char {
        match self {
            Piece::I => 'I',
            Piece::T => 'T',
            Piece::O => 'O',
            Piece::L => 'L',
            Piece::J => 'J',
            Piece::S => 'S',
            Piece::Z => 'Z',
        }
    }

    pub fn color(self) -> CellColor {
        match self {
            Piece::I => CellColor::I,
            Piece::T => CellColor::T,
            Piece::O => CellColor::O,
            Piece::L => CellColor::L,
            Piece::J => CellColor::J,
            Piece::S => CellColor::S,
            Piece::Z => CellColor::Z,
        }
    }
}

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum PieceMovement {
    Left,
    Right,
    Cw,
    Ccw,
    SonicDrop
}

impl PieceMovement {
    pub fn apply(self, piece: &mut FallingPiece, board: &Board) -> bool {
        match self {
            PieceMovement::Left => piece.shift(board, -1, 0),
            PieceMovement::Right => piece.shift(board, 1, 0),
            PieceMovement::Ccw => piece.ccw(board),
            PieceMovement::Cw => piece.cw(board),
            PieceMovement::SonicDrop => piece.sonic_drop(board)
        }
    }
}

#[derive(EnumSetType, Debug)]
pub enum Direction {
    Up, Down, Left, Right
}