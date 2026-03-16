use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{self, ClearType};
use crossterm::{execute, queue};
use rand::Rng;
use std::cmp::max;
use std::io::{self, Write};
use std::time::{Duration, Instant};

const LANES: usize = 6;
const MIN_COLS: u16 = 20;
const MIN_ROWS: u16 = 8;

type AppResult<T> = std::io::Result<T>;

#[derive(Clone, Copy)]
struct Block {
    lane: usize,
    y: u16,
}

struct GameState {
    player_lane: usize,
    blocks: Vec<Block>,
    start_time: Instant,
    last_drop: Instant,
    last_spawn: Instant,
    rows: u16,
    cols: u16,
    lane_width: u16,
    game_over: bool,
    final_score: u64,
}

struct TerminalGuard;

impl TerminalGuard {
    fn new() -> AppResult<Self> {
        terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        let _ = execute!(stdout, terminal::LeaveAlternateScreen, cursor::Show);
        let _ = terminal::disable_raw_mode();
    }
}

fn main() -> AppResult<()> {
    let (cols, rows) = terminal::size()?;
    if cols < MIN_COLS || rows < MIN_ROWS {
        println!(
            "Terminal too small. Need at least {}x{}, got {}x{}.",
            MIN_COLS, MIN_ROWS, cols, rows
        );
        return Ok(());
    }

    let _guard = TerminalGuard::new()?;
    run_game()
}

fn run_game() -> AppResult<()> {
    let (cols, rows) = terminal::size()?;
    let lane_width = max(3, (cols - 2) / LANES as u16);
    let mut state = GameState {
        player_lane: LANES / 2,
        blocks: Vec::new(),
        start_time: Instant::now(),
        last_drop: Instant::now(),
        last_spawn: Instant::now(),
        rows,
        cols,
        lane_width,
        game_over: false,
        final_score: 0,
    };

    let mut stdout = io::stdout();

    loop {
        if handle_input(&mut state)? {
            break;
        }

        if !state.game_over {
            update_game(&mut state);
        }

        draw(&mut stdout, &state)?;
        stdout.flush()?;

        if state.game_over {
            // Slow down the loop when game over to reduce CPU usage.
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    Ok(())
}

fn handle_input(state: &mut GameState) -> AppResult<bool> {
    if event::poll(Duration::from_millis(16))? {
        if let Event::Key(KeyEvent { code, modifiers, .. }) = event::read()? {
            match code {
                KeyCode::Char('q') | KeyCode::Char('Q') => {
                    return Ok(true);
                }
                KeyCode::Char('r') | KeyCode::Char('R') => {
                    if state.game_over {
                        reset_game(state);
                    }
                }
                KeyCode::Left | KeyCode::Char('a') | KeyCode::Char('A') => {
                    if state.player_lane > 0 {
                        state.player_lane -= 1;
                    }
                }
                KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('D') => {
                    if state.player_lane + 1 < LANES {
                        state.player_lane += 1;
                    }
                }
                KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(true);
                }
                _ => {}
            }
        }
    }

    Ok(false)
}

fn update_game(state: &mut GameState) {
    let now = Instant::now();
    let elapsed_secs = now.duration_since(state.start_time).as_secs();
    let drop_interval_ms = max(150, 600_u64.saturating_sub(elapsed_secs * 5));
    let drop_interval = Duration::from_millis(drop_interval_ms);

    if now.duration_since(state.last_drop) >= drop_interval {
        for block in &mut state.blocks {
            block.y = block.y.saturating_add(1);
        }
        state.last_drop = now;
    }

    if now.duration_since(state.last_spawn) >= drop_interval {
        let mut rng = rand::thread_rng();
        let lane = rng.gen_range(0..LANES);
        state.blocks.push(Block { lane, y: 1 });
        state.last_spawn = now;
    }

    let player_y = state.rows.saturating_sub(2);
    state.blocks.retain(|b| b.y <= player_y);

    if state
        .blocks
        .iter()
        .any(|b| b.y == player_y && b.lane == state.player_lane)
    {
        state.game_over = true;
        state.final_score = elapsed_secs;
    }
}

fn draw(stdout: &mut io::Stdout, state: &GameState) -> AppResult<()> {
    queue!(stdout, terminal::Clear(ClearType::All))?;

    let score = if state.game_over {
        state.final_score
    } else {
        Instant::now().duration_since(state.start_time).as_secs()
    };

    let header = format!(
        "Score: {}s  Q=quit  A/D or ←/→ move",
        score
    );
    let header = truncate_to_cols(&header, state.cols as usize);
    queue!(stdout, cursor::MoveTo(0, 0))?;
    queue!(stdout, crossterm::style::Print(header))?;

    let player_y = state.rows.saturating_sub(2);
    let left_margin = 1u16;

    for block in &state.blocks {
        let x = lane_center_x(left_margin, state.lane_width, block.lane, state.cols);
        let y = block.y;
        if y < state.rows {
            queue!(stdout, cursor::MoveTo(x, y))?;
            queue!(stdout, crossterm::style::Print('#'))?;
        }
    }

    let player_x = lane_center_x(left_margin, state.lane_width, state.player_lane, state.cols);
    queue!(stdout, cursor::MoveTo(player_x, player_y))?;
    queue!(stdout, crossterm::style::Print('A'))?;

    if state.game_over {
        let line1 = format!("Game Over! Score: {}s", state.final_score);
        let line2 = "Press R to restart, Q to quit";
        draw_centered(stdout, state.cols, state.rows / 2, &line1)?;
        draw_centered(stdout, state.cols, state.rows / 2 + 1, line2)?;
    }

    Ok(())
}

fn draw_centered(
    stdout: &mut io::Stdout,
    cols: u16,
    row: u16,
    text: &str,
) -> AppResult<()> {
    let len = text.chars().count() as u16;
    let x = cols.saturating_sub(len) / 2;
    queue!(stdout, cursor::MoveTo(x, row))?;
    queue!(stdout, crossterm::style::Print(text))?;
    Ok(())
}

fn lane_center_x(left: u16, lane_width: u16, lane: usize, cols: u16) -> u16 {
    let lane_start = left.saturating_add(lane_width.saturating_mul(lane as u16));
    let x = lane_start.saturating_add(lane_width / 2);
    if x >= cols {
        cols.saturating_sub(1)
    } else {
        x
    }
}

fn truncate_to_cols(text: &str, cols: usize) -> String {
    text.chars().take(cols).collect()
}

fn reset_game(state: &mut GameState) {
    state.player_lane = LANES / 2;
    state.blocks.clear();
    state.start_time = Instant::now();
    state.last_drop = state.start_time;
    state.last_spawn = state.start_time;
    state.game_over = false;
    state.final_score = 0;
}
