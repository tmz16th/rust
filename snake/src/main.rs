// Snake -- a small game in a real window.
//
// Controls:
//   Arrow keys or WASD  -- steer
//   R                   -- restart after losing
//   Escape              -- quit
//
// How the drawing works: minifb gives us a window and a pixel buffer,
// a `Vec<u32>` with one entry per pixel in 0xRRGGBB order. Every frame we
// write colours into that buffer and hand it back to the window.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use font8x8::{BASIC_FONTS, UnicodeFonts};
use minifb::{Key, KeyRepeat, Window, WindowOptions};
use rand::prelude::*;

/// The board is a grid of square cells, `COLS` wide and `ROWS` tall.
const CELL: usize = 20;
const COLS: usize = 32;
const ROWS: usize = 24;

/// The window is exactly big enough to hold the grid.
const WIDTH: usize = COLS * CELL;
const HEIGHT: usize = ROWS * CELL;

/// How long between snake moves. Smaller means faster.
const MOVE_EVERY: Duration = Duration::from_millis(110);

/// Pack three colour channels into the 0xRRGGBB number the window expects.
const fn rgb(red: u32, green: u32, blue: u32) -> u32 {
    (red << 16) | (green << 8) | blue
}

const BACKGROUND: u32 = rgb(0x10, 0x1C, 0x14);
const GRID: u32 = rgb(0x18, 0x2A, 0x1E);
const SNAKE_HEAD: u32 = rgb(0x70, 0xE0, 0x70);
const SNAKE_BODY: u32 = rgb(0x38, 0xA0, 0x48);
const FOOD: u32 = rgb(0xE0, 0x50, 0x40);
const TEXT: u32 = rgb(0xE8, 0xF0, 0xE8);
const OVERLAY: u32 = rgb(0x06, 0x0A, 0x08);

/// The four ways the snake can travel.
#[derive(Clone, Copy, PartialEq)]
enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    /// The grid offset this direction moves by.
    fn delta(self) -> (i32, i32) {
        match self {
            Direction::Up => (0, -1),
            Direction::Down => (0, 1),
            Direction::Left => (-1, 0),
            Direction::Right => (1, 0),
        }
    }

    /// True if `other` is the reverse of `self`. The snake may not do that.
    fn is_opposite(self, other: Direction) -> bool {
        matches!(
            (self, other),
            (Direction::Up, Direction::Down)
                | (Direction::Down, Direction::Up)
                | (Direction::Left, Direction::Right)
                | (Direction::Right, Direction::Left)
        )
    }
}

/// All the state of one game.
struct Game {
    /// The cells the snake occupies. The front is the head.
    snake: VecDeque<(i32, i32)>,
    /// The direction the snake is actually moving this tick.
    direction: Direction,
    /// The direction the player wants. It is applied on the next tick.
    next_direction: Direction,
    /// Where the next piece of food is.
    food: (i32, i32),
    score: u32,
    alive: bool,
}

impl Game {
    /// Start a fresh game: a length-3 snake in the middle, moving right.
    fn new() -> Self {
        let head_x = (COLS / 2) as i32;
        let head_y = (ROWS / 2) as i32;

        let mut snake = VecDeque::new();
        snake.push_back((head_x, head_y));
        snake.push_back((head_x - 1, head_y));
        snake.push_back((head_x - 2, head_y));

        let mut game = Game {
            snake,
            direction: Direction::Right,
            next_direction: Direction::Right,
            food: (0, 0),
            score: 0,
            alive: true,
        };
        game.food = game.random_food();
        game
    }

    /// Pick a random cell that the snake is not sitting on.
    fn random_food(&self) -> (i32, i32) {
        let mut rng = rand::rng();
        loop {
            let x = rng.random_range(0..COLS as i32);
            let y = rng.random_range(0..ROWS as i32);
            if !self.snake.contains(&(x, y)) {
                return (x, y);
            }
        }
    }

    /// Read the arrow keys and WASD, and queue up a direction change.
    fn handle_input(&mut self, window: &Window) {
        let wanted = if window.is_key_down(Key::Up) || window.is_key_down(Key::W) {
            Some(Direction::Up)
        } else if window.is_key_down(Key::Down) || window.is_key_down(Key::S) {
            Some(Direction::Down)
        } else if window.is_key_down(Key::Left) || window.is_key_down(Key::A) {
            Some(Direction::Left)
        } else if window.is_key_down(Key::Right) || window.is_key_down(Key::D) {
            Some(Direction::Right)
        } else {
            None
        };

        if let Some(direction) = wanted {
            // Compare against the direction we are really moving, not the one
            // we queued last frame. That way two fast turns inside a single
            // tick cannot fold the snake back into itself.
            if !direction.is_opposite(self.direction) {
                self.next_direction = direction;
            }
        }
    }

    /// Move the snake forward one cell.
    fn step(&mut self) {
        if !self.alive {
            return;
        }

        self.direction = self.next_direction;

        let (dx, dy) = self.direction.delta();
        let (head_x, head_y) = *self.snake.front().expect("the snake is never empty");
        let new_head = (head_x + dx, head_y + dy);

        // Ran into a wall?
        if new_head.0 < 0
            || new_head.0 >= COLS as i32
            || new_head.1 < 0
            || new_head.1 >= ROWS as i32
        {
            self.alive = false;
            return;
        }

        let eating = new_head == self.food;

        // Ran into itself? Moving into the tail cell is allowed when we are
        // not eating, because the tail is about to move out of the way.
        if self.snake.contains(&new_head) {
            let tail = *self.snake.back().expect("the snake is never empty");
            if !(new_head == tail && !eating) {
                self.alive = false;
                return;
            }
        }

        self.snake.push_front(new_head);

        if eating {
            // Do not remove the tail: the snake grows by one.
            self.score += 1;
            self.food = self.random_food();
        } else {
            self.snake.pop_back();
        }
    }

    /// Paint the whole frame into `buffer`.
    fn draw(&self, buffer: &mut [u32]) {
        for pixel in buffer.iter_mut() {
            *pixel = BACKGROUND;
        }

        // Grid lines along every cell boundary.
        for col in 0..COLS {
            fill_rect(buffer, col * CELL, 0, 1, HEIGHT, GRID);
        }
        for row in 0..ROWS {
            fill_rect(buffer, 0, row * CELL, WIDTH, 1, GRID);
        }

        draw_cell(buffer, self.food.0 as usize, self.food.1 as usize, FOOD);

        // The head gets a brighter colour than the body.
        for (index, &(x, y)) in self.snake.iter().enumerate() {
            let colour = if index == 0 { SNAKE_HEAD } else { SNAKE_BODY };
            draw_cell(buffer, x as usize, y as usize, colour);
        }

        draw_text(buffer, &format!("SCORE {}", self.score), 10, 8, TEXT, 2);

        if !self.alive {
            self.draw_game_over(buffer);
        }
    }

    /// A dark band across the middle with the final score and a hint.
    fn draw_game_over(&self, buffer: &mut [u32]) {
        let band_height = 104;
        let band_y = (HEIGHT - band_height) / 2;
        fill_rect(buffer, 0, band_y, WIDTH, band_height, OVERLAY);

        draw_text_centered(buffer, "GAME OVER", band_y + 16, TEXT, 3);
        draw_text_centered(buffer, &format!("SCORE {}", self.score), band_y + 52, TEXT, 2);
        draw_text_centered(buffer, "PRESS R TO RESTART", band_y + 78, TEXT, 1);
    }
}

fn main() {
    let mut window = Window::new("Snake", WIDTH, HEIGHT, WindowOptions::default())
        .expect("could not open a window");

    // Pace the loop at 60 frames per second.
    window.set_target_fps(60);

    let mut buffer: Vec<u32> = vec![BACKGROUND; WIDTH * HEIGHT];
    let mut game = Game::new();
    let mut last_move = Instant::now();

    while window.is_open() && !window.is_key_down(Key::Escape) {
        game.handle_input(&window);

        if game.alive {
            // The snake only moves every MOVE_EVERY, even though we draw
            // far more often than that.
            if last_move.elapsed() >= MOVE_EVERY {
                last_move = Instant::now();
                game.step();
            }
        } else if window.is_key_pressed(Key::R, KeyRepeat::No) {
            game = Game::new();
            last_move = Instant::now();
        }

        game.draw(&mut buffer);
        window
            .update_with_buffer(&buffer, WIDTH, HEIGHT)
            .expect("failed to draw the frame");
    }
}

/// Fill a rectangle with one colour. Anything past the window edge is clipped.
fn fill_rect(buffer: &mut [u32], x: usize, y: usize, width: usize, height: usize, colour: u32) {
    let x_end = (x + width).min(WIDTH);
    let y_end = (y + height).min(HEIGHT);
    for py in y..y_end {
        for px in x..x_end {
            buffer[py * WIDTH + px] = colour;
        }
    }
}

/// Draw one grid cell with a one-pixel gap, so cells look like separate blocks.
fn draw_cell(buffer: &mut [u32], col: usize, row: usize, colour: u32) {
    fill_rect(
        buffer,
        col * CELL + 1,
        row * CELL + 1,
        CELL - 2,
        CELL - 2,
        colour,
    );
}

/// Draw 8x8 bitmap text. `scale` 1 is 8 pixels tall, 2 is 16, and so on.
fn draw_text(buffer: &mut [u32], text: &str, x: usize, y: usize, colour: u32, scale: usize) {
    let mut cursor_x = x;

    for ch in text.chars() {
        if let Some(glyph) = BASIC_FONTS.get(ch) {
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..8usize {
                    // Bit 0 is the leftmost pixel of the row.
                    if *bits & (1 << col) != 0 {
                        fill_rect(
                            buffer,
                            cursor_x + col * scale,
                            y + row * scale,
                            scale,
                            scale,
                            colour,
                        );
                    }
                }
            }
        }
        // Move past this glyph, leaving one pixel of spacing.
        cursor_x += 8 * scale + scale;
    }
}

/// How wide `text` will be when drawn at `scale`.
fn text_width(text: &str, scale: usize) -> usize {
    text.chars().count() * (8 * scale + scale)
}

/// Draw `text` horizontally centred in the window.
fn draw_text_centered(buffer: &mut [u32], text: &str, y: usize, colour: u32, scale: usize) {
    let x = WIDTH.saturating_sub(text_width(text, scale)) / 2;
    draw_text(buffer, text, x, y, colour, scale);
}
