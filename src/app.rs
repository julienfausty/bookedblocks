use crate::actions::Action;
use crate::feed::TickerState;
use crate::pipeline::{SplattedBlocks, SplattedDepth, SplattedVolumes};

use crossterm::event::{self, Event};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Color, Style, Stylize};
use ratatui::symbols;
use ratatui::text::Text;
use ratatui::widgets::{Axis, Block, Chart, Dataset, GraphType, Paragraph, Widget};

use tokio::sync::Mutex;
use tokio::sync::mpsc::Sender;
use tokio::task::{JoinHandle, spawn};
use tokio::time::{Duration, interval};

use std::collections::HashMap;
use std::sync::Arc;

/// Enum of different pages one could move to in application
#[derive(Debug, Clone)]
pub enum Page {
    Search,
    Logs,
    Ticker,
}

/// State data structure relevant to rendering interface
#[derive(Clone, Debug)]
pub struct State {
    pub page: Page,
    pub sender: Sender<Action>,
    pub tickers: Option<Vec<String>>,
    pub current_ticker: Option<String>,
    pub ticker_data: Option<TickerState>,
    pub depth: Option<SplattedDepth>,
    pub volumes: Option<SplattedVolumes>,
    pub blocks: Option<SplattedBlocks>,
}

/// Widget for rendering TickerState in interface
struct TickerWidget {
    state: TickerState,
}

impl TickerWidget {
    /// constructor
    pub fn new(state: TickerState) -> TickerWidget {
        TickerWidget { state }
    }
}

impl Widget for TickerWidget {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let vchunks = Layout::vertical(vec![
            Constraint::Percentage(2),
            Constraint::Percentage(48),
            Constraint::Percentage(48),
            Constraint::Percentage(2),
        ])
        .split(area.clone());

        let top_chunks = Layout::horizontal(vec![
            Constraint::Percentage(2),
            Constraint::Percentage(24),
            Constraint::Percentage(24),
            Constraint::Percentage(24),
            Constraint::Percentage(24),
            Constraint::Percentage(2),
        ])
        .split(vchunks[1]);

        let bottom_chunks = Layout::horizontal(vec![
            Constraint::Percentage(2),
            Constraint::Percentage(24),
            Constraint::Percentage(24),
            Constraint::Percentage(24),
            Constraint::Percentage(24),
            Constraint::Percentage(2),
        ])
        .split(vchunks[2]);

        let green_bold = Style::new().green().bold();
        let red_bold = Style::new().red().bold();
        let just_bold = Style::new().bold();

        let ask_widget = Paragraph::new(
            Text::from(format!("{:}", self.state.ask))
                .alignment(Alignment::Center)
                .style(green_bold.clone()),
        )
        .block(Block::bordered().title("Ask"))
        .alignment(Alignment::Center);

        ask_widget.render(top_chunks[1], buf);

        let bid_widget = Paragraph::new(
            Text::from(format!("{:}", self.state.bid))
                .alignment(Alignment::Center)
                .style(red_bold.clone()),
        )
        .block(Block::bordered().title("Bid"))
        .alignment(Alignment::Center);

        bid_widget.render(bottom_chunks[1], buf);

        let change_widget = Paragraph::new(
            Text::from(format!(
                "{:} %\n{:}",
                self.state.change_pct, self.state.change
            ))
            .alignment(Alignment::Center)
            .style(if self.state.change < 0.0 {
                red_bold.clone()
            } else {
                green_bold.clone()
            }),
        )
        .block(Block::bordered().title("24hr Change"))
        .alignment(Alignment::Center);

        change_widget.render(top_chunks[2], buf);

        let last_widget = Paragraph::new(
            Text::from(format!("{:}", self.state.last))
                .alignment(Alignment::Center)
                .style(just_bold.clone()),
        )
        .block(Block::bordered().title("Last Trade"))
        .alignment(Alignment::Center);

        last_widget.render(bottom_chunks[2], buf);

        let high_widget = Paragraph::new(
            Text::from(format!("{:}", self.state.high))
                .alignment(Alignment::Center)
                .style(green_bold.clone()),
        )
        .block(Block::bordered().title("High"))
        .alignment(Alignment::Center);

        high_widget.render(top_chunks[3], buf);

        let low_widget = Paragraph::new(
            Text::from(format!("{:}", self.state.low))
                .alignment(Alignment::Center)
                .style(red_bold.clone()),
        )
        .block(Block::bordered().title("Low"))
        .alignment(Alignment::Center);

        low_widget.render(bottom_chunks[3], buf);

        let volume_widget = Paragraph::new(
            Text::from(format!("{:}", self.state.volume))
                .alignment(Alignment::Center)
                .style(just_bold.clone()),
        )
        .block(Block::bordered().title("Volume"))
        .alignment(Alignment::Center);

        volume_widget.render(top_chunks[4], buf);

        let vwap_widget = Paragraph::new(
            Text::from(format!("{:}", self.state.vwap))
                .alignment(Alignment::Center)
                .style(just_bold.clone()),
        )
        .block(Block::bordered().title("VWAP"))
        .alignment(Alignment::Center);

        vwap_widget.render(bottom_chunks[4], buf);
    }
}

/// Widget for rendering market depth to interface
struct DepthWidget {
    depth: SplattedDepth,
}

impl DepthWidget {
    pub fn new(depth: SplattedDepth) -> DepthWidget {
        DepthWidget { depth }
    }
}

impl Widget for DepthWidget {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let x_axis = Axis::default()
            .title("Price")
            .bounds([self.depth.price_range.0, self.depth.price_range.1])
            .labels([
                format!("{:}", self.depth.price_range.0),
                format!(
                    "{:}",
                    (self.depth.price_range.0 + self.depth.price_range.1) / 2.0
                ),
                format!("{:}", self.depth.price_range.1),
            ]);

        let max_vol = self.depth.volumes.iter().fold(f64::MIN, |acc, volume| {
            if acc < volume.abs() {
                volume.clone()
            } else {
                acc
            }
        });

        let y_axis = Axis::default()
            .title("Volumes")
            .bounds([-max_vol, max_vol])
            .labels([
                format!("{:}", max_vol),
                format!("0.0"),
                format!("{:}", max_vol),
            ]);

        let step = (self.depth.price_range.1 - self.depth.price_range.0)
            / (self.depth.volumes.len() as f64);
        let ask_graph = self
            .depth
            .volumes
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, vol)| {
                (
                    ((index as f64) * step) + self.depth.price_range.0,
                    if vol > 0.0 { vol } else { 0.0 },
                )
            })
            .filter(|(_, vol)| *vol != 0.0)
            .collect::<Vec<_>>();

        let ask_dataset = Dataset::default()
            .name("Asks")
            .data(&ask_graph)
            .marker(symbols::Marker::HalfBlock)
            .graph_type(GraphType::Bar)
            .green();

        let bid_graph = self
            .depth
            .volumes
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, vol)| {
                (
                    ((index as f64) * step) + self.depth.price_range.0,
                    if vol < 0.0 { vol } else { 0.0 },
                )
            })
            .filter(|(_, vol)| *vol != 0.0)
            .collect::<Vec<_>>();

        let bid_dataset = Dataset::default()
            .name("Bids")
            .data(&bid_graph)
            .marker(symbols::Marker::HalfBlock)
            .graph_type(GraphType::Bar)
            .red();

        let chart = Chart::new(vec![ask_dataset, bid_dataset])
            .block(Block::bordered().title("Depth"))
            .x_axis(x_axis)
            .y_axis(y_axis);

        chart.render(area, buf)
    }
}

/// Widget for rendering market volumes to interface
struct VolumeWidget {
    volumes: SplattedVolumes,
}

impl VolumeWidget {
    pub fn new(volumes: SplattedVolumes) -> VolumeWidget {
        VolumeWidget { volumes }
    }
}

impl Widget for VolumeWidget {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let x_axis = Axis::default()
            .title("Time (s)")
            .bounds([
                self.volumes.time_range.0 as f64,
                self.volumes.time_range.1 as f64,
            ])
            .labels([
                format!("{:}", self.volumes.time_range.1 - self.volumes.time_range.0),
                format!(
                    "{:}",
                    (self.volumes.time_range.1 - self.volumes.time_range.0) / 2
                ),
                "now".to_string(),
            ]);

        let max_vol = self
            .volumes
            .ask_volumes
            .iter()
            .fold(f64::MIN, |acc, volume| {
                if acc < volume.abs() {
                    volume.clone()
                } else {
                    acc
                }
            });

        let max_vol = self
            .volumes
            .bid_volumes
            .iter()
            .fold(max_vol, |acc, volume| {
                if acc < volume.abs() {
                    volume.clone()
                } else {
                    acc
                }
            });

        let y_axis = Axis::default()
            .title("Volumes")
            .bounds([-max_vol, max_vol])
            .labels([
                format!("{:}", max_vol),
                format!("0.0"),
                format!("{:}", max_vol),
            ]);

        let step = ((self.volumes.time_range.1 - self.volumes.time_range.0) as f64)
            / (self.volumes.ask_volumes.len() as f64);
        let ask_graph = self
            .volumes
            .ask_volumes
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, vol)| {
                (
                    ((index as f64) * step) + self.volumes.time_range.0 as f64,
                    vol,
                )
            })
            .filter(|(_, vol)| *vol != 0.0)
            .collect::<Vec<_>>();

        let ask_dataset = Dataset::default()
            .name("Asks")
            .data(&ask_graph)
            .marker(symbols::Marker::HalfBlock)
            .graph_type(GraphType::Bar)
            .green();

        let bid_graph = self
            .volumes
            .bid_volumes
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, vol)| {
                (
                    ((index as f64) * step) + self.volumes.time_range.0 as f64,
                    -vol,
                )
            })
            .filter(|(_, vol)| *vol != 0.0)
            .collect::<Vec<_>>();

        let bid_dataset = Dataset::default()
            .name("Bids")
            .data(&bid_graph)
            .marker(symbols::Marker::HalfBlock)
            .graph_type(GraphType::Bar)
            .red();

        let chart = Chart::new(vec![bid_dataset, ask_dataset])
            .block(Block::bordered().title("Order Volumes"))
            .x_axis(x_axis)
            .y_axis(y_axis);

        chart.render(area, buf)
    }
}

/// Widget for rendering order book heat map to interface
struct HeatMapWidget {
    blocks: SplattedBlocks,
}

impl HeatMapWidget {
    pub fn new(blocks: SplattedBlocks) -> HeatMapWidget {
        HeatMapWidget { blocks }
    }
}

impl Widget for HeatMapWidget {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let x_axis = Axis::default()
            .title("Time (s)")
            .bounds([
                self.blocks.grid.time_range.0 as f64,
                self.blocks.grid.time_range.1 as f64,
            ])
            .labels([
                format!(
                    "{:}",
                    self.blocks.grid.time_range.1 - self.blocks.grid.time_range.0
                ),
                format!(
                    "{:}",
                    (self.blocks.grid.time_range.1 - self.blocks.grid.time_range.0) / 2
                ),
                "now".to_string(),
            ]);

        let y_axis = Axis::default()
            .title("Price")
            .bounds([
                self.blocks.grid.price_range.0,
                self.blocks.grid.price_range.1,
            ])
            .labels([
                format!("{:}", self.blocks.grid.price_range.0),
                format!(
                    "{:}",
                    (self.blocks.grid.price_range.1 + self.blocks.grid.price_range.0) / 2.0
                ),
                format!("{:}", self.blocks.grid.price_range.1),
            ]);

        let max_vol =
            self.blocks.volumes.iter().fold(
                0.0,
                |acc, vol| if acc < vol.abs() { vol.abs() } else { acc },
            );
        let color_map = |vol: f64| {
            if vol < 0.0 {
                Color::Rgb(
                    (((vol.abs() / max_vol) * 9.0 + 1.0).round() * 25.5) as u8,
                    0,
                    0,
                )
            } else {
                Color::Rgb(
                    0,
                    (((vol.abs() / max_vol) * 9.0 + 1.0).round() * 25.5) as u8,
                    0,
                )
            }
        };

        let mut layered_points: HashMap<Color, Vec<(f64, f64)>> = HashMap::new();

        let time_step = (self.blocks.grid.time_range.1 - self.blocks.grid.time_range.0) as f64
            / (self.blocks.volumes.shape()[0] as f64);
        let price_step = (self.blocks.grid.price_range.1 - self.blocks.grid.price_range.0)
            / (self.blocks.volumes.shape()[1] as f64);

        for (t_grid, row) in self.blocks.volumes.rows().into_iter().enumerate() {
            for (p_grid, volume) in row.into_iter().enumerate() {
                if volume.abs() >= 0.001 * max_vol {
                    let color = color_map(*volume);
                    let point = (
                        time_step * t_grid as f64 + self.blocks.grid.time_range.0 as f64,
                        price_step * p_grid as f64 + self.blocks.grid.price_range.0 as f64,
                    );
                    if let Some(points) = layered_points.get_mut(&color) {
                        points.push(point);
                    } else {
                        layered_points.insert(color, vec![point]);
                    }
                }
            }
        }

        let mut sorted_points = layered_points
            .into_iter()
            .map(|(color, points)| {
                let (red, green) = match color.clone() {
                    Color::Rgb(red, green, _) => (red, green),
                    _ => (0, 0),
                };
                (red as u16 + green as u16, color, points)
            })
            .collect::<Vec<_>>();
        sorted_points.sort_by(|lhs, rhs| lhs.0.cmp(&rhs.0));

        let datasets = sorted_points
            .iter()
            .map(|(_, color, points)| {
                Dataset::default()
                    .data(points)
                    .marker(symbols::Marker::HalfBlock)
                    .graph_type(GraphType::Scatter)
                    .style(color.clone())
            })
            .collect::<Vec<_>>();

        let chart = Chart::new(datasets)
            .block(Block::bordered().title("Order Map"))
            .x_axis(x_axis)
            .y_axis(y_axis);

        chart.render(area, buf)
    }
}

/// Encapsulation structure for handling user interface
pub struct App {
    render_loop: JoinHandle<Result<(), String>>,
    pipeline_request_loop: JoinHandle<Result<(), String>>,
    state: Arc<Mutex<State>>,
}

impl App {
    /// constructor
    pub async fn new(sender: Sender<Action>) -> App {
        let state = Arc::new(Mutex::new(State {
            page: Page::Ticker,
            sender: sender.clone(),
            tickers: None,
            current_ticker: None,
            ticker_data: None,
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

    /// Set the current ticker in the application state
    pub async fn set_current_ticker(&self, ticker: String) {
        let mut locked_state = self.state.lock().await;
        locked_state.current_ticker = Some(ticker.clone());
    }

    /// Get the state object used for rendering
    pub fn get_state(&self) -> Arc<Mutex<State>> {
        self.state.clone()
    }

    /// Method to request pipeline updates regularly and keep state data updated
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

    /// Run rendering loop for user interface
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

    /// Render single frame using provided state object
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
                        Constraint::Percentage(35),
                    ])
                    .split(data_chunk);

                    let top_data_chunks = Layout::horizontal(vec![
                        Constraint::Percentage(65),
                        Constraint::Percentage(35),
                    ])
                    .split(vertical_data_chunks[0]);

                    let bottom_data_chunks = Layout::horizontal(vec![
                        Constraint::Percentage(65),
                        Constraint::Percentage(35),
                    ])
                    .split(vertical_data_chunks[1]);

                    match state.depth {
                        Some(splatted) => {
                            let depth_widget = DepthWidget::new(splatted);
                            frame.render_widget(depth_widget, top_data_chunks[1]);
                        }
                        None => {
                            frame.render_widget(
                                Paragraph::new("Loading...").alignment(Alignment::Center),
                                top_data_chunks[1],
                            );
                        }
                    }

                    match state.volumes {
                        Some(splatted) => {
                            let volume_widget = VolumeWidget::new(splatted);
                            frame.render_widget(volume_widget, bottom_data_chunks[0]);
                        }
                        None => {
                            frame.render_widget(
                                Paragraph::new("Loading...").alignment(Alignment::Center),
                                bottom_data_chunks[0],
                            );
                        }
                    }

                    match state.blocks {
                        Some(splatted) => {
                            let blocks_widget = HeatMapWidget::new(splatted);
                            frame.render_widget(blocks_widget, top_data_chunks[0]);
                        }
                        None => {
                            frame.render_widget(
                                Paragraph::new("Loading...").alignment(Alignment::Center),
                                top_data_chunks[0],
                            );
                        }
                    }

                    match state.ticker_data {
                        Some(ticker) => {
                            let ticker_widget = TickerWidget::new(ticker);
                            frame.render_widget(ticker_widget, bottom_data_chunks[1]);
                        }
                        None => {
                            frame.render_widget(
                                Paragraph::new("Loading...").alignment(Alignment::Center),
                                bottom_data_chunks[1],
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
