use std::ffi::CString;
use std::os::raw::*;
use lazy_static::lazy_static;
use std::sync::Mutex;
use libtetris::{ Board, Piece, PieceMovement };

#[no_mangle]
pub extern "C" fn AIDllVersion() -> c_int {
    2
}

#[no_mangle]
pub extern "C" fn AIName(level: c_int) -> *mut c_char {
    //Pretty sure this leaks memory but hopefully MisaMino Client just calls it once
    CString::new(format!("Cold Clear LVL {}", level)).unwrap().into_raw()
}

struct MisaInterface {
    bot: Option<cold_clear::Interface>,
    move_ptr: Option<usize>,
    expected_field: [[bool; 10]; 40],
    expected_queue: Vec<Piece>,
    prev_field: [[bool; 10]; 40],
    prev_hold: Option<Piece>
}

impl MisaInterface {
    fn new() -> MisaInterface {
        MisaInterface {
            bot: None,
            move_ptr: None,
            expected_field: [[false; 10]; 40],
            expected_queue: Vec::new(),
            prev_field: [[true; 10]; 40],
            prev_hold: None
        }
    }
}

lazy_static! {
    static ref STATE: Mutex<[MisaInterface; 2]> = Mutex::new([MisaInterface::new(), MisaInterface::new()]);
}

fn create_interface(board: &Board) -> cold_clear::Interface {
    cold_clear::Interface::launch(
        board.clone(),
        cold_clear::Options::default(),
        cold_clear::evaluation::Standard::fast_config()
    )
}

#[no_mangle]
pub extern "C" fn TetrisAI(
    overfield: *const c_int, field: *const c_int, field_w: c_int, field_h: c_int, b2b: c_int,
    combo: c_int, next: *const c_char, hold: c_char, cur_can_hold: bool, active: c_char,
    x: i32, y: i32, spin: i32, canhold: bool, can180spin: bool, incoming_att: c_int,
    combo_table: *const c_int, max_depth: c_int, level: c_int, player: c_int)
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
    let next: &[c_char] = unsafe { std::slice::from_raw_parts(next, max_depth as usize) };
    let next: Vec<Piece> = next.into_iter().map(|&p| Piece::from_char((p as u8) as char)).collect();
    let hold = (hold as u8) as char;
    let hold = if hold == ' ' {
        None
    } else {
        Some(Piece::from_char(hold))
    };
    let mut state = &mut STATE.lock().unwrap()[player as usize];
    let mut board = Board::<u16>::new();
    board.add_next_piece(Piece::from_char((active as u8) as char));
    board.set_field(field);
    for &piece in &next {
        board.add_next_piece(piece);
    }
    board.hold_piece = hold;
    board.b2b_bonus = b2b != 0;
    board.combo = combo as u32;
    let mut update_queue = true;
    if state.bot.is_none() {
        state.bot = Some(create_interface(&board));
        update_queue = false;
    }
    let mut unexpected = false;
    for (i, &row) in field.iter().enumerate() {
        if state.expected_field[i] != row {
            unexpected = true;
            break;
        }
    }
    if unexpected {
        let mut piece_dropped = false;
        for (i, &row) in field.iter().enumerate() {
            if state.prev_field[i] != row {
                piece_dropped = true;
                break;
            }
        }
        
        if piece_dropped {
            println!("Misdrop or garbage!");
            state.bot.as_mut().unwrap().reset(field, b2b != 0, combo as u32);
        } else {
            println!("Returned old calculation for board");
            return state.move_ptr.unwrap() as *mut c_char;
        }
        if state.expected_queue.iter().zip(next.iter()).any(|p| *p.0 != *p.1) {
            println!("Detected new game. Reset bot.");
            state.bot = None;
            state.bot = Some(create_interface(&board));
            if let Some(ptr) = state.move_ptr {
                let _ = unsafe { CString::from_raw(ptr as *mut c_char) };
            }
            state.move_ptr = None;
            update_queue = false;
        }
    }
    if update_queue {
        if state.prev_hold.is_none() && hold.is_some() {
            state.bot.as_mut().unwrap().add_next_piece(next[next.len() - 2]);
        }
        state.bot.as_mut().unwrap().add_next_piece(next[next.len() - 1]);
    }
    state.prev_hold = hold;
    state.prev_field = field;
    let bot = state.bot.as_mut().unwrap();
    bot.request_next_move(incoming_att as u32);
    let ptr = if let Some((mv, _)) = bot.block_next_move() {
        let mut moves = String::with_capacity(mv.inputs.len() + 2);
        board.lock_piece(mv.expected_location);
        state.expected_field = board.get_field();
        state.expected_queue = next;
        state.expected_queue.remove(0);
        if mv.hold {
            moves.push('v');
        }
        for mv in mv.inputs {
            moves.push(match mv {
                PieceMovement::Left => 'l',
                PieceMovement::Right => 'r',
                PieceMovement::Cw => 'c',
                PieceMovement::Ccw => 'z',
                PieceMovement::SonicDrop => 'D'
            });
        }
        moves.push('V');
        CString::new(moves).unwrap()
    } else {
        CString::new("V").unwrap()
    }.into_raw();
    if let Some(ptr) = state.move_ptr {
        let _ = unsafe { CString::from_raw(ptr as *mut c_char) };
    }
    state.move_ptr = Some(ptr as usize);
    ptr
}