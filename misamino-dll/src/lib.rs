use std::ffi::CString;
use std::os::raw::*;
use lazy_static::lazy_static;
use std::sync::Mutex;
use libtetris::{ Board, Piece };

#[no_mangle]
pub extern "C" fn AIDllVersion() -> c_int {
    2
}

#[no_mangle]
pub extern "C" fn AIName(level: c_int) -> *mut c_char {
    //Pretty sure this leaks memory but hopefully MisaMino Client just calls it once
    CString::new(format!("Cold Clear LVL {}", level)).unwrap().into_raw()
}


struct GlobalState {

}

lazy_static! {
    static ref STATE: Mutex<GlobalState> = Mutex::new(GlobalState {
        
    });
}

#[no_mangle]
pub extern "C" fn TetrisAI(
    overfield: *const c_int, field: *const c_int, field_w: c_int, field_h: c_int, b2b: c_int,
    combo: c_int, next: *const c_char, hold: c_char, curCanHold: bool, active: c_char,
    x: i32, y: i32, spin: i32, canhold: bool, can180spin: bool, upcomeAtt: c_int,
    comboTable: *const c_int, maxDepth: c_int, level: c_int, player: c_int)
    -> *mut c_char {
    assert!(!field.is_null(), "`field` was null");
    assert!(field_w == 10, "`field_w` was not 10");
    assert!(field_h == 22, "`field_h` was not 22");
    let raw_field: &[c_int] = unsafe { std::slice::from_raw_parts(field, field_h as usize + 1) };
    let mut field = [[false; 10]; 40];
    for (y, &row) in raw_field.iter().rev().enumerate() {
        for x in 0..10 {
            field[y][x] = (row & (1 << x)) != 0;
        }
    }
    let next: &[c_char] = unsafe { std::slice::from_raw_parts(next, maxDepth as usize) };
    let mut board = Board::<u16>::new();
    board.set_field(field);
    for &piece in next {
        board.add_next_piece(match (piece as u8) as char {
            'I' => Piece::I,
            'T' => Piece::T,
            'O' => Piece::O,
            'L' => Piece::L,
            'J' => Piece::J,
            'S' => Piece::S,
            'Z' => Piece::Z,
            p => unreachable!("Invalid piece: {}", p)
        })
    }
    //TODO
    CString::new("V").unwrap().into_raw()
}