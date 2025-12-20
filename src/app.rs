use crate::actions::Action;
use crate::pipeline::{SplattedBlocks, SplattedDepth, SplattedVolumes};

use crossterm::event::{self, Event};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::widgets::{Block, Tabs};

use tokio::sync::Mutex;
use tokio::sync::mpsc::Sender;
use tokio::task::{JoinHandle, spawn};
use tokio::time::{Duration, interval};

use std::sync::Arc;

#[derive(Debug, Clone)]
enum Page {
    Search,
    Logs,
    Ticker,
}

#[derive(Clone, Debug)]
struct State {
    pub page: Page,
    pub sender: Sender<Action>,
    pub tickers: Option<Vec<String>>,
    pub current_ticker: Option<String>,
    pub depth: Option<SplattedDepth>,
    pub volumes: Option<SplattedVolumes>,
    pub blocks: Option<SplattedBlocks>,
}

pub struct App {
    render_loop: JoinHandle<Result<(), String>>,
    pipeline_request_loop: JoinHandle<Result<(), String>>,
    state: Arc<Mutex<State>>,
}

impl App {
    pub async fn new(sender: Sender<Action>) -> App {
        let state = Arc::new(Mutex::new(State {
            page: Page::Search,
            sender: sender.clone(),
            tickers: None,
            current_ticker: None,
            depth: None,
            volumes: None,
            blocks: None,
        }));
        let clonned_state = state.clone();
        let render_loop = spawn(App::run(clonned_state));

        let clonned_sender = sender.clone();
        let clonned_state = state.clone();
        let pipeline_request_loop = spawn(App::request_pipeline(clonned_sender, clonned_state));

        App {
            render_loop,
            pipeline_request_loop,
            state,
        }
    }

    pub async fn update_splats(&self, splats: (SplattedDepth, SplattedVolumes, SplattedBlocks)) {
        let mut locked_state = self.state.lock().await;
        locked_state.depth = Some(splats.0);
        locked_state.volumes = Some(splats.1);
        locked_state.blocks = Some(splats.2);
    }

    async fn request_pipeline(
        sender: Sender<Action>,
        state: Arc<Mutex<State>>,
    ) -> Result<(), String> {
        let mut timer = interval(Duration::from_secs(1));
        loop {
            timer.tick().await;
            match &state.lock().await.current_ticker {
                Some(symbol) => match sender.send(Action::RunPipeline(symbol.clone())).await {
                    Ok(()) => (),
                    Err(message) => return Err(format!("{:?}", message)),
                },
                None => (),
            }
        }
    }

    async fn run(state: Arc<Mutex<State>>) -> Result<(), String> {
        let mut terminal = ratatui::init();

        let mut run_result = Ok(());
        loop {
            let clonned_state = state.lock().await.clone();
            match terminal.draw(|frame| App::render(frame, clonned_state)) {
                Ok(_) => (),
                Err(message) => {
                    run_result = Err(format!("{:?}", message));
                    break;
                }
            }

            match event::poll(std::time::Duration::from_millis(100)) {
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

    fn render(frame: &mut Frame, state: State) {
        let top_block = Block::bordered().title("bookedblocks");

        match state.page {
            Page::Search => {
                let vchunks = Layout::vertical(vec![
                    Constraint::Percentage(40),
                    Constraint::Percentage(20),
                    Constraint::Percentage(40),
                ])
                .split(frame.area());

                let hchunks = Layout::horizontal(vec![
                    Constraint::Percentage(5),
                    Constraint::Percentage(90),
                    Constraint::Percentage(5),
                ])
                .split(vchunks[1]);

                frame.render_widget(Block::bordered().title("Search"), hchunks[1]);
            }
            Page::Ticker => match (state.tickers.clone(), state.current_ticker.clone()) {
                (Some(tickers), Some(current)) => {
                    let tab_index = match tickers.iter().position(|ticker| *ticker == current) {
                        Some(found) => found,
                        None => 0,
                    };
                    let tabs = Tabs::new(tickers).select(tab_index);

                    frame.render_widget(tabs, frame.area());
                }
                _ => (),
            },
            Page::Logs => (),
        };

        frame.render_widget(top_block, frame.area())
    }
}
