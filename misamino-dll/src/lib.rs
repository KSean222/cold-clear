use std::ffi::{ CString, CStr };
use std::os::raw::*;
use std::sync::Mutex;
use std::path::PathBuf;
use std::io::{ BufReader, BufWriter, Write };
use std::fs::File;

use lazy_static::lazy_static;
use libtetris::*;
use serde::{ Serialize, Deserialize };

mod das_pf;
use das_pf::DasPieceMovement;

lazy_static! {
    static ref DLL_DIR: PathBuf = dll_path().unwrap().parent().unwrap().to_path_buf();
}

#[no_mangle]
pub extern "C" fn AIDllVersion() -> c_int {
    2
}

#[no_mangle]
pub extern "C" fn AIName(level: c_int) -> *mut c_char {
    let _options = &OPTIONS.ai_p1;
    //Pretty sure this leaks memory but hopefully MisaMino Client just calls it once
    CString::new(format!("Cold Clear LVL {}", level)).unwrap().into_raw()
}

//tetris_ai_runner compatibility
#[no_mangle]
pub extern "C" fn Name() -> *const c_char {
    CStr::from_bytes_with_nul(b"Cold Clear\0").unwrap().as_ptr()
}

struct MisaInterface {
    bot: Option<cold_clear::Interface>,
    last_move: CString,
    expected_field: [[bool; 10]; 40],
    expected_queue: Vec<Piece>,
    prev_field: [[bool; 10]; 40],
    prev_hold: Option<Piece>
}

fn create_misa_interface() -> Mutex<MisaInterface> {
    Mutex::new(MisaInterface {
        bot: None,
        last_move: CString::default(),
        expected_field: [[false; 10]; 40],
        expected_queue: Vec::new(),
        prev_field: [[true; 10]; 40],
        prev_hold: None
    })
}

#[derive(Serialize, Deserialize, Clone)]
enum PathfinderMode {
    Hypertap,
    Das
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
struct BotConfig {
    options: cold_clear::Options,
    pathfinder: PathfinderMode,
    weights: cold_clear::evaluation::Standard,
}

impl Default for BotConfig {
    fn default() -> BotConfig {
        BotConfig {
            options: cold_clear::Options {
                spawn_rule: SpawnRule::Row21AndFall,
                ..Default::default()
            },
            pathfinder: PathfinderMode::Das,
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
    let path_ptr = CString::new(" ".repeat(255)).unwrap().into_raw();
    let path_size = unsafe { GetModuleFileNameA(hm, path_ptr, 255) };
    let path = unsafe { CString::from_raw(path_ptr) };
    if path_size == 0 {
        return Err(("GetModuleFileNameA", unsafe { GetLastError() }));
    }
    Ok(PathBuf::from(path.into_string().unwrap()))
}

#[derive(Debug)]
enum MisaCCOptionError {
    FileError(std::io::Error),
    YamlParsingError(serde_yaml::Error)
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
        let options_path = DLL_DIR.clone().join("cc_options.yaml");
        match File::open(&options_path) {
            Ok(file) => Ok(serde_yaml::from_reader(BufReader::new(file))?),
            Err(e) => if e.kind() == std::io::ErrorKind::NotFound {
                let options = MisaCCOptions::default();
                let mut file = BufWriter::new(File::create(options_path)?);
                write!(&mut file, "{}", include_str!("options-header"))?;
                serde_yaml::to_writer(file, &options)?;
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
    overfield: *const c_int, field_ptr: *const c_int, field_w: c_int, field_h: c_int, b2b: c_int,
    combo: c_int, next: *const c_char, hold: c_char, cur_can_hold: bool, active: c_char,
    x: i32, y: i32, spin: i32, canhold: bool, can180spin: bool, incoming_att: c_int,
    combo_table: *const c_int, max_depth: c_int, level: c_int, player: c_int)
    -> *const c_char {
    assert!(!field_ptr.is_null(), "`field` was null");
    assert!(field_w == 10, "`field_w` was not 10");
    assert!(field_h == 22, "`field_h` was not 22");
    let raw_field: &[c_int] = unsafe { std::slice::from_raw_parts(field_ptr, field_h as usize + 1) };
    let mut field = [[false; 10]; 40];
    for (y, &row) in raw_field.iter().rev().enumerate() {
        for x in 0..10 {
            field[y][x] = (row & (1 << x)) != 0;
        }
    }
    let next_char: &[c_char] = unsafe { std::slice::from_raw_parts(next, max_depth as usize) };
    let next: Vec<Piece> = next_char.iter().map(|&p| Piece::from_char((p as u8) as char)).collect();
    let hold_char = (hold as u8) as char;
    let hold = if hold_char == ' ' {
        None
    } else {
        Some(Piece::from_char(hold_char))
    };
    let mut state = STATE[player as usize].lock().unwrap();
    let mut board = Board::<u16>::new();
    board.add_next_piece(Piece::from_char((active as u8) as char));
    board.set_field(field);
    for &piece in next.iter().take(level as usize) {
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
    let mut unexpected_field = false;
    for (i, &row) in field.iter().enumerate() {
        if state.expected_field[i] != row {
            unexpected_field = true;
            break;
        }
    }
    let unexpected_queue = state.expected_queue.iter().zip(next.iter()).any(|p| *p.0 != *p.1);
    if unexpected_queue {
        state.bot = None;
        state.bot = Some(create_interface(&board, player));
        update_queue = false;
    } else if unexpected_field {
        let mut piece_dropped = false;
        for (i, &row) in field.iter().enumerate() {
            if state.prev_field[i] != row {
                piece_dropped = true;
                break;
            }
        }
        if piece_dropped {
            state.bot.as_mut().unwrap().reset(field, b2b != 0, combo as u32);
        } else {
            return state.last_move.as_ptr();
        }
    }
    if update_queue {
        if state.prev_hold.is_none() && hold.is_some() && level > 1 {
            state.bot.as_mut().unwrap().add_next_piece(next[level as usize - 2]);
        }
        if level > 0 {
            state.bot.as_mut().unwrap().add_next_piece(next[level as usize - 1]);
        }
    }
    state.prev_hold = hold;
    state.prev_field = field;
    let bot = state.bot.as_mut().unwrap();
    bot.request_next_move(incoming_att as u32);
    state.last_move = if let Some((mv, _)) = bot.block_next_move() {
        let mut moves = String::with_capacity(32);
        if mv.hold {
            moves.push('v');
        }
        let options = &(if player == 0 {
            &OPTIONS.ai_p1
        } else {
            &OPTIONS.ai_p2
        });
        let inputs = match options.pathfinder {
            PathfinderMode::Hypertap => mv.inputs
                .into_iter()
                .map(|mv| mv.into())
                .collect(),
            PathfinderMode::Das => das_pf::find_das_path(
                field, 
                options.options.spawn_rule.spawn(mv.expected_location.kind.0, &board).unwrap(),
                mv.expected_location
            ).unwrap_or(Vec::new())
        };
        moves.extend(inputs.iter().map(|&mv| match mv {
            DasPieceMovement::DasLeft => 'L',
            DasPieceMovement::DasRight => 'R',
            DasPieceMovement::Left => 'l',
            DasPieceMovement::Right => 'r',
            DasPieceMovement::Cw => 'c',
            DasPieceMovement::Ccw => 'z',
            DasPieceMovement::SonicDrop => 'D'
        }));
        moves.push('V');
        board.lock_piece(mv.expected_location);
        state.expected_field = board.get_field();
        state.expected_queue = next;
        state.expected_queue.drain(0..(1 + mv.hold as usize));
        CString::new(moves).unwrap()
    } else {
        CString::new("V").unwrap()
    };
    state.last_move.as_ptr()
}