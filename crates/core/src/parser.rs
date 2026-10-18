use crate::model::Vocab;
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub verb: Option<String>,
    pub noun: Option<String>,
    pub second: Option<String>,
    pub unknown: Option<String>,
}

#[derive(Debug)]
pub struct VocabIndex {
    pub ignore: BTreeSet<String>,
    pub prepositions: BTreeSet<String>,
    pub phrases: Vec<(String, String)>,
    pub rewrites: Vec<(Regex, String)>,
    pub verbs: BTreeMap<String, String>,
    pub nouns: BTreeMap<String, String>,
    pub verb_meta: BTreeMap<String, bool>,
    pub verb_ranged: BTreeMap<String, bool>,
    pub verb_default: BTreeMap<String, String>,
    /// verb id → room id → noun to walk toward.
    pub verb_targets: BTreeMap<String, BTreeMap<String, String>>,
    pub rooms: BTreeMap<String, BTreeMap<String, String>>,
    pub unknown_word: String,
    pub unknown_command: String,
    pub bare: BTreeMap<String, String>,
}

impl VocabIndex {
    pub fn build(vocab: &Vocab) -> Result<Self, String> {
        let mut verbs = BTreeMap::new();
        let mut verb_meta = BTreeMap::new();
        let mut verb_ranged = BTreeMap::new();
        let mut verb_default = BTreeMap::new();
        let mut verb_targets = BTreeMap::new();
        for verb in &vocab.verbs {
            verb_meta.insert(verb.id.clone(), verb.meta);
            verb_ranged.insert(verb.id.clone(), verb.ranged);
            if let Some(reply) = &verb.default_reply {
                verb_default.insert(verb.id.clone(), reply.clone());
            }
            if !verb.targets.is_empty() {
                verb_targets.insert(verb.id.clone(), verb.targets.clone());
            }
            for word in &verb.words {
                let word = word.to_lowercase();
                if verbs.insert(word.clone(), verb.id.clone()).is_some() {
                    return Err(format!("verb word '{word}' is in two groups"));
                }
            }
        }
        let mut nouns = BTreeMap::new();
        for noun in &vocab.nouns {
            for word in &noun.words {
                let word = word.to_lowercase();
                if nouns.insert(word.clone(), noun.id.clone()).is_some() {
                    return Err(format!("noun word '{word}' is in two groups"));
                }
            }
        }
        let mut rewrites = Vec::new();
        for rewrite in &vocab.rewrites {
            let re = Regex::new(&rewrite.pattern)
                .map_err(|err| format!("bad rewrite /{}/: {err}", rewrite.pattern))?;
            rewrites.push((re, rewrite.replace.clone()));
        }
        let mut phrases: Vec<(String, String)> = vocab
            .phrases
            .iter()
            .map(|p| (p.from.to_lowercase(), p.to.to_lowercase()))
            .collect();
        phrases.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then(a.0.cmp(&b.0)));
        let mut rooms = BTreeMap::new();
        for (room, map) in &vocab.rooms {
            let mut lowered = BTreeMap::new();
            for (from, to) in map {
                lowered.insert(from.to_lowercase(), to.to_lowercase());
            }
            rooms.insert(room.clone(), lowered);
        }
        Ok(Self {
            ignore: vocab.ignore.iter().map(|w| w.to_lowercase()).collect(),
            prepositions: vocab
                .prepositions
                .iter()
                .map(|w| w.to_lowercase())
                .collect(),
            phrases,
            rewrites,
            verbs,
            nouns,
            verb_meta,
            verb_ranged,
            verb_default,
            verb_targets,
            rooms,
            unknown_word: vocab.unknown_word.clone(),
            unknown_command: vocab.unknown_command.clone(),
            bare: vocab.bare.clone(),
        })
    }

    pub fn parse(&self, room: &str, input: &str) -> Parsed {
        let mut text = normalize(input);
        for (re, replace) in &self.rewrites {
            text = re.replace_all(&text, replace).into_owned();
        }
        text = normalize(&text);
        for (from, to) in &self.phrases {
            text = replace_phrase(&text, from, to);
        }
        let tokens: Vec<String> = text
            .split_whitespace()
            .filter(|tok| !self.ignore.contains(*tok))
            .map(|s| s.to_string())
            .collect();
        if let Some(word) = tokens.iter().find(|tok| !self.known(tok)) {
            return Parsed {
                verb: None,
                noun: None,
                second: None,
                unknown: Some(word.clone()),
            };
        }
        if tokens.is_empty() {
            return Parsed {
                verb: None,
                noun: None,
                second: None,
                unknown: None,
            };
        }
        let Some(verb) = self.verbs.get(&tokens[0]).cloned() else {
            return Parsed {
                verb: None,
                noun: None,
                second: None,
                unknown: None,
            };
        };
        let rest = &tokens[1..];
        let split = rest.iter().position(|tok| self.prepositions.contains(tok));
        let (noun_toks, second_toks): (&[String], &[String]) = if let Some(0) = split {
            // "look at well" — the preposition sits where the noun would, so it
            // is only a filler and the following words are the noun.
            (&rest[1..], &[])
        } else if let Some(i) = split {
            (&rest[..i], &rest[i + 1..])
        } else {
            (rest, &[])
        };
        let mut noun = self.resolve_noun(room, noun_toks);
        let second = self.resolve_noun(room, second_toks);
        if noun.is_none() {
            if let Some(fill) = self.bare.get(&verb) {
                noun = Some(fill.clone());
            }
        }
        Parsed {
            verb: Some(verb),
            noun,
            second,
            unknown: None,
        }
    }

    fn known(&self, word: &str) -> bool {
        self.verbs.contains_key(word)
            || self.nouns.contains_key(word)
            || self.prepositions.contains(word)
            || self.ignore.contains(word)
            || self.rooms.values().any(|map| map.contains_key(word))
    }

    fn resolve_noun(&self, room: &str, tokens: &[String]) -> Option<String> {
        if tokens.is_empty() {
            return None;
        }
        let mapped: Vec<String> = tokens
            .iter()
            .map(|tok| {
                self.rooms
                    .get(room)
                    .and_then(|map| map.get(tok))
                    .cloned()
                    .unwrap_or_else(|| tok.clone())
            })
            .collect();
        let max = mapped.len().min(4);
        for len in (1..=max).rev() {
            let joined = mapped[..len].join(" ");
            if let Some(id) = self.nouns.get(&joined) {
                return Some(id.clone());
            }
        }
        for tok in &mapped {
            if let Some(id) = self.nouns.get(tok) {
                return Some(id.clone());
            }
        }
        None
    }

    pub fn unknown_message(&self, word: &str) -> String {
        self.unknown_word.replace("{word}", word)
    }
}

fn normalize(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '\'' || ch == '$' {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(' ');
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn replace_phrase(input: &str, from: &str, to: &str) -> String {
    let words: Vec<&str> = input.split_whitespace().collect();
    let from_words: Vec<&str> = from.split_whitespace().collect();
    if from_words.is_empty() {
        return input.to_string();
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < words.len() {
        if words[i..].starts_with(&from_words[..]) {
            if !to.is_empty() {
                out.push(to);
            }
            i += from_words.len();
        } else {
            out.push(words[i]);
            i += 1;
        }
    }
    out.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{NounDef, Phrase, VerbDef};
    use std::collections::BTreeMap;

    fn sample() -> VocabIndex {
        let vocab = Vocab {
            ignore: vec!["a".into(), "the".into()],
            prepositions: vec!["on".into(), "with".into(), "at".into()],
            phrases: vec![Phrase {
                from: "pick up".into(),
                to: "get".into(),
            }],
            verbs: vec![
                VerbDef {
                    id: "get".into(),
                    words: vec!["get".into(), "take".into()],
                    ranged: false,
                    meta: false,
                    default_reply: Some("You can't take that.".into()),
                    targets: BTreeMap::new(),
                },
                VerbDef {
                    id: "look".into(),
                    words: vec!["look".into()],
                    ranged: true,
                    meta: false,
                    default_reply: None,
                    targets: BTreeMap::new(),
                },
                VerbDef {
                    id: "use".into(),
                    words: vec!["use".into()],
                    ranged: false,
                    meta: false,
                    default_reply: None,
                    targets: BTreeMap::new(),
                },
            ],
            nouns: vec![
                NounDef {
                    id: "key".into(),
                    words: vec!["key".into()],
                },
                NounDef {
                    id: "door".into(),
                    words: vec!["door".into()],
                },
                NounDef {
                    id: "well".into(),
                    words: vec!["well".into()],
                },
            ],
            rooms: BTreeMap::from([(
                "porch".into(),
                BTreeMap::from([("rim".into(), "well".into())]),
            )]),
            unknown_word: "I don't know the word '{word}'.".into(),
            ..Vocab::default()
        };
        VocabIndex::build(&vocab).unwrap()
    }

    #[test]
    fn phrases_preps_and_room_words() {
        let vocab = sample();
        let parsed = vocab.parse("porch", "pick up the key");
        assert_eq!(parsed.verb.as_deref(), Some("get"));
        assert_eq!(parsed.noun.as_deref(), Some("key"));
        assert!(parsed.unknown.is_none());

        let parsed = vocab.parse("porch", "use key on door");
        assert_eq!(parsed.verb.as_deref(), Some("use"));
        assert_eq!(parsed.noun.as_deref(), Some("key"));
        assert_eq!(parsed.second.as_deref(), Some("door"));

        let parsed = vocab.parse("porch", "look at the rim");
        assert_eq!(parsed.noun.as_deref(), Some("well"));

        let parsed = vocab.parse("porch", "get flurb");
        assert_eq!(parsed.unknown.as_deref(), Some("flurb"));
        assert_eq!(
            vocab.unknown_message("flurb"),
            "I don't know the word 'flurb'."
        );
    }
}
