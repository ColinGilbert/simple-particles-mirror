use std::{cell::RefCell, collections::HashMap};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    German,
}

impl Language {
    pub const ALL: [Self; 2] = [Self::English, Self::German];

    pub const fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::German => "de",
        }
    }

    pub const fn native_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::German => "Deutsch",
        }
    }

    const fn data(self) -> &'static str {
        match self {
            Self::English => include_str!("../../assets/lang/en.lang"),
            Self::German => include_str!("../../assets/lang/de.lang"),
        }
    }
}

fn parse(data: &'static str) -> HashMap<&'static str, &'static str> {
    let mut map = HashMap::new();
    for line in data.lines() {
        let line = line.trim();
        if let Some((key, value)) = line.split_once(' ') {
            map.insert(key, value.trim());
        }
    }
    map
}

fn detect_system_language() -> Language {
    for variable in ["LANG", "LC_ALL", "LC_MESSAGES", "LANGUAGE"] {
        let Ok(value) = std::env::var(variable) else {
            continue;
        };
        let code = value
            .split('.')
            .next()
            .unwrap_or_default()
            .split('_')
            .next()
            .unwrap_or_default();
        if let Some(language) = Language::ALL.iter().find(|entry| entry.code() == code) {
            return *language;
        }
    }
    Language::English
}

struct Strings {
    language: Language,
    map: HashMap<&'static str, &'static str>,
}

thread_local! {
    static STRINGS: RefCell<Strings> = RefCell::new({
        let language = detect_system_language();
        Strings { language, map: parse(language.data()) }
    });
}

pub fn current() -> Language {
    STRINGS.with(|strings| strings.borrow().language)
}

pub fn set(language: Language) {
    STRINGS.with(|strings| {
        let mut strings = strings.borrow_mut();
        strings.language = language;
        strings.map = parse(language.data());
    });
}

pub fn t(key: &str) -> &'static str {
    STRINGS.with(|strings| strings.borrow().map.get(key).copied().unwrap_or("?"))
}
