use crate::actions::Action;

use crossterm::event::{self, Event};
use ratatui::{DefaultTerminal, Frame};

use tokio::sync::Mutex;
use tokio::sync::mpsc::Sender;
use tokio::task::{JoinHandle, spawn};

use std::io::Error;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Debug)]
struct State {
    pub sender: Sender<Action>,
}

pub struct App {
    render_loop: JoinHandle<Result<(), String>>,
    state: Arc<Mutex<State>>,
}

impl App {
    pub async fn new(sender: Sender<Action>) -> App {
        let state = Arc::new(Mutex::new(State { sender }));
        let clonned_state = state.clone();
        let render_loop = spawn(App::run(clonned_state));

        App { render_loop, state }
    }

    pub async fn run(state: Arc<Mutex<State>>) -> Result<(), String> {
        let mut terminal = ratatui::init();

        let mut run_result = Ok(());
        loop {
            let clonned_state = state.clone();
            match terminal.draw(move |frame| App::render(frame, clonned_state)) {
                Ok(_) => (),
                Err(message) => {
                    run_result = Err(format!("{:?}", message));
                    break;
                }
            }

            match event::poll(Duration::from_millis(100)) {
                Ok(true) => match event::read() {
                    Ok(Event::Key(press)) => {
                        if press.code == event::KeyCode::Char('q') {
                            match state.lock().await.sender.send(Action::Quit).await {
                                Ok(()) => (),
                                Err(message) => run_result = Err(format!("{:?}", message)),
                            }
                            break;
                        }
                    }
                    _ => (),
                },
                Ok(false) => (),
                Err(message) => {
                    run_result = Err(format!("{:?}", message));
                    break;
                }
            }
        }

        ratatui::restore();
        run_result
    }

    fn render(frame: &mut Frame, state: Arc<Mutex<State>>) {
        frame.render_widget("hello world", frame.area());
    }
}
