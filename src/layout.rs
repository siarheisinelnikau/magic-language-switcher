//! Conversion by physical keys, independent of language names or alphabets.
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Stroke {
    pub modifiers: u8,
    pub scan: u16,
}

#[derive(Clone, Debug)]
pub struct KeyboardLayout {
    pub id: u32,
    pub name: String,
    pub keys: BTreeMap<Stroke, String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    /// None uses character coverage, with the active layout as the tie-breaker.
    pub from: Option<u32>,
    /// None cycles to the next installed layout after the detected source.
    pub to: Option<u32>,
}

#[derive(Clone, Debug, Default)]
pub struct Catalog {
    pub layouts: Vec<KeyboardLayout>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Conversion {
    pub source: u32,
    pub destination: u32,
    pub text: String,
}

impl Catalog {
    pub fn convert(
        &self,
        text: &str,
        selection: Selection,
        active: u32,
    ) -> Result<Conversion, String> {
        if self.layouts.len() < 2 {
            return Err(
                "Add another keyboard layout in Windows Settings, then reopen this window.".into(),
            );
        }
        let source = if let Some(id) = selection.from {
            self.layouts.iter().position(|layout| layout.id == id)
                .ok_or("The source layout is no longer installed. Reopen settings to refresh the list.")?
        } else {
            let active_index = self
                .layouts
                .iter()
                .position(|layout| layout.id == active)
                .unwrap_or(0);
            self.layouts
                .iter()
                .enumerate()
                .max_by_key(|(index, layout)| {
                    let coverage = text
                        .chars()
                        .filter(|c| {
                            c.is_alphabetic()
                                && layout.keys.values().any(|value| value.contains(*c))
                        })
                        .count();
                    (
                        coverage,
                        usize::from(*index == active_index),
                        self.layouts.len() - index,
                    )
                })
                .map_or(active_index, |(index, _)| index)
        };
        let destination = if let Some(id) = selection.to {
            self.layouts.iter().position(|layout| layout.id == id)
                .ok_or("The destination layout is no longer installed. Reopen settings to refresh the list.")?
        } else {
            (source + 1) % self.layouts.len()
        };
        if source == destination {
            return Err("Choose different From and To layouts.".into());
        }
        let from = &self.layouts[source];
        let to = &self.layouts[destination];
        Ok(Conversion {
            source: from.id,
            destination: to.id,
            text: map_keys(text, from, to),
        })
    }
}

fn map_keys(text: &str, from: &KeyboardLayout, to: &KeyboardLayout) -> String {
    // Longest output first handles keyboard ligatures. Duplicate characters prefer the
    // simplest modifier state, but only when the destination has the same physical key.
    let mut mappings: Vec<_> = from
        .keys
        .iter()
        .filter_map(|(key, value)| {
            to.keys
                .get(key)
                .filter(|target| !target.is_empty())
                .map(|target| (value, target, key))
        })
        .filter(|(source, _, _)| !source.is_empty())
        .collect();
    mappings.sort_by_key(|(source, _, key)| (std::cmp::Reverse(source.len()), **key));
    let mut remaining = text;
    let mut result = String::with_capacity(text.len());
    while !remaining.is_empty() {
        if let Some((source, target, _)) = mappings
            .iter()
            .find(|(source, _, _)| remaining.starts_with(source.as_str()))
        {
            result.push_str(target);
            remaining = &remaining[source.len()..];
        } else if let Some(character) = remaining.chars().next() {
            result.push(character);
            remaining = &remaining[character.len_utf8()..];
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keyboard(id: u32, plain: &str, shifted: &str) -> KeyboardLayout {
        let keys = [(0, plain), (1, shifted)]
            .into_iter()
            .flat_map(|(modifiers, row)| {
                row.chars().enumerate().map(move |(index, c)| {
                    (
                        Stroke {
                            scan: index as u16,
                            modifiers,
                        },
                        c.to_string(),
                    )
                })
            })
            .collect();
        KeyboardLayout {
            id,
            name: format!("Test layout {id}"),
            keys,
        }
    }
    fn catalog() -> Catalog {
        Catalog {
            layouts: vec![
                keyboard(
                    1,
                    "qwertyuiop[]asdfghjkl;'zxcvbnm,./",
                    "QWERTYUIOP{}ASDFGHJKL:\"ZXCVBNM<>?",
                ),
                keyboard(
                    2,
                    "йцукенгшщзхъфывапролджэячсмитьбю.",
                    "ЙЦУКЕНГШЩЗХЪФЫВАПРОЛДЖЭЯЧСМИТЬБЮ,",
                ),
                keyboard(
                    3,
                    "azertyuiop^$qsdfghjklmùwxcvbn,;:",
                    "AZERTYUIOP¨£QSDFGHJKLM%WXCVBN?./",
                ),
                keyboard(
                    4,
                    ";ςερτυθιοπ[]ασδφγηξκλ΄'ζχψωβνμ,./",
                    ":ΣΕΡΤΥΘΙΟΠ{}ΑΣΔΦΓΗΞΚΛ¨\"ΖΧΨΩΒΝΜ<>?",
                ),
            ],
        }
    }
    fn explicit(from: u32, to: u32) -> Selection {
        Selection {
            from: Some(from),
            to: Some(to),
        }
    }

    #[test]
    fn english_russian_with_case_and_spacing() {
        let layouts = catalog();
        assert_eq!(
            layouts
                .convert("Ghbdtn vbh!\r\n\t123", explicit(1, 2), 1)
                .unwrap()
                .text,
            "Привет мир!\r\n\t123"
        );
        assert_eq!(
            layouts
                .convert("руддщ цщкдв", explicit(2, 1), 2)
                .unwrap()
                .text,
            "hello world"
        );
    }
    #[test]
    fn supports_non_cyrillic_layouts() {
        let layouts = catalog();
        assert_eq!(
            layouts.convert("qwerty", explicit(1, 3), 1).unwrap().text,
            "azerty"
        );
        assert_eq!(
            layouts.convert("azerty", explicit(3, 1), 3).unwrap().text,
            "qwerty"
        );
        assert_eq!(
            layouts.convert("qwerty", explicit(1, 4), 1).unwrap().text,
            ";ςερτυ"
        );
    }
    #[test]
    fn detects_script_even_if_active_layout_changed() {
        let result = catalog().convert("руддщ", Selection::default(), 1).unwrap();
        assert_eq!(result.source, 2);
        assert_eq!(result.destination, 3);
    }
    #[test]
    fn latin_layout_ambiguity_uses_active_layout() {
        assert_eq!(
            catalog()
                .convert("azerty", Selection::default(), 3)
                .unwrap()
                .source,
            3
        );
        assert_eq!(
            catalog()
                .convert("qwerty", Selection::default(), 1)
                .unwrap()
                .source,
            1
        );
    }
    #[test]
    fn cycles_all_installed_layouts_and_wraps() {
        let result = catalog()
            .convert(";ςερτυ", Selection::default(), 4)
            .unwrap();
        assert_eq!(result.destination, 1);
        assert_eq!(result.text, "qwerty");
    }
    #[test]
    fn unmapped_characters_and_whitespace_survive() {
        let input = "ghbdtn 🪄 👩‍💻 中文 é\r\n\t";
        assert_eq!(
            catalog().convert(input, explicit(1, 2), 1).unwrap().text,
            "привет 🪄 👩‍💻 中文 é\r\n\t"
        );
    }
    #[test]
    fn altgr_and_multiple_characters_use_physical_keys() {
        let key = Stroke {
            scan: 0x12,
            modifiers: 2,
        };
        let mut source = keyboard(1, "", "");
        let mut target = keyboard(2, "", "");
        source.keys.insert(key, "€".into());
        target.keys.insert(key, "ab".into());
        let layouts = Catalog {
            layouts: vec![source, target],
        };
        assert_eq!(
            layouts.convert("€!", explicit(1, 2), 1).unwrap().text,
            "ab!"
        );
        assert_eq!(
            layouts.convert("ab!", explicit(2, 1), 2).unwrap().text,
            "€!"
        );
    }
    #[test]
    fn rejects_missing_or_identical_layouts() {
        assert!(catalog().convert("a", explicit(1, 1), 1).is_err());
        assert!(catalog().convert("a", explicit(999, 1), 1).is_err());
        assert!(
            Catalog::default()
                .convert("a", Selection::default(), 1)
                .is_err()
        );
    }
    #[test]
    fn empty_text_is_valid_for_preview() {
        assert_eq!(
            catalog().convert("", Selection::default(), 1).unwrap().text,
            ""
        );
    }
}
