//! Portable shortcut values, validation, and English display names.
use std::fmt;

pub const ALT: u16 = 1;
pub const CTRL: u16 = 2;
pub const SHIFT: u16 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub modifiers: u16,
    pub key: u16,
}

impl Shortcut {
    pub fn validate(self) -> Result<(), &'static str> {
        if self.modifiers & !(ALT | CTRL | SHIFT) != 0 {
            return Err("Use Ctrl, Alt, or Shift. Windows-key shortcuts are not supported.");
        }
        if self.key == 0x7b {
            return Err("F12 is reserved by Windows. Choose another key.");
        }
        if key_name(self.key).is_none() {
            return Err("Choose a letter, number, function key, or navigation key.");
        }
        if self.modifiers & (CTRL | ALT) == 0
            && !(0x70..=0x87).contains(&self.key)
            && !matches!(self.key, 0x13 | 0x14 | 0x90 | 0x91)
        {
            return Err("Add Ctrl or Alt to this key, or choose a function key.");
        }
        Ok(())
    }

    pub fn encode(self) -> String {
        format!("{:04X}:{:04X}", self.modifiers, self.key)
    }
    pub fn decode(text: &str) -> Option<Self> {
        let (modifiers, key) = text.split_once(':')?;
        let value = Self {
            modifiers: u16::from_str_radix(modifiers, 16).ok()?,
            key: u16::from_str_radix(key, 16).ok()?,
        };
        value.validate().ok()?;
        Some(value)
    }
}

impl fmt::Display for Shortcut {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (modifier, name) in [(CTRL, "Ctrl"), (ALT, "Alt"), (SHIFT, "Shift")] {
            if self.modifiers & modifier != 0 {
                write!(formatter, "{name}+")?;
            }
        }
        formatter.write_str(&key_name(self.key).unwrap_or_else(|| "Unknown key".into()))
    }
}

fn key_name(key: u16) -> Option<String> {
    let special = match key {
        0x08 => "Backspace",
        0x09 => "Tab",
        0x0d => "Enter",
        0x13 => "Pause",
        0x14 => "Caps Lock",
        0x20 => "Space",
        0x21 => "PgUp",
        0x22 => "PgDn",
        0x23 => "End",
        0x24 => "Home",
        0x25 => "Left",
        0x26 => "Up",
        0x27 => "Right",
        0x28 => "Down",
        0x2d => "Insert",
        0x2e => "Delete",
        0x6a => "Num *",
        0x6b => "Num +",
        0x6c => "Num Sep",
        0x6d => "Num -",
        0x6e => "Num .",
        0x6f => "Num /",
        0x90 => "Num Lock",
        0x91 => "Scroll Lock",
        0xba => ";",
        0xbb => "=",
        0xbc => ",",
        0xbd => "-",
        0xbe => ".",
        0xbf => "/",
        0xc0 => "`",
        0xdb => "[",
        0xdc => "\\",
        0xdd => "]",
        0xde => "'",
        0xe2 => "OEM 102",
        0x30..=0x39 | 0x41..=0x5a => return char::from_u32(key as u32).map(|c| c.to_string()),
        0x60..=0x69 => return Some(format!("Num {}", key - 0x60)),
        0x70..=0x87 => return Some(format!("F{}", key - 0x6f)),
        _ => return None,
    };
    Some(special.into())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shortcuts(pub [Shortcut; 3]);

impl Default for Shortcuts {
    fn default() -> Self {
        Self([
            Shortcut {
                modifiers: 0,
                key: 0x77,
            },
            Shortcut {
                modifiers: SHIFT,
                key: 0x77,
            },
            Shortcut {
                modifiers: CTRL,
                key: 0x77,
            },
        ])
    }
}

impl Shortcuts {
    pub fn validate(self) -> Result<(), &'static str> {
        for (index, shortcut) in self.0.iter().enumerate() {
            shortcut.validate()?;
            if self.0[..index].contains(shortcut) {
                return Err("That shortcut is already assigned to another action.");
            }
        }
        Ok(())
    }
    pub fn updated(mut self, index: usize, shortcut: Shortcut) -> Result<Self, &'static str> {
        *self.0.get_mut(index).ok_or("Unknown shortcut action.")? = shortcut;
        self.validate()?;
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_are_unique_and_displayed_in_english() {
        let defaults = Shortcuts::default();
        assert!(defaults.validate().is_ok());
        assert_eq!(
            defaults.0.map(|value| value.to_string()),
            ["F8", "Shift+F8", "Ctrl+F8"]
        );
        assert_eq!(
            Shortcut {
                key: 0x2e,
                modifiers: CTRL | ALT | SHIFT
            }
            .to_string(),
            "Ctrl+Alt+Shift+Delete"
        );
    }
    #[test]
    fn duplicate_replacement_does_not_mutate_the_previous_set() {
        let original = Shortcuts::default();
        assert!(original.updated(0, original.0[2]).is_err());
        assert_eq!(original, Shortcuts::default());
        assert!(original.updated(3, original.0[0]).is_err());
    }
    #[test]
    fn serialization_round_trips_custom_bindings() {
        for shortcut in [
            Shortcut {
                key: 0x4b,
                modifiers: CTRL | SHIFT,
            },
            Shortcut {
                key: 0x78,
                modifiers: ALT,
            },
            Shortcut {
                key: 0x91,
                modifiers: 0,
            },
        ] {
            assert_eq!(Shortcut::decode(&shortcut.encode()), Some(shortcut));
        }
    }
    #[test]
    fn rejects_typing_keys_modifiers_and_reserved_shortcuts() {
        for shortcut in [
            Shortcut {
                key: 0x41,
                modifiers: 0,
            },
            Shortcut {
                key: 0x41,
                modifiers: SHIFT,
            },
            Shortcut {
                key: 0x10,
                modifiers: CTRL,
            },
            Shortcut {
                key: 0x7b,
                modifiers: CTRL,
            },
            Shortcut {
                key: 0x41,
                modifiers: 8,
            },
        ] {
            assert!(shortcut.validate().is_err(), "{shortcut}");
        }
        assert_eq!(Shortcut::decode("0000:0041"), None);
        assert_eq!(Shortcut::decode("not-a-shortcut"), None);
    }
}
