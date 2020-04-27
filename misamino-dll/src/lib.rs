use std::ffi::CString;
use std::os::raw::*;
use lazy_static::lazy_static;
use std::sync::Mutex;
use libtetris::{ Board, Piece, PieceMovement };
use serde::{ Serialize, Deserialize };
use std::path::PathBuf;
use std::str::FromStr;
use std::io::{ BufReader, BufWriter };
use std::fs::File;

#[no_mangle]
pub extern "C" fn AIDllVersion() -> c_int {
    2
}

#[no_mangle]
pub extern "C" fn AIName(level: c_int) -> *mut c_char {
    //Ensure that the options are loaded by now
    let _options = &OPTIONS.ai_p1;
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

fn create_misa_interface() -> Mutex<MisaInterface> {
    Mutex::new(MisaInterface {
        bot: None,
        move_ptr: None,
        expected_field: [[false; 10]; 40],
        expected_queue: Vec::new(),
        prev_field: [[true; 10]; 40],
        prev_hold: None
    })
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
struct BotConfig {
    options: cold_clear::Options,
    pathfinder: Option<PathBuf>,
    weights: cold_clear::evaluation::Standard,
}

impl Default for BotConfig {
    fn default() -> BotConfig {
        BotConfig {
            options: cold_clear::Options::default(),
            pathfinder: None,
            weights: cold_clear::evaluation::Standard::fast_config()
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
struct MisaCCOptions {
    pub ai_p1: BotConfig,
    pub ai_p2: BotConfig
}

fn dll_path() -> Result<PathBuf, (&'static str, u32)> {
    use winapi::um::libloaderapi::*;
    use winapi::um::winnt::LPCSTR;
    use winapi::shared::minwindef::HMODULE;
    use winapi::um::errhandlingapi::GetLastError;
    let mut hm = 0 as HMODULE;
    if unsafe { GetModuleHandleExA(
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | 
        GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        dll_path as LPCSTR,
        &mut hm) } == 0 {
        return Err(("GetModuleHandleExA", unsafe { GetLastError() }));
    }
    let filename_ptr = CString::new([' '; 255].iter().collect::<String>()).unwrap().into_raw();
    let result = unsafe { GetModuleFileNameA(hm, filename_ptr, 255) };
    let filename = unsafe { CString::from_raw(filename_ptr) };
    if result == 0 {
        return Err(("GetModuleFileNameA", unsafe { GetLastError() }));
    }
    let filename = filename.into_string().unwrap();
    Ok(PathBuf::from_str(&filename[0..(result as usize)]).unwrap())
}

#[derive(Debug)]
enum MisaCCOptionError {
    WinDllPathError((&'static str, u32)),
    FileError(std::io::Error),
    YamlParsingError(serde_yaml::Error)
}

impl From<(&'static str, u32)> for MisaCCOptionError {
    fn from(err: (&'static str, u32)) -> MisaCCOptionError {
        MisaCCOptionError::WinDllPathError(err)
    }
}

impl From<std::io::Error> for MisaCCOptionError {
    fn from(err: std::io::Error) -> MisaCCOptionError {
        MisaCCOptionError::FileError(err)
    }
}

impl From<serde_yaml::Error> for MisaCCOptionError {
    fn from(err: serde_yaml::Error) -> MisaCCOptionError {
        MisaCCOptionError::YamlParsingError(err)
    }
}

impl MisaCCOptions {
    fn read_options() -> Result<MisaCCOptions, MisaCCOptionError> {
        let path = dll_path()?.parent().unwrap().join("cc_options.yaml");
        match File::open(&path) {
            Ok(file) => Ok(serde_yaml::from_reader(BufReader::new(file))?),
            Err(e) => if e.kind() == std::io::ErrorKind::NotFound {
                let options = MisaCCOptions::default();
                serde_yaml::to_writer(BufWriter::new(File::create(path)?), &options)?;
                Ok(options)
            } else {
                Err(e.into())
            }
        }
    }
}

lazy_static! {
    static ref STATE: [Mutex<MisaInterface>; 2] = [
        create_misa_interface(),
        create_misa_interface()
    ];
    static ref OPTIONS: MisaCCOptions = MisaCCOptions::read_options()
        .expect("Failed to read or create options file");
}

fn create_interface(board: &Board, player: i32) -> cold_clear::Interface {
    let config = if player == 0 {
        OPTIONS.ai_p1.clone()
    } else {
        OPTIONS.ai_p2.clone()
    };
    cold_clear::Interface::launch(
        board.clone(),
        config.options,
        config.weights
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
    let mut state = STATE[player as usize].lock().unwrap();
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
        state.bot = Some(create_interface(&board, player));
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
            state.bot = Some(create_interface(&board, player));
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