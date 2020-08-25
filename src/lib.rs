use json::JsonValue;
use lazy_static::lazy_static;
use nannou::app;
use nannou::prelude::*;
use std::collections::HashSet;
use std::env;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{
    mpsc::{channel, Receiver},
    Mutex,
};
use std::thread::{self, JoinHandle};

mod dependency_tree;
use dependency_tree::{crate_name_from_package_id, Dependency, DependencyTree};

mod drawing;
use drawing::{draw_deps, find_dep_by_pos, Phase, Point, Transition};

enum CargoTask {
    Active(String),
    Completed(String),
}

lazy_static! {
    static ref COMPLETED_RECEIVER: Mutex<Option<(JoinHandle<()>, Receiver<CargoTask>, DependencyTree)>> =
        Mutex::new(None);
}

pub fn launch(cargo_command: Vec<&'static str>, prefix: &'static str) {
    let (sender, receiver) = channel();
    let parsed_tree = match DependencyTree::new(Path::new(".")) {
        Ok(parsed_tree) => parsed_tree,
        Err((exit_code, error)) => {
            eprint!("{}", error);
            std::process::exit(exit_code);
        }
    };

    let thread = thread::spawn(move || {
        let build_args: Vec<_> = cargo_command
            .iter()
            .map(|x| x.to_string())
            .chain(env::args().skip(2))
            .collect();

        // TODO: right now we just wait for cargo to exit.
        // Instead of waiting, kill cargo when the window is closed.
        let cargo_proc = Command::new("cargo")
            .args(build_args)
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("Failed to run cargo");

        let stderr = cargo_proc.stderr.expect("Failed to get cargo stderr");
        let stdout = cargo_proc.stdout.expect("Failed to get cargo stdout");

        let stdout_reader = thread::spawn({
            let sender = sender.clone();
            move || {
                for line in BufReader::new(stdout).lines() {
                    if let Ok(line) = line {
                        // With --message-format=json, cargo prints useful build
                        // steps into stdout. Parse those as we encounter them.
                        if !line.starts_with("{") {
                            println!("{}", line);
                            continue;
                        }

                        if let Ok(line) = json::parse(&line) {
                            if line["reason"] != "compiler-artifact" {
                                continue;
                            }

                            if let JsonValue::String(line) = &line["package_id"] {
                                let completed = crate_name_from_package_id(&line);
                                sender
                                    .send(CargoTask::Completed(completed))
                                    .expect("Failed to send completion to channel");
                            }
                        }
                    }
                }
            }
        });

        for line in BufReader::new(stderr).lines() {
            if let Ok(line) = line {
                eprintln!("{}", line);

                if !line.starts_with(prefix) {
                    continue;
                }

                // Skip the prefix when constructing the crate name
                let active = crate_name_from_package_id(line[prefix.len()..].trim());
                sender
                    .send(CargoTask::Active(active))
                    .expect("Failed to send activation to channel");
            }
        }

        let _ = stdout_reader.join();
    });

    COMPLETED_RECEIVER
        .lock()
        .expect("Failed to take the receiver lock")
        .replace((thread, receiver, parsed_tree));

    nannou::app(model).update(update).run();
}

struct Model {
    tree: DependencyTree,
    mouse_last: Option<Point>,
    active_tree: Option<String>,
    active_tasks: HashSet<String>,
    completed_tasks: HashSet<String>,
    sender_thread: Option<JoinHandle<()>>,
    tasks_receiver: std::sync::mpsc::Receiver<CargoTask>,
}

impl Model {
    fn current_root(&self) -> Dependency {
        match &self.active_tree {
            None => self.tree.root(),
            Some(active) => self.tree.get(active).unwrap(),
        }
    }
}

fn model(app: &App) -> Model {
    app.new_window().event(event).view(view).build().unwrap();

    let (sender_thread, tasks_receiver, parsed_tree) = COMPLETED_RECEIVER
        .lock()
        .expect("Failed to take receiver lock")
        .take()
        .expect("Failed to unpack initial state");

    Model {
        tree: parsed_tree,
        mouse_last: None,
        active_tree: None,
        active_tasks: HashSet::new(),
        completed_tasks: HashSet::new(),
        sender_thread: Some(sender_thread),
        tasks_receiver,
    }
}

fn update(_app: &App, model: &mut Model, _update: Update) {
    if let Ok(new_task) = model.tasks_receiver.try_recv() {
        match new_task {
            CargoTask::Active(task) => {
                model.active_tasks.insert(task);
            }
            CargoTask::Completed(task) => {
                model.active_tasks.remove(&task);
                model.completed_tasks.insert(task);
            }
        }
    }
}

fn event(app: &App, model: &mut Model, event: WindowEvent) {
    match event {
        // Keyboard events
        KeyPressed(_key) => model.active_tree = None,

        // Mouse events
        MouseMoved(_pos) => model.mouse_last = Some(Point(_pos.x, _pos.y)),
        MouseReleased(_button) => {
            if let Some(mouse_last) = model.mouse_last {
                if let Some(active) =
                    find_dep_by_pos(model.current_root(), mouse_last, get_phase(app.time))
                {
                    model.active_tree = Some(active);
                }
            }
        }

        // Touch events
        Touch(_touch) => {}
        TouchPressure(_pressure) => {}

        Closed => {
            model.sender_thread.take().map(|thread| thread.join());
        }

        _ => {}
    }
}

fn view(app: &App, model: &Model, frame: Frame) {
    let draw = app.draw();

    draw.background().color(BLACK);
    draw_deps(
        &draw,
        get_phase(app.time),
        get_transition(app.time),
        model.current_root(),
        &model.completed_tasks,
        &model.active_tasks,
    );
    draw.to_frame(app, &frame).unwrap();
}

fn get_phase(time: app::DrawScalar) -> Phase {
    time.sin() * 0.1
}

fn get_transition(time: app::DrawScalar) -> Transition {
    time.sin().abs()
}
