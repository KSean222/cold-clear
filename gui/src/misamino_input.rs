use libtetris::*;
use battle::{ Event, PieceMoveExecutor };
use game_util::glutin::VirtualKeyCode;
use gilrs::Gamepad;
use std::collections::{HashSet, VecDeque};
use std::process::{Command, ChildStdin, Stdio};
use std::io::{BufReader, BufWriter, BufRead, Write};
use std::sync::mpsc::{channel, Receiver};
use crate::input::InputSource;
use serde::{Serialize, Deserialize};

#[allow(non_snake_case)]
#[derive(Serialize)]
struct MisaMinoArgs {
    Queue: Vec<Piece>,
    Current: Piece,
    Hold: Option<Piece>,
    Height: i32,
    Field: Vec<[bool; 10]>,
    Combo: u32,
    B2b: bool,
    Garbage: u32
}

#[allow(non_snake_case)]
#[derive(Deserialize)]
struct MisaMinoResult {
    Instructions: Vec<u32>
}

pub struct MisaMinoInput {
    stdin: BufWriter<ChildStdin>,
    rx: Receiver<Result<MisaMinoResult, ()>>,
    args: VecDeque<MisaMinoArgs>,
    discard: bool,
    executing: Option<PieceMoveExecutor>,
    controller: Controller,
    speed_limit: u32
}

impl MisaMinoInput {
    pub fn new(speed_limit: u32) -> Self {
        let interface = Command::new("./bots/MisaMino/MisaMinoCLI.exe")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let (tx, rx) = channel();

        let mut stdout = BufReader::new(interface.stdout.unwrap());
        std::thread::spawn(move || {
            let mut line = String::new();
            while stdout.read_line(&mut line).is_ok() {
                tx.send(serde_json::from_str(&line).unwrap()).unwrap();
                line.clear();
            }
        });
        MisaMinoInput {
            stdin: BufWriter::new(interface.stdin.unwrap()),
            rx,
            args: VecDeque::new(),
            discard: false,
            executing: None,
            controller: Default::default(),
            speed_limit,
        }
    }
}

impl InputSource for MisaMinoInput {
    fn controller(&self, _keys: &HashSet<VirtualKeyCode>, _gamepad: Option<Gamepad>) -> Controller {
        self.controller
    }

    fn update(
        &mut self, board: &Board<ColoredRow>, events: &[Event], incoming: u32
    ) -> Option<cold_clear::Info> {
        for event in events {
            match event {
                Event::SpawnDelayStart => if self.executing.is_none() {
                    self.find_move(board, incoming);
                },
                Event::PieceSpawned { .. } => {
                    self.abort();
                    // self.find_move(board, incoming);
                }
                // Event::GarbageAdded(_) => {
                //     self.discard = true;
                //     self.abort();
                //     self.find_move(board, incoming);
                // }
                _ => {}
            }
        }
        if self.executing.is_none() {
            if let Ok(result) = self.rx.try_recv() {
                let args = self.args.pop_front().unwrap();
                if self.discard {
                    self.discard = false;
                } else {
                    let mut hold = false;
                    let mut inputs = VecDeque::new();
                    if let Ok(result) = result {
                        if let Some(mut piece) = SpawnRule::Row19Or20.spawn(args.Current, board) {
                            for instruction in result.Instructions {
                                let instruction = match instruction {
                                    1 => Some((PieceMovement::Left, false)),
                                    2 => Some((PieceMovement::Right, false)),
                                    3 => Some((PieceMovement::Left, true)),
                                    4 => Some((PieceMovement::Right, true)),
                                    //Technically 5 is the "soft drop one down" instruction but that's tricky to support
                                    5 | 6 => Some((PieceMovement::SonicDrop, false)),
                                    7 => Some((PieceMovement::Ccw, false)),
                                    8 => Some((PieceMovement::Cw, false)),
                                    10 => {
                                        hold = true;
                                        let new_piece = args.Hold.unwrap_or(*args.Queue.first().unwrap());
                                        if let Some(new_piece) = SpawnRule::Row19Or20.spawn(new_piece, board) {
                                            piece = new_piece;
                                        } else {
                                            break;
                                        }
                                        None
                                    }
                                    _ => None
                                };
                                if let Some((movement, repeated)) = instruction {
                                    while movement.apply(&mut piece, board) {
                                        inputs.push_back(movement);
                                        if !repeated {
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    self.executing = Some(PieceMoveExecutor::new(hold, inputs, self.speed_limit));
                }
            }
        }
        if let Some(executor) = &mut self.executing {
            if executor.update(&mut self.controller, board, events).is_some() {
                self.executing = None;
            }
        }
        None
    }
}

impl MisaMinoInput {
    fn find_move(&mut self, board: &Board<impl Row>, incoming: u32) {
        let current = board.get_next_piece().unwrap();
        let height = SpawnRule::Row19Or20
            .spawn(current, board)
            .map(|x| x.y + 2)
            .unwrap_or(20);
        let args = MisaMinoArgs {
            Queue: board.next_queue().skip(1).collect(),
            Current: current,
            Hold: board.hold_piece,
            Height: height,
            Field: board.get_field().iter().copied().collect(),
            Combo: board.combo,
            B2b: board.b2b_bonus,
            Garbage: incoming
        };
        serde_json::to_writer(&mut self.stdin, &args).unwrap();
        writeln!(&mut self.stdin).unwrap();
        self.stdin.flush().unwrap();
        self.args.push_back(args);
    }

    fn abort(&mut self) {
        writeln!(&mut self.stdin, "\"abort\"").unwrap();
        self.stdin.flush().unwrap();
    }
}