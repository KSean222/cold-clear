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


struct GlobalState {
    bot: Option<cold_clear::Interface>,
    move_ptrs: [Option<usize>; 2],
    last_seen_field: [[bool; 10]; 40],
    reset: bool,
    prev_hold: char
}

lazy_static! {
    static ref STATE: Mutex<GlobalState> = Mutex::new(GlobalState {
        bot: None,
        move_ptrs: [None; 2],
        last_seen_field: [[true; 10]; 40],
        reset: false,
        prev_hold: ' '
    });
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
    let mut state = STATE.lock().unwrap();
    if state.bot.is_none() {
        let mut board = Board::<u16>::new();
        board.add_next_piece(Piece::from_char((active as u8) as char));
        board.set_field(field);
        for &piece in &next {
            board.add_next_piece(piece);
        }
        board.hold_piece = if hold == ' ' {
            None
        } else {
            Some(Piece::from_char(hold))
        };
        board.b2b_bonus = b2b != 0;
        board.combo = combo as u32;
        state.bot = Some(cold_clear::Interface::launch(
            board,
            cold_clear::Options {
                speculate: true,
                ..Default::default()
            },
            cold_clear::evaluation::Standard::fast_config()
        ));
    } else if state.reset {
        state.bot.as_mut().unwrap().reset(field, b2b != 0, combo as u32);
        state.reset = false;
    } else {
        if state.prev_hold == ' ' && hold != ' ' {
            println!("Added new piece {}", next[next.len() - 2].to_char());
            state.bot.as_mut().unwrap().add_next_piece(next[next.len() - 2]);
        }
        println!("Added new piece {}", next[next.len() - 1].to_char());
        state.bot.as_mut().unwrap().add_next_piece(next[next.len() - 1]);
    }
    state.prev_hold = hold;
    state.reset = true;
    for (i, &row) in field.iter().enumerate() {
        if state.last_seen_field[i] != row {
            state.reset = false;
        }
    }
    if state.reset {
        println!("Returned old calculation for board");
        return state.move_ptrs[player as usize].unwrap() as *mut c_char;
    }
    state.last_seen_field = field;
    let bot = state.bot.as_mut().unwrap();
    bot.request_next_move(incoming_att as u32);
    //TODO check if it doesn't leak memory
    //It desyncs because MisaMino calls the bot *again* if there's new information. That's what we want, but we need to detect these and apply the reset function
    let ptr = if let Some((mv, _)) = bot.block_next_move() {
        let mut moves = String::with_capacity(mv.inputs.len() + 2);
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
    if let Some(ptr) = state.move_ptrs[player as usize] {
        let _ = unsafe { CString::from_raw(ptr as *mut c_char) };
    }
    state.move_ptrs[player as usize] = Some(ptr as usize);
    ptr
}