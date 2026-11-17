//! Keyboard and mouse input mapping for TUI scrolling and quit actions.

use crossterm::event::{
    Event as CrosstermEvent, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollAction {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserAction {
    Scroll(ScrollAction),
    Quit,
}

/// Map a crossterm terminal event to a TUI `UserAction`.
pub fn map_crossterm_event(ev: &CrosstermEvent) -> Option<UserAction> {
    match ev {
        CrosstermEvent::Key(key) => {
            if key.kind == KeyEventKind::Release {
                return None;
            }
            if key.modifiers.contains(KeyModifiers::CONTROL)
                && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
            {
                return Some(UserAction::Quit);
            }
            if key.modifiers.is_empty() && key.code == KeyCode::Char('q') {
                return Some(UserAction::Quit);
            }
            let scroll = match key.code {
                KeyCode::Up | KeyCode::Char('k') => ScrollAction::Up,
                KeyCode::Down | KeyCode::Char('j') => ScrollAction::Down,
                KeyCode::PageUp | KeyCode::Char('b') => ScrollAction::PageUp,
                KeyCode::PageDown | KeyCode::Char(' ') => ScrollAction::PageDown,
                KeyCode::Home | KeyCode::Char('g') => ScrollAction::Home,
                KeyCode::End | KeyCode::Char('G') => ScrollAction::End,
                _ => return None,
            };
            Some(UserAction::Scroll(scroll))
        }
        CrosstermEvent::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => Some(UserAction::Scroll(ScrollAction::Up)),
            MouseEventKind::ScrollDown => Some(UserAction::Scroll(ScrollAction::Down)),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyEvent, MouseEvent};

    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> CrosstermEvent {
        CrosstermEvent::Key(KeyEvent::new(code, modifiers))
    }

    #[test]
    fn maps_quit_keys() {
        assert_eq!(
            map_crossterm_event(&key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(UserAction::Quit)
        );
        assert_eq!(
            map_crossterm_event(&key(KeyCode::Char('q'), KeyModifiers::NONE)),
            Some(UserAction::Quit)
        );
        // Ctrl+q is ignored
        assert_eq!(
            map_crossterm_event(&key(KeyCode::Char('q'), KeyModifiers::CONTROL)),
            None
        );
    }

    #[test]
    fn maps_all_scroll_keys_and_ignores_release() {
        let cases = [
            (KeyCode::Up, ScrollAction::Up),
            (KeyCode::Char('k'), ScrollAction::Up),
            (KeyCode::Down, ScrollAction::Down),
            (KeyCode::Char('j'), ScrollAction::Down),
            (KeyCode::PageUp, ScrollAction::PageUp),
            (KeyCode::Char('b'), ScrollAction::PageUp),
            (KeyCode::PageDown, ScrollAction::PageDown),
            (KeyCode::Char(' '), ScrollAction::PageDown),
            (KeyCode::Home, ScrollAction::Home),
            (KeyCode::Char('g'), ScrollAction::Home),
            (KeyCode::End, ScrollAction::End),
            (KeyCode::Char('G'), ScrollAction::End),
        ];
        for (code, expected) in cases {
            assert_eq!(
                map_crossterm_event(&key(code, KeyModifiers::NONE)),
                Some(UserAction::Scroll(expected))
            );
        }

        let mut release = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        release.kind = KeyEventKind::Release;
        assert_eq!(map_crossterm_event(&CrosstermEvent::Key(release)), None);

        let wheel_up = CrosstermEvent::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(
            map_crossterm_event(&wheel_up),
            Some(UserAction::Scroll(ScrollAction::Up))
        );
    }
}
