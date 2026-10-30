use super::{
    clipboard::Session,
    native::{self, Target},
};
use crate::layout::{Catalog, Selection};
use std::{thread, time::Duration};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

#[derive(Clone, Copy, Debug)]
pub enum Action {
    LastWord = 1,
    Selection = 2,
    Undo = 3,
}

struct UndoEntry {
    target: Target,
    original: String,
    converted: String,
}

#[derive(Default)]
pub struct Engine {
    undo: Option<UndoEntry>,
}

impl Engine {
    pub fn execute(
        &mut self,
        action: Action,
        target: Target,
        owner: usize,
        catalog: &Catalog,
        selection: Selection,
    ) -> Result<String, String> {
        for attempt in 0..=100 {
            if !native::modifiers_down() {
                break;
            }
            if attempt == 100 {
                return Err("Release the shortcut keys and try again.".into());
            }
            thread::sleep(Duration::from_millis(15));
        }
        target.ensure_active()?;
        if target.is_password() {
            return Err("Password fields are not supported.".into());
        }
        let mut clipboard = Session::capture(owner)?;
        let result = if matches!(action, Action::Undo) {
            self.undo(target, &mut clipboard)
        } else {
            self.correct(action, target, &mut clipboard, catalog, selection)
        };
        let restored = clipboard.restore();
        match (result, restored) {
            (Ok(message), Ok(())) => Ok(message),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(format!("Text corrected, but {error}")),
        }
    }

    fn correct(
        &mut self,
        action: Action,
        target: Target,
        clipboard: &mut Session,
        catalog: &Catalog,
        selection: Selection,
    ) -> Result<String, String> {
        let last_word = matches!(action, Action::LastWord);
        if last_word {
            native::chord(VK_LEFT, &[VK_CONTROL, VK_SHIFT])?;
            thread::sleep(Duration::from_millis(35));
        }
        let mut replaced = false;
        let result = (|| {
            let original = clipboard.copy_selection()?;
            target.ensure_active()?;
            if original.is_empty() {
                return Err("There is no selected text to convert.".into());
            }
            if original.encode_utf16().count() > 4096 {
                return Err("Select up to 4,096 characters at a time.".into());
            }
            let conversion = catalog.convert(&original, selection, target.layout as u32)?;
            let converted = conversion.text;
            if converted == original {
                return Err(
                    "This text has no characters that change between the chosen layouts.".into(),
                );
            }
            replace(&converted, clipboard)?;
            replaced = true;
            self.undo = Some(UndoEntry {
                target,
                original,
                converted,
            });
            native::switch_layout(target.window, conversion.destination);
            Ok("Text corrected. Ctrl+F8 undoes the last fix.".into())
        })();
        if last_word && !replaced && target.ensure_active().is_ok() {
            let _ = native::chord(VK_RIGHT, &[]);
        }
        result
    }

    fn undo(&mut self, target: Target, clipboard: &mut Session) -> Result<String, String> {
        let entry = self.undo.as_ref().ok_or("Nothing to undo yet.")?;
        if target.window != entry.target.window || target.focus != entry.target.focus {
            return Err("Return to the same editor to undo the last fix.".into());
        }
        // CRLF is one caret step in native Windows editors. Complex Unicode caret behavior
        // differs by editor; the exact text comparison below prevents an incorrect replacement.
        let steps = entry.converted.chars().filter(|c| *c != '\r').count();
        native::select_previous(steps)?;
        let result = (|| {
            thread::sleep(Duration::from_millis(35));
            let actual = clipboard.copy_selection()?;
            target.ensure_active()?;
            if actual != entry.converted {
                return Err("The text or caret moved. Undo directly after a correction.".into());
            }
            replace(&entry.original, clipboard)?;
            native::switch_layout(target.window, entry.target.layout as u32);
            Ok("Last correction undone.".into())
        })();
        if result.is_ok() {
            self.undo = None;
        } else if target.ensure_active().is_ok() {
            let _ = native::chord(VK_RIGHT, &[]);
        }
        result
    }
}

fn replace(text: &str, clipboard: &mut Session) -> Result<(), String> {
    if text.contains(['\r', '\n', '\t']) {
        clipboard.paste(text)
    } else {
        native::type_unicode(text)
    }
}
