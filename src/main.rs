use crate::BuiltIn::*;
use crate::ParseResult::*;
use rustyline::error::ReadlineError;
use rustyline::Editor;
use std::env;
use std::io;
use std::io::{Error, ErrorKind};
use std::process::{Command, Stdio};

#[derive(PartialEq, Eq, Debug, Clone)]
enum ParseResult {
    BuiltIn(BuiltIn),
    Exec { cmd: String, args: Vec<String> },
    Pipes(Vec<ParseResult>),
    Empty,
}

#[derive(PartialEq, Eq, Debug, Clone)]
enum BuiltIn {
    CD(String),
    Exit,
}

#[derive(Debug)]
struct ShellState {
    current_dir: String,
    read_line: Editor<()>,
}

fn main() -> io::Result<()> {
    let mut state = ShellState {
        //TODO choose a better default
        current_dir: env::var("HOME").unwrap_or(String::from("/home/kelton")),
        read_line: Editor::<()>::new().expect("failed to init readline"),
    };
    //TODO deal with prev. history
   
    println!("welcome to stsh! :)");
    loop {
        //print line
        //read the next line of standard input
        //TODO actually handle errors
        let line = read_in_line(&mut state)?;

        //parse the line we just saw
        let parse_res = parse(&line);

        //handle the result
        eval(&parse_res, &mut state);
    }
}
fn read_in_line(state: &mut ShellState) -> io::Result<String> {
    let status_line =
        if state.current_dir == env::var("HOME").unwrap_or(String::from("/home/kelton")) {
            String::from("> ")
        } else {
            format!("{}> ", state.current_dir)
        };

    let res = state.read_line.readline(&status_line);
    match res {
        Ok(line) => {
            state.read_line.add_history_entry(line.as_str());
            Ok(line)
        }
        Err(ReadlineError::Interrupted) => read_in_line(state),
        Err(ReadlineError::Eof) => std::process::exit(0),
        Err(_) => Err(Error::new(ErrorKind::NotFound, "failed to read line")),
    }
}

//TODO eventually make this a full PL and have this just parse an expr from stdin
fn parse(input: &String) -> ParseResult {
    let mut words = input.split_whitespace();
    match words.next() {
        Some("cd") => {
            if let Some(x) = words.next() {
                BuiltIn(CD(String::from(x)))
            } else {
                //TODO better default
                BuiltIn(CD(env::var("HOME").unwrap_or(String::from("/home/kelton"))))
            }
        }
        Some("exit") => BuiltIn(Exit),
        Some(x) => {
            //this being kludge-y is fine because this will get replaced with a real parser soon:tm:
            let args: Vec<String> = words
                .by_ref()
                .take_while(|w| *w != "|")
                .map(String::from)
                .collect();
            let mut pipe_chain = Vec::new();
            pipe_chain.push(Exec {
                cmd: String::from(x),
                args,
            });
            while let Some(new_cmd) = words.next() {
                let new_args: Vec<String> = words
                    .by_ref()
                    .take_while(|w| *w != "|")
                    .map(String::from)
                    .collect();
                pipe_chain.push(Exec {
                    cmd: new_cmd.to_string(),
                    args: new_args,
                });
            }
            if pipe_chain.len() == 1 {
                //annoying that I have to clone here... easier the messying all the logic with
                //more special cases, though
                pipe_chain[0].clone()
            } else {
                Pipes(pipe_chain)
            }
        }
        None => Empty,
    }
}

fn eval_pipe(res: &ParseResult, state: &mut ShellState) -> io::Result<std::process::Child> {
    match res {
        Pipes(cmds) => {
            let mut cmd_iter = cmds.iter().enumerate();
            let (_, first_cmd) = cmd_iter.next().unwrap();
            let mut last_child = eval_exec(&first_cmd, state, Stdio::inherit(), Stdio::piped())?;
            for (index, item) in cmd_iter {
                let out_pipe = if index <= cmds.len() - 1 {
                    Stdio::inherit()
                } else {
                    Stdio::piped()
                };
                //TODO handle this error
                let last_child_out_pipe = last_child
                    .stdout
                    .expect("failed to get stdout from last child")
                    .into();
                last_child = eval_exec(item, state, last_child_out_pipe, out_pipe)?;
            }
            Ok(last_child)
        }
        _ => panic!("invalid argument to eval_pipe"),
    }
}

fn eval_exec(
    res: &ParseResult,
    state: &mut ShellState,
    in_pipe: Stdio,
    out_pipe: Stdio,
) -> io::Result<std::process::Child> {
    match res {
        Exec { cmd, args } => {
             Command::new(cmd)
                .args(args)
                .current_dir(&state.current_dir)
                .stdin(in_pipe)
                .stdout(out_pipe)
                .spawn()
        }
        _ => panic!("invalid argument to eval_exec"),
    }
}

fn eval(res: &ParseResult, state: &mut ShellState) {
    match res {
        BuiltIn(CD(x)) => {
            //absolute or relative path
            if let Some('/') = x.chars().next() {
                state.current_dir = x.to_string()
            } else {
                if x.ends_with('/') {
                    //not exactly sure why I need to clone here
                    state.current_dir = state.current_dir.clone() + &x.to_string()
                } else {
                    state.current_dir = state.current_dir.clone() + "/" + &x.to_string()
                }
            }
        }
        BuiltIn(Exit) => {
            println!("");
            std::process::exit(0)
        }
        Exec { .. } => {
            let child = eval_exec(res, state, Stdio::inherit(), Stdio::inherit())
                .and_then(|mut x| x.wait())
                .map(|x| x.to_string());
            match child {
                Err(e) => println!("failed to exec with error {:?}", e),
                Ok(s) => println!("successfully exec'd with result {s}"),
            }
        }
        Pipes(_) => {
            let child = eval_pipe(res, state)
                .and_then(|mut x| x.wait())
                .map(|x| x.to_string());
            match child {
                Err(e) => println!("failed to pipe with error {:?}", e),
                Ok(s) => println!("successfully piped with result {s}"),
            }
        }
        Empty => println!(""),
    }
}
