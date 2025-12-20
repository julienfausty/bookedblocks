use tokio::sync::Mutex;
use tokio::task::{JoinHandle, spawn};

use std::sync::Arc;

#[derive(Clone, Debug)]
struct State {}

pub struct App {
    render_loop: JoinHandle<Result<(), String>>,
    state: Arc<Mutex<State>>,
}

impl App {
    pub async fn new() -> App {
        let state = Arc::new(Mutex::new(State {}));
        let clonned_state = state.clone();
        let render_loop = spawn(App::run(clonned_state));

        App { render_loop, state }
    }

    pub async fn run(state: Arc<Mutex<State>>) -> Result<(), String> {}
}
