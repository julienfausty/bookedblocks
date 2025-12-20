use crate::actions::Action;
use crate::pipeline::{SplattedBlocks, SplattedDepth, SplattedVolumes};

use crossterm::event::{self, Event};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::Stylize;
use ratatui::symbols;
use ratatui::widgets::{Axis, Block, Chart, Dataset, GraphType, Paragraph};

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
            page: Page::Ticker,
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

    pub async fn set_current_ticker(&self, ticker: String) {
        let mut locked_state = self.state.lock().await;
        locked_state.current_ticker = Some(ticker.clone());
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
            Page::Ticker => match state.current_ticker {
                Some(symbol) => {
                    let vchunks = Layout::vertical(vec![
                        Constraint::Percentage(2),
                        Constraint::Percentage(96),
                        Constraint::Percentage(2),
                    ])
                    .split(frame.area());

                    let hchunks = Layout::horizontal(vec![
                        Constraint::Percentage(2),
                        Constraint::Percentage(96),
                        Constraint::Percentage(2),
                    ])
                    .split(vchunks[1]);

                    let ticker_block = Block::bordered().title(symbol.clone());
                    frame.render_widget(ticker_block, hchunks[1]);

                    let data_chunk = Layout::vertical(vec![
                        Constraint::Percentage(2),
                        Constraint::Percentage(96),
                        Constraint::Percentage(2),
                    ])
                    .split(
                        Layout::horizontal(vec![
                            Constraint::Percentage(2),
                            Constraint::Percentage(96),
                            Constraint::Percentage(2),
                        ])
                        .split(hchunks[1])[1],
                    )[1];

                    let vertical_data_chunks = Layout::vertical(vec![
                        Constraint::Percentage(65),
                        Constraint::Percentage(10),
                        Constraint::Percentage(25),
                    ])
                    .split(data_chunk);

                    let top_data_chunks = Layout::horizontal(vec![
                        Constraint::Percentage(65),
                        Constraint::Percentage(10),
                        Constraint::Percentage(25),
                    ])
                    .split(vertical_data_chunks[0]);

                    let bottom_data_chunks = Layout::horizontal(vec![
                        Constraint::Percentage(65),
                        Constraint::Percentage(10),
                        Constraint::Percentage(25),
                    ])
                    .split(vertical_data_chunks[2]);

                    match state.depth {
                        Some(splatted) => {
                            let x_axis = Axis::default()
                                .title("Price")
                                .bounds([splatted.price_range.0, splatted.price_range.1])
                                .labels([
                                    format!("{:}", splatted.price_range.0),
                                    format!(
                                        "{:}",
                                        (splatted.price_range.0 + splatted.price_range.1) / 2.0
                                    ),
                                    format!("{:}", splatted.price_range.1),
                                ]);

                            let max_vol = splatted.volumes.iter().fold(f64::MIN, |acc, volume| {
                                if acc < *volume { volume.clone() } else { acc }
                            });
                            let min_vol = splatted.volumes.iter().fold(f64::MAX, |acc, volume| {
                                if acc > *volume { volume.clone() } else { acc }
                            });
                            let y_axis = Axis::default()
                                .title("Volumes")
                                .bounds([min_vol, max_vol])
                                .labels([
                                    format!("{:}", min_vol),
                                    format!("{:}", (min_vol + max_vol) / 2.0),
                                    format!("{:}", max_vol),
                                ]);

                            let step = (splatted.price_range.1 - splatted.price_range.0)
                                / (splatted.volumes.len() as f64);
                            let graph = splatted
                                .volumes
                                .clone()
                                .into_iter()
                                .enumerate()
                                .map(|(index, vol)| {
                                    (((index as f64) * step) + splatted.price_range.0, vol)
                                })
                                .collect::<Vec<_>>();

                            let depth_dataset = Dataset::default()
                                .name("Depth")
                                .data(&graph)
                                .marker(symbols::Marker::HalfBlock)
                                .graph_type(GraphType::Bar)
                                .green();

                            let depth_widget = Chart::new(vec![depth_dataset])
                                .block(Block::bordered().title("Depth"))
                                .x_axis(x_axis)
                                .y_axis(y_axis);
                            frame.render_widget(depth_widget, top_data_chunks[2]);
                        }
                        None => {
                            frame.render_widget(
                                Paragraph::new("Loading...").alignment(Alignment::Center),
                                top_data_chunks[2],
                            );
                        }
                    }
                }
                None => frame.render_widget(
                    Paragraph::new("Loading...").alignment(Alignment::Center),
                    frame.area(),
                ),
            },
            Page::Logs => (),
        };

        frame.render_widget(top_block, frame.area())
    }
}
