use std::ffi::CString;
use std::os::raw::*;
use lazy_static::lazy_static;
use std::sync::Mutex;
use libtetris::{ Board, Piece, PieceMovement, RotationState, TspinStatus };
use serde::{ Serialize, Deserialize };
use std::path::PathBuf;
use std::io::{ BufReader, BufWriter };
use std::io::Write;
use std::fs::File;
use std::collections::HashMap;

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
    fn load_pathfinders(&self) -> HashMap<PathBuf, libloading::Library> {
        let mut map = HashMap::new();
        fn load_config(config: &BotConfig, map: &mut HashMap<PathBuf, libloading::Library>) {
            if let Some(path) = &config.pathfinder {
                match libloading::Library::new(DLL_DIR.clone().join(path)) {
                    Ok(lib) => { map.insert(path.clone(), lib); },
                    Err(err) => println!("Error loading {:?}: {}", path, err)
                }
            }
        }
        load_config(&self.ai_p1, &mut map);
        load_config(&self.ai_p2, &mut map);
        map
    }
}

lazy_static! {
    static ref STATE: [Mutex<MisaInterface>; 2] = [
        create_misa_interface(),
        create_misa_interface()
    ];
    static ref OPTIONS: MisaCCOptions = MisaCCOptions::read_options()
        .expect("Failed to read or create options file");
    static ref PATHFINDERS: HashMap<PathBuf, libloading::Library> = OPTIONS.load_pathfinders();
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

fn create_interface(board: &Board) -> cold_clear::Interface {
    cold_clear::Interface::launch(
        board.clone(),
        cold_clear::Options::default(),
        cold_clear::evaluation::Standard::fast_config()
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
            return state.last_move.as_ptr();
        }
        if state.expected_queue.iter().zip(next.iter()).any(|p| *p.0 != *p.1) {
            println!("Detected new game. Reset bot.");
            state.bot = None;
            state.bot = Some(create_interface(&board, player));
            update_queue = false;
        }
        if state.expected_queue.iter().zip(next.iter()).any(|p| *p.0 != *p.1) {
            println!("Detected new game. Reset bot.");
            state.bot = None;
            state.bot = Some(create_interface(&board));
            if let Some(ptr) = state.move_ptrs[player as usize] {
                let _ = unsafe { CString::from_raw(ptr as *mut c_char) };
            }
            state.move_ptrs[player as usize] = None;
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
    state.last_move = if let Some((mv, _)) = bot.block_next_move() {
        let pathfinder = &(if player == 0 {
            &OPTIONS.ai_p1
        } else {
            &OPTIONS.ai_p2
        }).pathfinder;
        let mut moves = String::with_capacity(32);
        board.lock_piece(mv.expected_location);
        state.expected_field = board.get_field();
        state.expected_queue = next;
        state.expected_queue.remove(0);
        if mv.hold {
            moves.push('v');
        }
        if let Some(path) = pathfinder {
            let path_ptr = CString::new(" ".repeat(32)).unwrap().into_raw();
            unsafe {
                let find_path: libloading::Symbol<unsafe extern fn(
                        path: *mut c_char,
                        field: *const c_int,
                        piece: c_char,
                        x: c_int,
                        y: c_int,
                        rot: c_int,
                        tspin: c_char
                    )> = PATHFINDERS.get(path).unwrap().get(b"find_path").unwrap();
                find_path(
                    path_ptr,
                    field_ptr,
                    if !mv.hold {
                        active
                    } else if hold_char != ' ' {
                        hold_char as u8 as c_char
                    } else {
                        next_char[0]
                    },
                    mv.expected_location.x,
                    mv.expected_location.y,
                    match mv.expected_location.kind.1 {
                        RotationState::North => 0,
                        RotationState::East => 1,
                        RotationState::South => 2,
                        RotationState::West => 3
                    },
                    (match mv.expected_location.tspin {
                        TspinStatus::None => ' ',
                        TspinStatus::Mini => 't',
                        TspinStatus::Full => 'T',
                        TspinStatus::PersistentFull => '+'
                    } as u8) as c_char
                );
                moves.push_str(CString::from_raw(path_ptr).to_str().unwrap());
            }
        } else {
            for mv in mv.inputs {
                moves.push(match mv {
                    PieceMovement::Left => 'l',
                    PieceMovement::Right => 'r',
                    PieceMovement::Cw => 'c',
                    PieceMovement::Ccw => 'z',
                    PieceMovement::SonicDrop => 'D'
                });
            }
        }
        moves.push('V');
        CString::new(moves).unwrap()
        
    } else {
        CString::new("V").unwrap()
    };
    state.last_move.as_ptr()
}