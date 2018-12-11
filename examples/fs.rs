#![feature(fnbox)]

use crossbeam_channel::{unbounded, Sender};
use std::fs::File;
use std::io::Read;
use std::boxed::FnBox;
use std::thread;
use failure::Error;

#[derive(Clone)]
pub struct Fs {
    task_sender: Sender<Task>,
}

pub struct FsHandler {
    io_worker: thread::JoinHandle<Result<(), Error>>,
    executor: thread::JoinHandle<Result<(), Error>>,
}

pub fn fs_async() -> (Fs, FsHandler) {
    let (task_sender, task_receiver) = unbounded();
    let (result_sender, result_receiver) = unbounded();
    let io_worker = std::thread::spawn(move || {
        loop {
            match task_receiver.recv() {
                Ok(task) => {
                    match task {
                        Task::Println(ref string) => println!("{}", string),
                        Task::Open(path, callback, fs) => {
                            result_sender
                                .send(TaskResult::Open(File::open(path).unwrap(), callback, fs))?
                        }
                        Task::ReadToString(mut file, callback, fs) => {
                            let mut value = String::new();
                            file.read_to_string(&mut value).unwrap();
                            result_sender
                                .send(TaskResult::ReadToString(value, callback, fs))?
                        }
                        Task::Exit => {
                            result_sender
                                .send(TaskResult::Exit)?;
                            break;
                        }
                    }
                }
                Err(_) => {
                    break;
                }
            }
        }
        Ok(())
    });
    let executor = std::thread::spawn(move || {
        loop {
            let result = result_receiver.recv()?;
            match result {
                TaskResult::ReadToString(value, callback, fs) => callback(value, fs),
                TaskResult::Open(file, callback, fs) => callback(file, fs),
                TaskResult::Exit => break
            };
        };
        Ok(())
    });

    (Fs { task_sender }, FsHandler { io_worker, executor })
}

impl Fs {
    pub fn println(&self, string: String) -> Result<(), Error> {
        Ok(self.task_sender.send(Task::Println(string))?)
    }

    pub fn open<F>(&self, path: &str, callback: F) -> Result<(), Error>
        where F: FnOnce(File, Fs) -> Result<(), Error> + Send + 'static {
        Ok(self.task_sender.send(Task::Open(path.to_string(), Box::new(callback), self.clone()))?)
    }

    pub fn read_to_string<F>(&self, file: File, callback: F) -> Result<(), Error>
        where F: FnOnce(String, Fs) -> Result<(), Error> + Send + 'static {
        Ok(self.task_sender.send(Task::ReadToString(file, Box::new(callback), self.clone()))?)
    }

    pub fn close(&self) -> Result<(), Error> {
        Ok(self.task_sender.send(Task::Exit)?)
    }
}

impl FsHandler {
    pub fn join(self) -> Result<(), Error> {
        self.io_worker.join().unwrap()?;
        self.executor.join().unwrap()
    }
}

type FileCallback = Box<FnBox(File, Fs) -> Result<(), Error> + Send>;
type StringCallback = Box<FnBox(String, Fs) -> Result<(), Error> + Send>;

pub enum Task {
    Exit,
    Println(String),
    Open(String, FileCallback, Fs),
    ReadToString(File, StringCallback, Fs),
}

pub enum TaskResult {
    Exit,
    Open(File, FileCallback, Fs),
    ReadToString(String, StringCallback, Fs),
}


const TEST_FILE_VALUE: &str = "Hello, World!";

fn main() -> Result<(), Error> {
    let (fs, fs_handler) = fs_async();
    fs.open("./examples/test.txt", |file, fs| {
        fs.read_to_string(file, |value, fs| {
            assert_eq!(TEST_FILE_VALUE, &value);
            fs.println(value)?;
            fs.close()
        })
    })?;
    fs_handler.join()?;
    Ok(())
}