//! Translates incoming `InputEvent`s from the controller into local OS input
//! via `enigo`.

use anyhow::{Context, Result};
use enigo::{Axis, Button as EnigoButton, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use visuara_common::control::{InputEvent, KeyCode, MouseButton};

pub struct InputInjector {
    enigo: Enigo,
}

impl InputInjector {
    pub fn new() -> Result<Self> {
        let enigo = Enigo::new(&Settings::default()).context("initialize input injector")?;
        Ok(Self { enigo })
    }

    pub fn apply(&mut self, event: InputEvent) -> Result<()> {
        match event {
            InputEvent::MouseMove { x, y } => {
                self.enigo.move_mouse(x, y, Coordinate::Abs).context("move mouse")?;
            }
            InputEvent::MouseButton { button, pressed } => {
                let direction = if pressed { Direction::Press } else { Direction::Release };
                self.enigo
                    .button(map_button(button), direction)
                    .context("send mouse button")?;
            }
            InputEvent::MouseScroll { delta_x, delta_y } => {
                if delta_y != 0 {
                    self.enigo.scroll(delta_y, Axis::Vertical).context("scroll vertical")?;
                }
                if delta_x != 0 {
                    self.enigo.scroll(delta_x, Axis::Horizontal).context("scroll horizontal")?;
                }
            }
            InputEvent::KeyEvent { key, pressed } => {
                let direction = if pressed { Direction::Press } else { Direction::Release };
                self.enigo.key(map_key(key), direction).context("send key event")?;
            }
            InputEvent::TypeText { text } => {
                self.enigo.text(&text).context("type text")?;
            }
        }
        Ok(())
    }
}

fn map_button(button: MouseButton) -> EnigoButton {
    match button {
        MouseButton::Left => EnigoButton::Left,
        MouseButton::Right => EnigoButton::Right,
        MouseButton::Middle => EnigoButton::Middle,
    }
}

fn map_key(key: KeyCode) -> Key {
    match key {
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Enter => Key::Return,
        KeyCode::Tab => Key::Tab,
        KeyCode::Escape => Key::Escape,
        KeyCode::Space => Key::Space,
        KeyCode::Delete => Key::Delete,
        KeyCode::ArrowUp => Key::UpArrow,
        KeyCode::ArrowDown => Key::DownArrow,
        KeyCode::ArrowLeft => Key::LeftArrow,
        KeyCode::ArrowRight => Key::RightArrow,
        KeyCode::Shift => Key::Shift,
        KeyCode::Control => Key::Control,
        KeyCode::Alt => Key::Alt,
        KeyCode::Meta => Key::Meta,
    }
}
