#![cfg_attr(windows, windows_subsystem = "windows")]

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const INDEX_HTML: &str = include_str!("../assets/web/index.html");
const APP_JS: &str = include_str!("../assets/web/app.js");
const EDITOR_TOOLS_JS: &str = include_str!("../assets/web/editor-tools.js");
const STYLES_CSS: &str = include_str!("../assets/web/styles.css");
const PROFILES_TSV: &str = include_str!("../assets/data/profiles.tsv");
const TIERS_TSV: &str = include_str!("../assets/data/tiers.tsv");
const BASIC_WORDS: &str = include_str!("../assets/data/basic_words.txt");
const SEED_LEXICON_TSV: &str = include_str!("../assets/data/seed_lexicon.tsv");
const DEMO_TEXT: &str = include_str!("../assets/data/demo.txt");
const ECDICT_TSV: &[u8] = include_bytes!("../assets/data/ecdict.tsv");
const CMUDICT: &[u8] = include_bytes!("../assets/data/cmudict.dict");

static FILE_COUNTER: AtomicU64 = AtomicU64::new(1);
const MAX_HTTP_HEADER_BYTES: usize = 64 * 1024;
const MAX_HTTP_BODY_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug)]
struct GradeProfile {
    code: String,
    label: String,
    estimated_vocab: usize,
    lexicon_min_len: usize,
    unknown_min_len: usize,
    note: String,
}

#[derive(Clone, Debug)]
struct LexiconEntry {
    term: String,
    ipa: String,
    zh: String,
    hard: bool,
}

impl LexiconEntry {
    fn ipa_text(&self) -> String {
        let ipa = if self.ipa.trim().is_empty() {
            "?"
        } else {
            self.ipa.trim()
        };
        if ipa.starts_with('(') && ipa.ends_with(')') {
            ipa.to_string()
        } else {
            format!("({ipa})")
        }
    }

    fn zh_text(&self) -> String {
        compact_translation(&self.zh, 10)
    }
}

struct AppState {
    profiles: Vec<GradeProfile>,
    known_words: HashMap<String, HashSet<String>>,
    basic_words: HashSet<String>,
    seed_lexicon: HashMap<String, LexiconEntry>,
    translations: HashMap<String, String>,
    pronunciations: HashMap<String, Vec<Vec<String>>>,
}

impl AppState {
    fn load() -> Result<Self, String> {
        let profiles = load_profiles()?;
        let tiers = load_tiers();
        let mut known_words = HashMap::new();
        for profile in &profiles {
            let mut known = HashSet::new();
            for tier_profile in &profiles {
                if let Some(words) = tiers.get(&tier_profile.code) {
                    known.extend(words.iter().cloned());
                }
                if tier_profile.code == profile.code {
                    break;
                }
            }
            known_words.insert(profile.code.clone(), known);
        }

        let basic_words = BASIC_WORDS
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect();

        let mut seed_lexicon = HashMap::new();
        for line in SEED_LEXICON_TSV.lines() {
            let cols: Vec<&str> = line.split('\t').collect();
            if cols.len() >= 4 {
                add_entry(
                    &mut seed_lexicon,
                    LexiconEntry {
                        term: cols[0].to_string(),
                        ipa: cols[1].to_string(),
                        zh: cols[2].to_string(),
                        hard: cols[3] == "true",
                    },
                );
            }
        }

        Ok(Self {
            profiles,
            known_words,
            basic_words,
            seed_lexicon,
            translations: load_translations()?,
            pronunciations: load_pronunciations()?,
        })
    }

    fn profile(&self, code: &str) -> &GradeProfile {
        self.profiles
            .iter()
            .find(|profile| profile.code == code)
            .unwrap_or_else(|| {
                self.profiles
                    .iter()
                    .find(|profile| profile.code == "P4")
                    .unwrap()
            })
    }
}

fn load_profiles() -> Result<Vec<GradeProfile>, String> {
    let mut profiles = Vec::new();
    for line in PROFILES_TSV.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 6 {
            continue;
        }
        profiles.push(GradeProfile {
            code: cols[0].to_string(),
            label: cols[1].to_string(),
            estimated_vocab: cols[2].parse().map_err(|_| "invalid profile vocab")?,
            lexicon_min_len: cols[3]
                .parse()
                .map_err(|_| "invalid profile lexicon length")?,
            unknown_min_len: cols[4]
                .parse()
                .map_err(|_| "invalid profile unknown length")?,
            note: cols[5].to_string(),
        });
    }
    if profiles.is_empty() {
        return Err("missing grade profiles".to_string());
    }
    Ok(profiles)
}

fn load_tiers() -> HashMap<String, Vec<String>> {
    let mut tiers = HashMap::new();
    for line in TIERS_TSV.lines() {
        let mut parts = line.splitn(2, '\t');
        let Some(code) = parts.next() else { continue };
        let Some(words) = parts.next() else { continue };
        tiers.insert(
            code.to_string(),
            words.split_whitespace().map(ToOwned::to_owned).collect(),
        );
    }
    tiers
}

fn load_translations() -> Result<HashMap<String, String>, String> {
    let text = std::str::from_utf8(ECDICT_TSV).map_err(|err| err.to_string())?;
    let mut map = HashMap::with_capacity(400_000);
    for line in text.lines() {
        let mut parts = line.splitn(2, '\t');
        let Some(word) = parts.next() else { continue };
        let Some(translation) = parts.next() else {
            continue;
        };
        map.insert(word.to_string(), translation.to_string());
    }
    Ok(map)
}

fn load_pronunciations() -> Result<HashMap<String, Vec<Vec<String>>>, String> {
    let text = std::str::from_utf8(CMUDICT).map_err(|err| err.to_string())?;
    let mut map: HashMap<String, Vec<Vec<String>>> = HashMap::with_capacity(140_000);
    for line in text.lines() {
        if line.trim().is_empty() || line.starts_with(";;;") {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(raw_word) = parts.next() else {
            continue;
        };
        let mut word = raw_word.to_ascii_lowercase();
        if let Some(index) = word.find('(') {
            word.truncate(index);
        }
        let phones: Vec<String> = parts.map(ToOwned::to_owned).collect();
        if !word.is_empty() && !phones.is_empty() {
            map.entry(word).or_default().push(phones);
        }
    }
    Ok(map)
}

fn normalize_key(term: &str) -> String {
    let mut value = term
        .trim()
        .to_ascii_lowercase()
        .replace(['’', '‘'], "'")
        .replace(['‐', '‑', '‒', '–', '—', '―'], "-")
        .replace(['“', '”', '"'], "");

    let start = value
        .char_indices()
        .find(|(_, ch)| ch.is_ascii_lowercase())
        .map(|(index, _)| index);
    let end = value
        .char_indices()
        .rev()
        .find(|(_, ch)| ch.is_ascii_lowercase())
        .map(|(index, ch)| index + ch.len_utf8());
    value = match (start, end) {
        (Some(start), Some(end)) if start < end => value[start..end].to_string(),
        _ => String::new(),
    };

    let mut collapsed = String::new();
    let mut last_space = false;
    for ch in value.chars() {
        if ch.is_whitespace() {
            if !last_space {
                collapsed.push(' ');
            }
            last_space = true;
        } else {
            collapsed.push(ch);
            last_space = false;
        }
    }
    collapsed.trim().to_string()
}

fn normalize_title(text: &str) -> String {
    let mut out = String::new();
    let mut last_space = false;
    for ch in text.to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_space = false;
        } else if !last_space {
            out.push(' ');
            last_space = true;
        }
    }
    out.trim().to_string()
}

fn key_variants(term: &str) -> Vec<String> {
    let key = normalize_key(term);
    let candidates = [
        key.clone(),
        key.replace('-', " "),
        key.replace(' ', "-"),
        key.replace(['-', ' '], ""),
    ];
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in candidates {
        if !item.is_empty() && seen.insert(item.clone()) {
            out.push(item);
        }
    }
    out
}

fn candidate_lemmas(word: &str) -> Vec<String> {
    let key = normalize_key(word);
    let mut candidates = vec![key.clone()];
    if key.ends_with("'s") && key.len() > 2 {
        candidates.push(key[..key.len() - 2].to_string());
    }
    if key.contains('-') {
        candidates.extend(key_variants(&key));
    }
    for suffix in ["ing", "ed", "s", "es", "ly", "er", "est"] {
        if key.len() <= suffix.len() + 2 || !key.ends_with(suffix) {
            continue;
        }
        let stem = key[..key.len() - suffix.len()].to_string();
        candidates.push(stem.clone());
        if suffix == "ing" || suffix == "ed" {
            candidates.push(format!("{stem}e"));
            let chars: Vec<char> = stem.chars().collect();
            if chars.len() > 2 && chars[chars.len() - 1] == chars[chars.len() - 2] {
                candidates.push(chars[..chars.len() - 1].iter().collect());
            }
        }
    }
    if key.ends_with("ies") && key.len() > 4 {
        candidates.push(format!("{}y", &key[..key.len() - 3]));
    }
    if key.ends_with("ves") && key.len() > 4 {
        candidates.push(format!("{}f", &key[..key.len() - 3]));
        candidates.push(format!("{}fe", &key[..key.len() - 3]));
    }
    if key.ends_with("men") && key.len() > 4 {
        candidates.push(format!("{}man", &key[..key.len() - 3]));
    }

    let mut seen = HashSet::new();
    let mut ordered = Vec::new();
    for item in candidates {
        for variant in key_variants(&item) {
            if seen.insert(variant.clone()) {
                ordered.push(variant);
            }
        }
    }
    ordered
}

fn add_entry(lexicon: &mut HashMap<String, LexiconEntry>, entry: LexiconEntry) {
    for key in key_variants(&entry.term) {
        lexicon.insert(key, entry.clone());
    }
}

fn parse_custom_annotations(custom_text: &str) -> (Vec<LexiconEntry>, Vec<String>, Vec<String>) {
    let mut entries = Vec::new();
    let mut force_terms = Vec::new();
    let mut ignored_terms = Vec::new();
    let text = custom_text.trim();
    if text.is_empty() {
        return (entries, force_terms, ignored_terms);
    }

    let mut rich_lines = Vec::new();
    let mut plain_chunks = Vec::new();
    for line in text.lines() {
        let mut stripped = line.trim();
        if stripped.is_empty() {
            continue;
        }
        if let Some(ignored) = stripped.strip_prefix('!') {
            ignored_terms.extend(split_terms(ignored));
            continue;
        }
        if let Some(important) = stripped.strip_prefix('*') {
            stripped = important.trim();
        }
        if stripped.is_empty() {
            continue;
        }
        if ['=', '|', ':']
            .iter()
            .any(|separator| stripped.contains(*separator))
        {
            rich_lines.push(stripped.to_string());
        } else {
            plain_chunks.push(stripped.to_string());
        }
    }

    for line in rich_lines {
        let mut parts = Vec::new();
        for separator in ['=', '|', ':'] {
            if line.contains(separator) {
                parts = line
                    .splitn(3, separator)
                    .map(|part| part.trim().to_string())
                    .collect();
                break;
            }
        }
        if parts.len() >= 3 && parts.iter().all(|part| !part.is_empty()) {
            entries.push(LexiconEntry {
                term: parts[0].clone(),
                ipa: parts[1].clone(),
                zh: parts[2].clone(),
                hard: true,
            });
        } else if let Some(first) = parts.first()
            && !first.is_empty()
        {
            force_terms.push(first.clone());
        }
    }

    if !plain_chunks.is_empty() {
        for raw in split_terms(&plain_chunks.join(" ")) {
            if !raw.is_empty() {
                force_terms.push(raw);
            }
        }
    }
    (entries, force_terms, ignored_terms)
}

fn split_terms(text: &str) -> Vec<String> {
    text.split(|ch: char| ch.is_whitespace() || matches!(ch, ',' | '，' | ';' | '；'))
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn extract_hard_terms(
    extra_text: &str,
    lexicon: &HashMap<String, LexiconEntry>,
) -> HashSet<String> {
    let mut hard_terms = HashSet::new();
    for (key, entry) in lexicon {
        if entry.hard && !key.contains(' ') {
            hard_terms.insert(key.clone());
        }
    }
    for raw in split_terms(extra_text) {
        for key in key_variants(&raw) {
            hard_terms.insert(key);
        }
    }
    hard_terms
}

fn lookup_entry(
    state: &AppState,
    word: &str,
    lexicon: &mut HashMap<String, LexiconEntry>,
) -> Option<LexiconEntry> {
    for candidate in candidate_lemmas(word) {
        if let Some(entry) = lexicon.get(&candidate) {
            return Some(entry.clone());
        }
    }
    let generated_ipa = lookup_generated_ipa(state, word)?;
    let translation = lookup_generated_translation(state, word)?;
    let entry = LexiconEntry {
        term: word.to_string(),
        ipa: generated_ipa,
        zh: translation,
        hard: false,
    };
    add_entry(lexicon, entry.clone());
    Some(entry)
}

fn lookup_generated_ipa(state: &AppState, word: &str) -> Option<String> {
    let key = normalize_key(word);
    for candidate in candidate_lemmas(word) {
        let variants = [
            candidate.clone(),
            candidate.replace(' ', "-"),
            candidate.replace('-', ""),
        ];
        for variant in variants {
            if let Some(found) = state.pronunciations.get(&variant)
                && let Some(pronunciation) = found.iter().min_by_key(|phones| {
                    let primary = phones.iter().filter(|phone| phone.ends_with('1')).count();
                    let secondary = phones.iter().filter(|phone| phone.ends_with('2')).count();
                    (primary.abs_diff(1), secondary, phones.len())
                })
            {
                let ipa = arpabet_to_ipa(pronunciation);
                if !ipa.is_empty() {
                    return Some(ipa);
                }
            }
        }
    }

    if key.contains('-') {
        let mut parts = Vec::new();
        for part in key.split('-') {
            let found = state.pronunciations.get(part)?;
            let pronunciation = found.first()?;
            let ipa = arpabet_to_ipa(pronunciation);
            if ipa.is_empty() {
                return None;
            }
            parts.push(ipa);
        }
        if !parts.is_empty() {
            return Some(parts.join("."));
        }
    }
    None
}

fn arpabet_to_ipa(phonemes: &[String]) -> String {
    let mut output: Vec<String> = Vec::new();
    let mut last_vowel_end = 0usize;
    for phoneme in phonemes {
        let (symbol, stress) = split_arpabet_stress(phoneme);
        let ipa = match symbol {
            "AH" if stress == Some('0') => "ə",
            "AH" => "ʌ",
            "ER" if stress == Some('0') => "ɚ",
            "ER" => "ɝ",
            "AA" => "ɑ",
            "AE" => "æ",
            "AO" => "ɔ",
            "AW" => "aʊ",
            "AY" => "aɪ",
            "B" => "b",
            "CH" => "tʃ",
            "D" => "d",
            "DH" => "ð",
            "EH" => "ɛ",
            "EY" => "eɪ",
            "F" => "f",
            "G" => "ɡ",
            "HH" => "h",
            "IH" => "ɪ",
            "IY" => "i",
            "JH" => "dʒ",
            "K" => "k",
            "L" => "l",
            "M" => "m",
            "N" => "n",
            "NG" => "ŋ",
            "OW" => "oʊ",
            "OY" => "ɔɪ",
            "P" => "p",
            "R" => "r",
            "S" => "s",
            "SH" => "ʃ",
            "T" => "t",
            "TH" => "θ",
            "UH" => "ʊ",
            "UW" => "u",
            "V" => "v",
            "W" => "w",
            "Y" => "j",
            "Z" => "z",
            "ZH" => "ʒ",
            _ => "",
        };
        if ipa.is_empty() {
            continue;
        }
        match stress {
            Some('1') => output.insert(last_vowel_end, "ˈ".to_string()),
            Some('2') => output.insert(last_vowel_end, "ˌ".to_string()),
            _ => {}
        }
        output.push(ipa.to_string());
        if stress.is_some() {
            last_vowel_end = output.len();
        }
    }
    output.join("")
}

fn split_arpabet_stress(phoneme: &str) -> (&str, Option<char>) {
    if let Some(last) = phoneme.chars().last()
        && matches!(last, '0' | '1' | '2')
    {
        let cut = phoneme.len() - last.len_utf8();
        return (&phoneme[..cut], Some(last));
    }
    (phoneme, None)
}

fn lookup_generated_translation(state: &AppState, word: &str) -> Option<String> {
    let mut exact_translation = None;
    let mut first_compact = None;
    for (index, candidate) in candidate_lemmas(word).iter().enumerate() {
        let Some(translation) = state.translations.get(candidate) else {
            continue;
        };
        let compact = compact_translation(translation, 10);
        if compact.is_empty() {
            continue;
        }
        if first_compact.is_none() {
            first_compact = Some(compact.clone());
        }
        if index == 0 {
            if contains_inflection_note(translation) {
                exact_translation = Some(compact);
                continue;
            }
            return Some(compact);
        }
        if !contains_inflection_note(translation) {
            return Some(compact);
        }
    }
    exact_translation.or(first_compact)
}

fn contains_inflection_note(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "复数",
        "过去式",
        "过去分词",
        "现在分词",
        "第三人称",
        "比较级",
        "最高级",
        "变形",
        "形式",
        "plural",
        "past tense",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn compact_translation(text: &str, max_length: usize) -> String {
    let mut value = text.replace("\\n", "\n").trim().to_string();
    if value.is_empty() {
        return String::new();
    }
    value = remove_bracketed(&value, '[', ']');
    value = remove_inflection_parentheses(&value);

    for raw_clause in value.split(['\n', ';', '；', '。']) {
        let mut clause = strip_pos_prefix(raw_clause.trim()).to_string();
        if !clause.chars().any(is_cjk) {
            continue;
        }
        while clause.chars().next().is_some_and(|ch| !is_cjk(ch)) {
            clause.remove(0);
        }
        let mut result = String::new();
        for piece in clause.split([',', '，', '、']) {
            let piece =
                piece.trim_matches(|ch: char| ch.is_whitespace() || " ,，、:：".contains(ch));
            if piece.chars().any(is_cjk) {
                result = piece.to_string();
                break;
            }
        }
        if result.is_empty() {
            result = clause;
        }
        result = result
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect::<String>()
            .trim_matches(|ch| " ,，、;；".contains(ch))
            .to_string();
        if !result.is_empty() {
            return take_chars(&result, max_length);
        }
    }
    String::new()
}

fn remove_bracketed(text: &str, open: char, close: char) -> String {
    let mut depth = 0usize;
    let mut out = String::new();
    for ch in text.chars() {
        if ch == open {
            depth += 1;
            continue;
        }
        if ch == close && depth > 0 {
            depth -= 1;
            continue;
        }
        if depth == 0 {
            out.push(ch);
        }
    }
    out
}

fn remove_inflection_parentheses(text: &str) -> String {
    let mut out = String::new();
    let mut buf = String::new();
    let mut in_paren = false;
    for ch in text.chars() {
        if matches!(ch, '(' | '（') && !in_paren {
            in_paren = true;
            buf.clear();
            continue;
        }
        if matches!(ch, ')' | '）') && in_paren {
            in_paren = false;
            if !contains_inflection_note(&buf) {
                out.push('（');
                out.push_str(&buf);
                out.push('）');
            }
            continue;
        }
        if in_paren {
            buf.push(ch);
        } else {
            out.push(ch);
        }
    }
    if in_paren {
        out.push_str(&buf);
    }
    out
}

fn strip_pos_prefix(mut text: &str) -> &str {
    loop {
        let trimmed = text.trim_start();
        let lower = trimmed.to_ascii_lowercase();
        let mut removed = false;
        for prefix in [
            "adj.", "adv.", "prep.", "conj.", "pron.", "num.", "art.", "int.", "vt.", "vi.", "n.",
            "v.", "a.",
        ] {
            if lower.starts_with(prefix) {
                text = &trimmed[prefix.len()..];
                removed = true;
                break;
            }
        }
        if !removed {
            return trimmed;
        }
    }
}

fn is_cjk(ch: char) -> bool {
    ('\u{3400}'..='\u{9fff}').contains(&ch)
}

fn take_chars(text: &str, max_length: usize) -> String {
    text.chars().take(max_length).collect()
}

fn alpha_len(token: &str) -> usize {
    token.chars().filter(|ch| ch.is_ascii_alphabetic()).count()
}

fn ipa_syllable_count(entry: &LexiconEntry) -> usize {
    let ipa = entry
        .ipa
        .trim_matches(|ch| ch == '(' || ch == ')' || ch == ' ');
    if ipa.is_empty() {
        1
    } else {
        1.max(ipa.matches('.').count() + ipa.matches('ˈ').count() + ipa.matches('ˌ').count())
    }
}

fn should_annotate_word(
    state: &AppState,
    word: &str,
    entry: Option<&LexiconEntry>,
    hard_terms: &HashSet<String>,
    profile: &GradeProfile,
    known_words: &HashSet<String>,
) -> bool {
    let key = normalize_key(word);
    if candidate_lemmas(word)
        .iter()
        .any(|candidate| hard_terms.contains(candidate))
    {
        return true;
    }
    if key.is_empty() || state.basic_words.contains(&key) {
        return false;
    }
    if known_words.contains(&key) {
        return false;
    }
    if let Some(entry) = entry {
        if entry.hard {
            return true;
        }
        if alpha_len(word) >= profile.lexicon_min_len {
            return true;
        }
        if key.contains('-') || key.contains(' ') {
            return true;
        }
        if ipa_syllable_count(entry) >= 3
            && alpha_len(word) >= 5.max(profile.lexicon_min_len.saturating_sub(2))
        {
            return true;
        }
        matches!(profile.code.as_str(), "P1" | "P2")
    } else {
        alpha_len(word) >= profile.unknown_min_len
    }
}

fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < text.len() {
        let ch = text[index..].chars().next().unwrap();
        let start = index;
        if ch.is_ascii_alphabetic() {
            index += ch.len_utf8();
            loop {
                if index >= text.len() {
                    break;
                }
                let current = text[index..].chars().next().unwrap();
                if current.is_ascii_alphabetic() {
                    index += current.len_utf8();
                    continue;
                }
                if matches!(current, '-' | '\'') {
                    let next_index = index + current.len_utf8();
                    if next_index < text.len()
                        && text[next_index..]
                            .chars()
                            .next()
                            .is_some_and(|next| next.is_ascii_alphabetic())
                    {
                        index = next_index;
                        continue;
                    }
                }
                break;
            }
            tokens.push(text[start..index].to_string());
        } else if ch.is_ascii_digit() {
            index += ch.len_utf8();
            while index < text.len()
                && text[index..]
                    .chars()
                    .next()
                    .is_some_and(|next| next.is_ascii_digit())
            {
                index += text[index..].chars().next().unwrap().len_utf8();
            }
            tokens.push(text[start..index].to_string());
        } else if ch.is_whitespace() {
            index += ch.len_utf8();
            while index < text.len()
                && text[index..]
                    .chars()
                    .next()
                    .is_some_and(|next| next.is_whitespace())
            {
                index += text[index..].chars().next().unwrap().len_utf8();
            }
            tokens.push(text[start..index].to_string());
        } else {
            index += ch.len_utf8();
            tokens.push(text[start..index].to_string());
        }
    }
    tokens
}

fn is_word_token(token: &str) -> bool {
    let mut saw_alpha = false;
    let mut previous_sep = false;
    for ch in token.chars() {
        if ch.is_ascii_alphabetic() {
            saw_alpha = true;
            previous_sep = false;
        } else if matches!(ch, '-' | '\'') && saw_alpha && !previous_sep {
            previous_sep = true;
        } else {
            return false;
        }
    }
    saw_alpha && !previous_sep
}

#[derive(Clone, Copy, Debug)]
struct TextSizes {
    english_hps: usize,
    ipa_hps: usize,
    zh_hps: usize,
    line_height: f32,
    word_spacing_pt: f32,
    page_size: PageSize,
}

#[derive(Clone, Copy, Debug)]
struct PageSize {
    width: f32,
    height: f32,
    unit: &'static str,
}

impl TextSizes {
    fn default_body() -> Self {
        Self {
            english_hps: 27,
            ipa_hps: 13,
            zh_hps: 13,
            line_height: 2.15,
            word_spacing_pt: 0.0,
            page_size: page_size_from_code("letter"),
        }
    }

    fn title_hps(&self) -> usize {
        (self.english_hps + 9).clamp(24, 72)
    }
}

fn page_size_from_code(code: &str) -> PageSize {
    match code.trim().to_ascii_lowercase().as_str() {
        "a4" => PageSize {
            width: 210.0,
            height: 297.0,
            unit: "mm",
        },
        "b5" => PageSize {
            width: 176.0,
            height: 250.0,
            unit: "mm",
        },
        "a5" => PageSize {
            width: 148.0,
            height: 210.0,
            unit: "mm",
        },
        _ => PageSize {
            width: 8.5,
            height: 11.0,
            unit: "in",
        },
    }
}

#[derive(Clone)]
struct AnnotationParts {
    ipa: String,
    zh: String,
    hard: bool,
}

struct AnnotationItem {
    text: String,
    annotation: Option<AnnotationParts>,
}

#[allow(clippy::too_many_arguments)]
fn generate_docx(
    state: &AppState,
    article_text: &str,
    output_path: &Path,
    title: &str,
    custom_annotations: &str,
    annotate_unknown: bool,
    grade_code: &str,
    sizes: TextSizes,
) -> Result<Vec<String>, String> {
    let article_text = prepare_article_text(article_text, title)?;
    let title = title.trim();
    let (mut lexicon, hard_terms, profile, known_words) =
        annotation_context(state, custom_annotations, grade_code)?;

    let mut missing_terms = HashSet::new();
    let mut paragraphs = Vec::new();
    if !title.is_empty() {
        let items = annotate_items(
            state,
            title,
            &mut lexicon,
            &hard_terms,
            profile,
            &known_words,
            annotate_unknown,
            &mut missing_terms,
        );
        paragraphs.push(items_to_docx_paragraph(
            &items,
            sizes.title_hps(),
            sizes,
            "center",
            true,
        ));
    }

    for raw_para in article_text.split('\n') {
        let text = raw_para.trim();
        if text.is_empty() {
            paragraphs.push("<w:p/>".to_string());
            continue;
        }
        let items = annotate_items(
            state,
            text,
            &mut lexicon,
            &hard_terms,
            profile,
            &known_words,
            annotate_unknown,
            &mut missing_terms,
        );
        paragraphs.push(items_to_docx_paragraph(
            &items,
            sizes.english_hps,
            sizes,
            "left",
            false,
        ));
    }

    let layout_report = audit_docx_ruby_layout(&paragraphs)?;
    let bytes = build_docx(&paragraphs, sizes)?;
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(output_path, bytes).map_err(|err| err.to_string())?;

    let _layout_summary = (
        layout_report.table_count,
        layout_report.min_cell_width,
        layout_report.min_row_height,
    );
    Ok(sorted_missing(missing_terms))
}

fn render_preview_html(
    state: &AppState,
    article_text: &str,
    title: &str,
    custom_annotations: &str,
    annotate_unknown: bool,
    grade_code: &str,
) -> Result<(String, Vec<String>), String> {
    let article_text = prepare_article_text(article_text, title)?;
    let title = title.trim();
    let (mut lexicon, hard_terms, profile, known_words) =
        annotation_context(state, custom_annotations, grade_code)?;
    let mut missing_terms = HashSet::new();
    let mut html = String::from("<div class=\"preview-page\">");

    if !title.is_empty() {
        let items = annotate_items(
            state,
            title,
            &mut lexicon,
            &hard_terms,
            profile,
            &known_words,
            annotate_unknown,
            &mut missing_terms,
        );
        html.push_str("<p class=\"preview-line preview-title\">");
        html.push_str(&items_to_preview_html(&items));
        html.push_str("</p>");
    }

    for raw_para in article_text.split('\n') {
        let text = raw_para.trim();
        if text.is_empty() {
            html.push_str("<p class=\"preview-line preview-empty\">&nbsp;</p>");
            continue;
        }
        let items = annotate_items(
            state,
            text,
            &mut lexicon,
            &hard_terms,
            profile,
            &known_words,
            annotate_unknown,
            &mut missing_terms,
        );
        html.push_str("<p class=\"preview-line\">");
        html.push_str(&items_to_preview_html(&items));
        html.push_str("</p>");
    }

    html.push_str("</div>");
    Ok((html, sorted_missing(missing_terms)))
}

#[allow(clippy::too_many_arguments)]
fn generate_pdf(
    state: &AppState,
    article_text: &str,
    output_path: &Path,
    title: &str,
    custom_annotations: &str,
    annotate_unknown: bool,
    grade_code: &str,
    sizes: TextSizes,
) -> Result<Vec<String>, String> {
    let (preview_html, missing) = render_preview_html(
        state,
        article_text,
        title,
        custom_annotations,
        annotate_unknown,
        grade_code,
    )?;
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let output_path = absolute_path(output_path)?;
    let temp_dir = std::env::temp_dir().join(format!("cijing-pdf-{}", unique_suffix()));
    fs::create_dir_all(&temp_dir).map_err(|err| err.to_string())?;
    let html_path = temp_dir.join("preview.html");
    let temp_pdf_path = temp_dir.join("preview.pdf");
    fs::write(&html_path, printable_preview_html(&preview_html, sizes))
        .map_err(|err| err.to_string())?;
    let result = print_html_to_pdf(&html_path, &temp_pdf_path).and_then(|_| {
        fs::copy(&temp_pdf_path, &output_path)
            .map(|_| ())
            .map_err(|err| format!("PDF 保存失败：{err}"))
    });
    let _ = fs::remove_dir_all(&temp_dir);
    result?;
    Ok(missing)
}

fn printable_preview_html(preview_html: &str, sizes: TextSizes) -> String {
    let page_width = page_length_css(sizes.page_size.width, sizes.page_size.unit);
    let page_height = page_length_css(sizes.page_size.height, sizes.page_size.unit);
    let line_height = css_number(sizes.line_height);
    let word_spacing = css_number(sizes.word_spacing_pt);
    format!(
        concat!(
            "<!doctype html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\"/>",
            "<title>词境精读 PDF</title><style>",
            "@page{{size:{page_width} {page_height};margin:0;}}",
            "html,body{{width:{page_width};min-height:{page_height};margin:0;padding:0;background:white;color:#111827;}}",
            ".preview-page{{width:{page_width};min-height:{page_height};padding:0.82in;box-sizing:border-box;background:white;",
            "font-family:\"Times New Roman\",serif;font-size:{english}pt;line-height:{line_height};word-spacing:{word_spacing}pt;letter-spacing:0;}}",
            ".preview-line{{margin:0 0 9pt;white-space:pre-wrap;}}",
            ".preview-title{{margin:0 0 18pt;text-align:center;font-size:{title}pt;font-weight:700;line-height:2;}}",
            ".preview-empty{{min-height:1.2em;}}",
            ".preview-token{{ruby-align:center;ruby-position:over;text-align:center;white-space:nowrap;break-inside:avoid;}}",
            ".preview-rt{{color:#444444;line-height:1.05;text-align:center;white-space:nowrap;}}",
            ".preview-token.hard .preview-base,.preview-token.hard .preview-rt{{color:#b35c00;font-weight:700;}}",
            ".preview-base{{line-height:1.08;}}",
            ".preview-token .preview-base{{font-weight:700;}}",
            ".preview-ipa{{font-family:Arial,sans-serif;font-size:{ipa}pt;}}",
            ".preview-zh{{font-family:\"Microsoft YaHei\",\"SimSun\",sans-serif;font-size:{zh}pt;}}",
            "</style></head><body>{preview_html}</body></html>"
        ),
        page_width = page_width,
        page_height = page_height,
        english = hps_to_points(sizes.english_hps),
        title = hps_to_points(sizes.title_hps()),
        line_height = line_height,
        word_spacing = word_spacing,
        ipa = hps_to_points(sizes.ipa_hps),
        zh = hps_to_points(sizes.zh_hps),
        preview_html = preview_html
    )
}

fn hps_to_points(hps: usize) -> String {
    let whole = hps / 2;
    if hps.is_multiple_of(2) {
        whole.to_string()
    } else {
        format!("{whole}.5")
    }
}

fn css_number(value: f32) -> String {
    let mut text = format!("{value:.2}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

fn page_length_css(value: f32, unit: &str) -> String {
    format!("{}{}", css_number(value), unit)
}

fn absolute_path(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .map_err(|err| err.to_string())?
            .join(path))
    }
}

fn print_html_to_pdf(html_path: &Path, output_path: &Path) -> Result<(), String> {
    let browser = find_pdf_browser().ok_or_else(|| {
        "未找到 Microsoft Edge 或 Chrome，无法自动输出 PDF。请安装 Edge/Chrome 后重试。".to_string()
    })?;
    let html_url = file_url(html_path)?;
    let profile_dir = output_path.with_extension("browser-profile");
    let _ = fs::create_dir_all(&profile_dir);
    let mut last_error = "浏览器没有生成 PDF 文件。".to_string();
    for headless_arg in ["--headless=new", "--headless"] {
        let _ = fs::remove_file(output_path);
        match run_pdf_browser_attempt(&browser, &html_url, output_path, &profile_dir, headless_arg)
        {
            Ok(()) => {
                let _ = fs::remove_dir_all(&profile_dir);
                return Ok(());
            }
            Err(error) => last_error = error,
        }
    }
    let _ = fs::remove_dir_all(&profile_dir);
    Err(format!("PDF 文件未生成：{last_error}"))
}

fn run_pdf_browser_attempt(
    browser: &Path,
    html_url: &str,
    output_path: &Path,
    profile_dir: &Path,
    headless_arg: &str,
) -> Result<(), String> {
    let output_arg = format!("--print-to-pdf={}", output_path.display());
    let profile_arg = format!("--user-data-dir={}", profile_dir.display());
    let mut child = Command::new(browser)
        .args([
            headless_arg,
            "--disable-gpu",
            "--disable-extensions",
            "--disable-background-networking",
            "--no-first-run",
            "--no-default-browser-check",
            "--allow-file-access-from-files",
            "--print-to-pdf-no-header",
            &profile_arg,
            &output_arg,
            html_url,
        ])
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("浏览器启动失败：{err}"))?;

    let started = Instant::now();
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|err| format!("浏览器打印失败：{err}"))?
        {
            break status;
        }
        if started.elapsed() > Duration::from_secs(45) {
            let _ = child.kill();
            return Err("浏览器打印超时。".to_string());
        }
        thread::sleep(Duration::from_millis(200));
    };

    let mut stderr = String::new();
    if let Some(mut stream) = child.stderr.take() {
        let _ = stream.read_to_string(&mut stderr);
    }

    if wait_for_nonempty_file(output_path, Duration::from_secs(8)) {
        return Ok(());
    }
    if !status.success() {
        let details = stderr.lines().next().unwrap_or("浏览器打印进程异常退出。");
        return Err(details.to_string());
    }
    Err("浏览器打印完成，但 PDF 文件仍未落盘。".to_string())
}

fn wait_for_nonempty_file(path: &Path, timeout: Duration) -> bool {
    let started = Instant::now();
    loop {
        if let Ok(metadata) = fs::metadata(path)
            && metadata.len() > 0
        {
            return true;
        }
        if started.elapsed() >= timeout {
            return false;
        }
        thread::sleep(Duration::from_millis(150));
    }
}

fn find_pdf_browser() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(program_files_x86) = std::env::var("ProgramFiles(x86)") {
        candidates.push(
            PathBuf::from(program_files_x86)
                .join("Microsoft")
                .join("Edge")
                .join("Application")
                .join("msedge.exe"),
        );
    }
    if let Ok(program_files) = std::env::var("ProgramFiles") {
        candidates.push(
            PathBuf::from(&program_files)
                .join("Microsoft")
                .join("Edge")
                .join("Application")
                .join("msedge.exe"),
        );
        candidates.push(
            PathBuf::from(program_files)
                .join("Google")
                .join("Chrome")
                .join("Application")
                .join("chrome.exe"),
        );
    }
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        candidates.push(
            PathBuf::from(local_app_data)
                .join("Google")
                .join("Chrome")
                .join("Application")
                .join("chrome.exe"),
        );
    }
    for candidate in candidates {
        if candidate.exists() {
            return Some(candidate);
        }
    }
    for name in ["msedge", "chrome", "google-chrome", "chromium"] {
        if Command::new(name).arg("--version").output().is_ok() {
            return Some(PathBuf::from(name));
        }
    }
    None
}

fn file_url(path: &Path) -> Result<String, String> {
    let path = absolute_path(path)?;
    let text = path.to_string_lossy().replace('\\', "/");
    let mut url = String::from("file:///");
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'/' | b':') {
            url.push(byte as char);
        } else {
            url.push_str(&format!("%{byte:02X}"));
        }
    }
    Ok(url)
}

fn prepare_article_text(article_text: &str, title: &str) -> Result<String, String> {
    let mut article_text = article_text.replace("\r\n", "\n").replace('\r', "\n");
    article_text = article_text.trim().to_string();
    if article_text.is_empty() {
        return Err("文章内容不能为空。".to_string());
    }

    let title = title.trim();
    if !title.is_empty() {
        let mut lines: Vec<String> = article_text.split('\n').map(ToOwned::to_owned).collect();
        while lines.first().is_some_and(|line| line.trim().is_empty()) {
            lines.remove(0);
        }
        if lines
            .first()
            .is_some_and(|line| normalize_title(line) == normalize_title(title))
        {
            lines.remove(0);
            while lines.first().is_some_and(|line| line.trim().is_empty()) {
                lines.remove(0);
            }
            article_text = lines.join("\n").trim().to_string();
        }
    }

    Ok(article_text)
}

type AnnotationContext<'a> = (
    HashMap<String, LexiconEntry>,
    HashSet<String>,
    &'a GradeProfile,
    HashSet<String>,
);

fn annotation_context<'a>(
    state: &'a AppState,
    custom_annotations: &str,
    grade_code: &str,
) -> Result<AnnotationContext<'a>, String> {
    let mut lexicon = state.seed_lexicon.clone();
    let (custom_entries, custom_force_terms, ignored_terms) =
        parse_custom_annotations(custom_annotations);
    for entry in custom_entries {
        add_entry(&mut lexicon, entry);
    }
    let hard_terms = extract_hard_terms(&custom_force_terms.join(" "), &lexicon);
    let profile = state.profile(grade_code);
    let mut known_words = state
        .known_words
        .get(&profile.code)
        .ok_or_else(|| "年级配置不可用。".to_string())?
        .clone();
    for ignored in ignored_terms {
        for key in key_variants(&ignored) {
            known_words.insert(key);
        }
    }
    Ok((lexicon, hard_terms, profile, known_words))
}

fn sorted_missing(missing_terms: HashSet<String>) -> Vec<String> {
    let mut missing: Vec<String> = missing_terms
        .into_iter()
        .filter(|term| !term.is_empty())
        .collect();
    missing.sort();
    missing
}

#[derive(Debug)]
struct LayoutReport {
    table_count: usize,
    min_cell_width: usize,
    min_row_height: usize,
}

#[allow(dead_code)]
fn audit_docx_layout(paragraphs: &[String]) -> Result<LayoutReport, String> {
    let xml = paragraphs.join("");
    let table_count = count_xml_items(&xml, "<w:tbl>");
    if table_count == 0 {
        return Ok(LayoutReport {
            table_count: 0,
            min_cell_width: 0,
            min_row_height: 0,
        });
    }

    let row_count = count_xml_items(&xml, "<w:tr>");
    let cant_split_count = count_xml_items(&xml, "<w:cantSplit/>");
    if table_count != row_count || row_count != cant_split_count {
        return Err("版式体检未通过：注释行缺少分页保护。".to_string());
    }
    if xml.contains("<w:ruby") || xml.contains("<w:fitText") {
        return Err("版式体检未通过：检测到不稳定的 Word 排版结构。".to_string());
    }

    if xml.contains("<w:t></w:t>") {
        return Err("Layout audit failed: empty visible text runs were detected.".to_string());
    }

    let table_widths = xml_attr_numbers(&xml, "<w:tblW", "w:w");
    if table_widths.iter().any(|width| *width > 9360) {
        return Err("版式体检未通过：有行宽超过页面可用宽度。".to_string());
    }

    let cell_widths = xml_attr_numbers(&xml, "<w:gridCol", "w:w");
    let min_cell_width = cell_widths.iter().copied().min().unwrap_or(0);
    if min_cell_width > 0 && min_cell_width < 300 {
        return Err("版式体检未通过：有词块宽度过窄，可能导致单词断开。".to_string());
    }

    let row_heights = xml_attr_numbers(&xml, "<w:trHeight", "w:val");
    let min_row_height = row_heights.iter().copied().min().unwrap_or(0);
    if min_row_height > 0 && min_row_height < 900 {
        return Err("版式体检未通过：行高不足，可能容不下注释和英文。".to_string());
    }

    Ok(LayoutReport {
        table_count,
        min_cell_width,
        min_row_height,
    })
}

fn count_xml_items(text: &str, needle: &str) -> usize {
    text.match_indices(needle).count()
}

fn audit_docx_ruby_layout(paragraphs: &[String]) -> Result<LayoutReport, String> {
    let xml = paragraphs.join("");
    let table_count = count_xml_items(&xml, "<w:tbl>");
    if table_count > 0 {
        return Err("Layout audit failed: table layout remains in the Word body.".to_string());
    }
    if xml.contains("<w:fitText") {
        return Err("Layout audit failed: unstable fitText layout was detected.".to_string());
    }
    if xml.contains("<w:t></w:t>") {
        return Err("Layout audit failed: empty visible text runs were detected.".to_string());
    }

    let ruby_count = count_xml_items(&xml, "<w:ruby>");
    let ruby_text_count = count_xml_items(&xml, "<w:rt>");
    let ruby_base_count = count_xml_items(&xml, "<w:rubyBase>");
    if ruby_count != ruby_text_count || ruby_count != ruby_base_count {
        return Err(
            "Layout audit failed: ruby annotations are not paired with base words.".to_string(),
        );
    }

    let line_heights = xml_attr_numbers(&xml, "<w:spacing", "w:line");
    let min_line_height = line_heights.iter().copied().min().unwrap_or(0);
    if ruby_count > 0 && min_line_height < 700 {
        return Err("Layout audit failed: line spacing is too small for annotations.".to_string());
    }

    Ok(LayoutReport {
        table_count: ruby_count,
        min_cell_width: 0,
        min_row_height: min_line_height,
    })
}

fn xml_attr_numbers(xml: &str, tag_prefix: &str, attr: &str) -> Vec<usize> {
    let mut values = Vec::new();
    let mut rest = xml;
    while let Some(tag_start) = rest.find(tag_prefix) {
        rest = &rest[tag_start..];
        let Some(tag_end) = rest.find('>') else {
            break;
        };
        let tag = &rest[..tag_end];
        let pattern = format!("{attr}=\"");
        if let Some(attr_start) = tag.find(&pattern) {
            let value_start = attr_start + pattern.len();
            if let Some(value_end) = tag[value_start..].find('"')
                && let Ok(value) = tag[value_start..value_start + value_end].parse::<usize>()
            {
                values.push(value);
            }
        }
        rest = &rest[tag_end + 1..];
    }
    values
}

#[allow(clippy::too_many_arguments)]
fn annotate_items(
    state: &AppState,
    text: &str,
    lexicon: &mut HashMap<String, LexiconEntry>,
    hard_terms: &HashSet<String>,
    profile: &GradeProfile,
    known_words: &HashSet<String>,
    annotate_unknown: bool,
    missing_terms: &mut HashSet<String>,
) -> Vec<AnnotationItem> {
    let mut items = Vec::new();
    for token in tokenize(text) {
        if !is_word_token(&token) {
            items.push(AnnotationItem {
                text: token,
                annotation: None,
            });
            continue;
        }
        let entry = lookup_entry(state, &token, lexicon);
        let force = should_annotate_word(
            state,
            &token,
            entry.as_ref(),
            hard_terms,
            profile,
            known_words,
        );
        if !force {
            items.push(AnnotationItem {
                text: token,
                annotation: None,
            });
            continue;
        }
        let is_hard = entry.as_ref().is_some_and(|entry| entry.hard)
            || candidate_lemmas(&token)
                .iter()
                .any(|candidate| hard_terms.contains(candidate));
        let Some(entry) = entry else {
            if annotate_unknown {
                missing_terms.insert(normalize_key(&token));
            }
            items.push(AnnotationItem {
                text: token,
                annotation: None,
            });
            continue;
        };
        let ipa = entry.ipa_text();
        let zh = entry.zh_text();
        if ipa.is_empty() && zh.is_empty() {
            if annotate_unknown {
                missing_terms.insert(normalize_key(&token));
            }
            items.push(AnnotationItem {
                text: token,
                annotation: None,
            });
            continue;
        }
        items.push(AnnotationItem {
            text: token,
            annotation: Some(AnnotationParts {
                ipa,
                zh,
                hard: is_hard,
            }),
        });
    }
    items
}

#[derive(Clone)]
#[allow(dead_code)]
struct DocxBlock {
    base: String,
    annotation: Option<AnnotationParts>,
    width_twips: usize,
}

fn items_to_docx_paragraph(
    items: &[AnnotationItem],
    base_hps: usize,
    sizes: TextSizes,
    align: &str,
    title: bool,
) -> String {
    let mut runs = String::new();
    for item in items {
        if item.text.is_empty() {
            continue;
        }
        if let Some(annotation) = &item.annotation {
            runs.push_str(&ruby_run_xml(&item.text, annotation, base_hps));
        } else {
            runs.push_str(&text_run_xml(
                &item.text,
                base_hps,
                None,
                false,
                "Times New Roman",
                "SimSun",
            ));
        }
    }

    let after = if title { 180 } else { 80 };
    docx_paragraph(
        runs,
        align,
        0,
        after,
        ruby_line_height(base_hps, sizes),
        title,
    )
}

fn ruby_annotation_hps(base_hps: usize) -> usize {
    ((base_hps * 48 + 50) / 100).clamp(10, 24)
}

fn ruby_line_height(base_hps: usize, _sizes: TextSizes) -> usize {
    let annotation_hps = ruby_annotation_hps(base_hps);
    (base_hps * 21 + annotation_hps * 7 + 90).clamp(700, 1080)
}

fn ruby_run_xml(base: &str, annotation: &AnnotationParts, base_hps: usize) -> String {
    let color = if annotation.hard {
        Some("B35C00")
    } else {
        Some("444444")
    };
    let annotation_hps = ruby_annotation_hps(base_hps);
    let hps_raise = (base_hps + annotation_hps / 2).clamp(20, 42);
    let mut rt_runs = text_run_xml(
        &annotation.ipa,
        annotation_hps,
        color,
        annotation.hard,
        "Arial",
        "Arial",
    );
    if !annotation.zh.is_empty() {
        rt_runs.push_str(&text_run_xml(
            &format!(" {}", annotation.zh),
            annotation_hps,
            color,
            annotation.hard,
            "Microsoft YaHei",
            "Microsoft YaHei",
        ));
    }
    let base_runs = text_run_xml(
        base,
        base_hps,
        None,
        annotation.hard,
        "Times New Roman",
        "SimSun",
    );

    format!(
        concat!(
            "<w:r><w:ruby><w:rubyPr>",
            "<w:rubyAlign w:val=\"center\"/>",
            "<w:hps w:val=\"{annotation_hps}\"/>",
            "<w:hpsRaise w:val=\"{hps_raise}\"/>",
            "<w:hpsBaseText w:val=\"{base_hps}\"/>",
            "<w:lid w:val=\"en-US\"/>",
            "</w:rubyPr><w:rt>{rt_runs}</w:rt>",
            "<w:rubyBase>{base_runs}</w:rubyBase>",
            "</w:ruby></w:r>"
        ),
        annotation_hps = annotation_hps,
        hps_raise = hps_raise,
        base_hps = base_hps,
        rt_runs = rt_runs,
        base_runs = base_runs
    )
}

fn docx_paragraph(
    runs: String,
    align: &str,
    before: usize,
    after: usize,
    line_twips: usize,
    keep_lines: bool,
) -> String {
    let keep_lines = if keep_lines { "<w:keepLines/>" } else { "" };
    format!(
        concat!(
            "<w:p><w:pPr>{keep_lines}<w:jc w:val=\"{align}\"/>",
            "<w:spacing w:before=\"{before}\" w:after=\"{after}\" w:line=\"{line_twips}\" w:lineRule=\"atLeast\"/>",
            "</w:pPr>{runs}</w:p>"
        ),
        keep_lines = keep_lines,
        align = xml_escape(align),
        before = before,
        after = after,
        line_twips = line_twips,
        runs = runs
    )
}

#[allow(dead_code)]
fn items_to_docx_tables(
    items: &[AnnotationItem],
    base_hps: usize,
    sizes: TextSizes,
    align: &str,
    title: bool,
) -> Vec<String> {
    let blocks = docx_blocks(items, base_hps, sizes);
    if blocks.is_empty() {
        return Vec::new();
    }

    let max_width = if title { 8600 } else { 9360 };
    let mut rows = Vec::new();
    let mut line = Vec::new();
    let mut width = 0usize;

    for block in blocks {
        if !line.is_empty() && width + block.width_twips > max_width {
            rows.push(docx_line_table(&line, base_hps, sizes, align, title));
            line.clear();
            width = 0;
        }
        width += block.width_twips;
        line.push(block);
    }

    if !line.is_empty() {
        rows.push(docx_line_table(&line, base_hps, sizes, align, title));
    }
    rows
}

#[allow(dead_code)]
fn docx_blocks(items: &[AnnotationItem], base_hps: usize, sizes: TextSizes) -> Vec<DocxBlock> {
    let mut blocks = Vec::new();
    let mut plain = String::new();

    for item in items {
        if let Some(annotation) = &item.annotation {
            flush_plain_docx_block(&mut blocks, &mut plain, base_hps, sizes);
            let base = item.text.trim().to_string();
            if !base.is_empty() {
                let mut block = DocxBlock {
                    base,
                    annotation: Some(annotation.clone()),
                    width_twips: 0,
                };
                block.width_twips = docx_block_width(&block, base_hps, sizes);
                blocks.push(block);
            }
            continue;
        }

        if item.text.chars().all(char::is_whitespace) {
            if !plain.is_empty() && !plain.ends_with(' ') {
                plain.push(' ');
            }
            continue;
        }

        if !item.text.chars().any(|ch| ch.is_ascii_alphanumeric()) {
            if !plain.is_empty() {
                plain.push_str(&item.text);
            } else if let Some(last) = blocks.last_mut() {
                last.base.push_str(&item.text);
                last.width_twips = docx_block_width(last, base_hps, sizes);
            } else {
                plain.push_str(&item.text);
            }
            continue;
        }

        let candidate = if plain.is_empty() {
            item.text.clone()
        } else if plain.ends_with(' ') {
            format!("{}{}", plain, item.text)
        } else {
            format!("{} {}", plain, item.text)
        };
        let max_plain_width = if base_hps >= 32 { 3200 } else { 2600 };
        if !plain.is_empty() && estimate_docx_text_width(&candidate, base_hps) > max_plain_width {
            flush_plain_docx_block(&mut blocks, &mut plain, base_hps, sizes);
        }
        if !plain.is_empty() && !plain.ends_with(' ') {
            plain.push(' ');
        }
        plain.push_str(&item.text);
    }

    flush_plain_docx_block(&mut blocks, &mut plain, base_hps, sizes);
    blocks
}

#[allow(dead_code)]
fn flush_plain_docx_block(
    blocks: &mut Vec<DocxBlock>,
    plain: &mut String,
    base_hps: usize,
    sizes: TextSizes,
) {
    let base = plain.trim().to_string();
    plain.clear();
    if base.is_empty() {
        return;
    }
    let mut block = DocxBlock {
        base,
        annotation: None,
        width_twips: 0,
    };
    block.width_twips = docx_block_width(&block, base_hps, sizes);
    blocks.push(block);
}

#[allow(dead_code)]
fn docx_block_width(block: &DocxBlock, base_hps: usize, sizes: TextSizes) -> usize {
    let base_width = estimate_docx_text_width(&block.base, base_hps);
    let annotation_width = block
        .annotation
        .as_ref()
        .map(|annotation| annotation_docx_width(annotation, sizes))
        .unwrap_or(0);
    let safety = if block.annotation.is_some() { 320 } else { 280 };
    let minimum = if block.annotation.is_some() { 720 } else { 560 };
    base_width
        .max(annotation_width)
        .saturating_add(safety)
        .max(minimum)
        .clamp(520, 4300)
}

#[allow(dead_code)]
fn annotation_docx_width(annotation: &AnnotationParts, sizes: TextSizes) -> usize {
    let ipa = estimate_docx_text_width(&annotation.ipa, sizes.ipa_hps);
    let zh = if annotation.zh.is_empty() {
        0
    } else {
        estimate_docx_text_width(&annotation.zh, sizes.zh_hps) + 80
    };
    ipa + zh
}

#[allow(dead_code)]
fn estimate_docx_text_width(text: &str, hps: usize) -> usize {
    text.chars()
        .map(|ch| {
            let factor = if is_cjk(ch) {
                17.0
            } else if ch.is_whitespace() || "ilI.,'`!|:;()[]{}".contains(ch) {
                5.0
            } else if "mwMW@#%&".contains(ch) {
                12.0
            } else if ch.is_ascii_uppercase() {
                10.0
            } else {
                8.6
            };
            (hps as f32 * factor).round() as usize
        })
        .sum::<usize>()
        .max((hps as f32 * 3.2).round() as usize)
}

#[allow(dead_code)]
fn docx_line_table(
    blocks: &[DocxBlock],
    base_hps: usize,
    sizes: TextSizes,
    align: &str,
    title: bool,
) -> String {
    let table_width: usize = blocks.iter().map(|block| block.width_twips).sum();
    let after = if title { 160 } else { 120 };
    let row_height = stacked_row_height(base_hps, sizes, after);
    let mut xml = format!(
        concat!(
            "<w:tbl><w:tblPr><w:tblW w:w=\"{table_width}\" w:type=\"dxa\"/>",
            "<w:jc w:val=\"{align}\"/><w:tblLayout w:type=\"fixed\"/>",
            "<w:tblBorders><w:top w:val=\"nil\"/><w:left w:val=\"nil\"/><w:bottom w:val=\"nil\"/><w:right w:val=\"nil\"/><w:insideH w:val=\"nil\"/><w:insideV w:val=\"nil\"/></w:tblBorders>",
            "<w:tblCellMar><w:top w:w=\"0\" w:type=\"dxa\"/><w:left w:w=\"45\" w:type=\"dxa\"/><w:bottom w:w=\"0\" w:type=\"dxa\"/><w:right w:w=\"45\" w:type=\"dxa\"/></w:tblCellMar>",
            "</w:tblPr><w:tblGrid>"
        ),
        table_width = table_width,
        align = xml_escape(align)
    );

    for block in blocks {
        xml.push_str(&format!("<w:gridCol w:w=\"{}\"/>", block.width_twips));
    }

    xml.push_str(&format!(
        "</w:tblGrid><w:tr><w:trPr><w:cantSplit/><w:trHeight w:val=\"{}\" w:hRule=\"atLeast\"/></w:trPr>",
        row_height
    ));
    for block in blocks {
        xml.push_str(&docx_stacked_cell(block, base_hps, sizes, after));
    }
    xml.push_str("</w:tr></w:tbl>");
    xml
}

#[allow(dead_code)]
fn annotation_line_height(sizes: TextSizes) -> usize {
    let hps = sizes.ipa_hps.max(sizes.zh_hps);
    (hps * 13 + 100).clamp(250, 430)
}

#[allow(dead_code)]
fn base_line_height(base_hps: usize) -> usize {
    (base_hps * 14 + 110).clamp(430, 780)
}

#[allow(dead_code)]
fn stacked_row_height(base_hps: usize, sizes: TextSizes, after: usize) -> usize {
    annotation_line_height(sizes) + base_line_height(base_hps) + after + 60
}

#[allow(dead_code)]
fn docx_stacked_cell(block: &DocxBlock, base_hps: usize, sizes: TextSizes, after: usize) -> String {
    let base_runs = docx_base_runs(block, base_hps);
    let annotation = if block.annotation.is_some() {
        docx_cell_paragraph(
            docx_annotation_runs(block, sizes),
            0,
            0,
            annotation_line_height(sizes),
            true,
        )
    } else {
        String::new()
    };
    let base_before = if block.annotation.is_some() {
        0
    } else {
        annotation_line_height(sizes)
    };
    format!(
        concat!(
            "<w:tc><w:tcPr><w:tcW w:w=\"{width_twips}\" w:type=\"dxa\"/><w:noWrap/><w:vAlign w:val=\"top\"/></w:tcPr>",
            "{annotation}{base}</w:tc>"
        ),
        width_twips = block.width_twips,
        annotation = annotation,
        base = docx_cell_paragraph(
            base_runs,
            base_before,
            after,
            base_line_height(base_hps),
            false
        )
    )
}

#[allow(dead_code)]
fn docx_annotation_runs(block: &DocxBlock, sizes: TextSizes) -> String {
    let Some(annotation) = &block.annotation else {
        return String::new();
    };

    let color = if annotation.hard {
        Some("B35C00")
    } else {
        Some("444444")
    };
    let mut runs = text_run_xml(
        &annotation.ipa,
        sizes.ipa_hps,
        color,
        annotation.hard,
        "Arial",
        "Arial",
    );
    if !annotation.zh.is_empty() {
        runs.push_str(&text_run_xml(
            &format!(" {}", annotation.zh),
            sizes.zh_hps,
            color,
            annotation.hard,
            "Microsoft YaHei",
            "Microsoft YaHei",
        ));
    }

    runs
}

#[allow(dead_code)]
fn docx_base_runs(block: &DocxBlock, base_hps: usize) -> String {
    let hard = block
        .annotation
        .as_ref()
        .is_some_and(|annotation| annotation.hard);
    text_run_xml(
        &block.base,
        base_hps,
        None,
        hard,
        "Times New Roman",
        "SimSun",
    )
}

#[allow(dead_code)]
fn docx_cell_paragraph(
    runs: String,
    before: usize,
    after: usize,
    line_twips: usize,
    keep_next: bool,
) -> String {
    let keep_next = if keep_next { "<w:keepNext/>" } else { "" };
    format!(
        concat!(
            "<w:p><w:pPr><w:keepLines/>{keep_next}<w:jc w:val=\"center\"/>",
            "<w:spacing w:before=\"{before}\" w:after=\"{after}\" w:line=\"{line_twips}\" w:lineRule=\"atLeast\"/></w:pPr>",
            "{runs}</w:p>"
        ),
        keep_next = keep_next,
        before = before,
        after = after,
        line_twips = line_twips,
        runs = runs
    )
}

fn text_run_xml(
    text: &str,
    size_hps: usize,
    color: Option<&str>,
    bold: bool,
    ascii_font: &str,
    east_asia_font: &str,
) -> String {
    format!(
        "<w:r>{}<w:t xml:space=\"preserve\">{}</w:t></w:r>",
        run_props_xml(size_hps, color, bold, ascii_font, east_asia_font),
        xml_escape(text)
    )
}

fn items_to_preview_html(items: &[AnnotationItem]) -> String {
    let mut html = String::new();
    for item in items {
        if let Some(annotation) = &item.annotation {
            let class_name = if annotation.hard {
                "preview-token hard"
            } else {
                "preview-token"
            };
            html.push_str(&format!(
                "<ruby class=\"{class_name}\"><span class=\"preview-base\">{}</span><rt class=\"preview-rt\"><span class=\"preview-ipa\">{}</span>",
                xml_escape(&item.text),
                xml_escape(&annotation.ipa)
            ));
            if !annotation.zh.is_empty() {
                html.push_str(&format!(
                    "<span class=\"preview-zh\"> {}</span>",
                    xml_escape(&annotation.zh)
                ));
            }
            html.push_str("</rt></ruby>");
        } else if is_word_token(&item.text) {
            html.push_str(&format!(
                "<span class=\"preview-base\">{}</span>",
                xml_escape(&item.text)
            ));
        } else {
            html.push_str(&xml_escape(&item.text));
        }
    }
    html
}

fn run_props_xml(
    size_hps: usize,
    color: Option<&str>,
    bold: bool,
    ascii_font: &str,
    east_asia_font: &str,
) -> String {
    let mut xml = format!(
        "<w:rPr><w:rFonts w:ascii=\"{}\" w:hAnsi=\"{}\" w:eastAsia=\"{}\"/><w:sz w:val=\"{}\"/><w:szCs w:val=\"{}\"/>",
        xml_escape(ascii_font),
        xml_escape(ascii_font),
        xml_escape(east_asia_font),
        size_hps,
        size_hps
    );
    if bold {
        xml.push_str("<w:b/><w:bCs/>");
    }
    xml.push_str("<w:noProof/>");
    if let Some(color) = color {
        xml.push_str(&format!("<w:color w:val=\"{}\"/>", xml_escape(color)));
    }
    xml.push_str("</w:rPr>");
    xml
}

fn build_docx(paragraphs: &[String], sizes: TextSizes) -> Result<Vec<u8>, String> {
    let (page_width_twips, page_height_twips) = page_size_twips(sizes.page_size);
    let document_xml = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
            "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" ",
            "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" ",
            "xmlns:xml=\"http://www.w3.org/XML/1998/namespace\"><w:body>{}",
            "<w:sectPr><w:pgSz w:w=\"{}\" w:h=\"{}\"/><w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\" w:header=\"720\" w:footer=\"720\" w:gutter=\"0\"/></w:sectPr>",
            "</w:body></w:document>"
        ),
        paragraphs.join(""),
        page_width_twips,
        page_height_twips
    );
    let styles_xml = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
            "<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
            "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\">",
            "<w:name w:val=\"Normal\"/><w:rPr><w:rFonts w:ascii=\"Times New Roman\" w:hAnsi=\"Times New Roman\" w:eastAsia=\"SimSun\"/><w:sz w:val=\"{}\"/><w:szCs w:val=\"{}\"/></w:rPr>",
            "</w:style></w:styles>"
        ),
        sizes.english_hps, sizes.english_hps
    );
    let content_types = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">",
        "<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>",
        "<Default Extension=\"xml\" ContentType=\"application/xml\"/>",
        "<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>",
        "<Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>",
        "<Override PartName=\"/word/settings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml\"/>",
        "</Types>"
    );
    let rels = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
        "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>",
        "</Relationships>"
    );
    let document_rels = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"></Relationships>"
    );
    let settings = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<w:settings xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:zoom w:percent=\"100\"/></w:settings>"
    );

    let mut zip = ZipWriter::new();
    zip.add("[Content_Types].xml", content_types.as_bytes())?;
    zip.add("_rels/.rels", rels.as_bytes())?;
    zip.add("word/document.xml", document_xml.as_bytes())?;
    zip.add("word/_rels/document.xml.rels", document_rels.as_bytes())?;
    zip.add("word/styles.xml", styles_xml.as_bytes())?;
    zip.add("word/settings.xml", settings.as_bytes())?;
    Ok(zip.finish())
}

fn page_size_twips(page_size: PageSize) -> (usize, usize) {
    let to_twips = |value: f32| {
        let inches = if page_size.unit == "mm" {
            value / 25.4
        } else {
            value
        };
        (inches * 1440.0).round().max(1.0) as usize
    };
    (to_twips(page_size.width), to_twips(page_size.height))
}

fn xml_escape(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

struct ZipEntry {
    name: String,
    crc: u32,
    size: u32,
    offset: u32,
}

struct ZipWriter {
    data: Vec<u8>,
    entries: Vec<ZipEntry>,
}

impl ZipWriter {
    fn new() -> Self {
        Self {
            data: Vec::new(),
            entries: Vec::new(),
        }
    }

    fn add(&mut self, name: &str, bytes: &[u8]) -> Result<(), String> {
        let size = u32::try_from(bytes.len()).map_err(|_| "DOCX part too large".to_string())?;
        let offset =
            u32::try_from(self.data.len()).map_err(|_| "DOCX package too large".to_string())?;
        let crc = crc32(bytes);
        write_u32(&mut self.data, 0x0403_4b50);
        write_u16(&mut self.data, 20);
        write_u16(&mut self.data, 0);
        write_u16(&mut self.data, 0);
        write_u16(&mut self.data, 0);
        write_u16(&mut self.data, 0);
        write_u32(&mut self.data, crc);
        write_u32(&mut self.data, size);
        write_u32(&mut self.data, size);
        write_u16(&mut self.data, name.len() as u16);
        write_u16(&mut self.data, 0);
        self.data.extend_from_slice(name.as_bytes());
        self.data.extend_from_slice(bytes);
        self.entries.push(ZipEntry {
            name: name.to_string(),
            crc,
            size,
            offset,
        });
        Ok(())
    }

    fn finish(mut self) -> Vec<u8> {
        let central_offset = self.data.len() as u32;
        for entry in &self.entries {
            write_u32(&mut self.data, 0x0201_4b50);
            write_u16(&mut self.data, 20);
            write_u16(&mut self.data, 20);
            write_u16(&mut self.data, 0);
            write_u16(&mut self.data, 0);
            write_u16(&mut self.data, 0);
            write_u16(&mut self.data, 0);
            write_u32(&mut self.data, entry.crc);
            write_u32(&mut self.data, entry.size);
            write_u32(&mut self.data, entry.size);
            write_u16(&mut self.data, entry.name.len() as u16);
            write_u16(&mut self.data, 0);
            write_u16(&mut self.data, 0);
            write_u16(&mut self.data, 0);
            write_u16(&mut self.data, 0);
            write_u32(&mut self.data, 0);
            write_u32(&mut self.data, entry.offset);
            self.data.extend_from_slice(entry.name.as_bytes());
        }
        let central_size = self.data.len() as u32 - central_offset;
        write_u32(&mut self.data, 0x0605_4b50);
        write_u16(&mut self.data, 0);
        write_u16(&mut self.data, 0);
        write_u16(&mut self.data, self.entries.len() as u16);
        write_u16(&mut self.data, self.entries.len() as u16);
        write_u32(&mut self.data, central_size);
        write_u32(&mut self.data, central_offset);
        write_u16(&mut self.data, 0);
        self.data
    }
}

fn write_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn profiles_json(state: &AppState) -> String {
    let profiles = state
        .profiles
        .iter()
        .map(|profile| {
            format!(
                "{{\"code\":\"{}\",\"label\":\"{}\",\"estimated_vocab\":{},\"note\":\"{}\"}}",
                json_escape(&profile.code),
                json_escape(&profile.label),
                profile.estimated_vocab,
                json_escape(&profile.note)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"profiles\":[{profiles}],\"default_grade\":\"P4\"}}")
}

fn handle_dictionary(state: &AppState, body: &str) -> (u16, String) {
    let word = json_string(body, "word").unwrap_or_default();
    let word = word.trim();
    if word.is_empty() || word.len() > 120 {
        return (400, "{\"error\":\"请输入需要查询的英文单词\"}".to_string());
    }

    let mut forms = Vec::new();
    let mut seen = HashSet::new();
    for candidate in candidate_lemmas(word) {
        if seen.insert(candidate.clone()) {
            forms.push(candidate);
        }
    }
    let mut lexicon = state.seed_lexicon.clone();
    let entry = lookup_entry(state, word, &mut lexicon);
    if let Some(entry) = entry {
        if seen.insert(entry.term.clone()) {
            forms.insert(0, entry.term.clone());
        }
        let forms_json = forms
            .iter()
            .map(|form| format!("\"{}\"", json_escape(form)))
            .collect::<Vec<_>>()
            .join(",");
        (
            200,
            format!(
                "{{\"ok\":true,\"found\":true,\"word\":\"{}\",\"term\":\"{}\",\"ipa\":\"{}\",\"definition\":\"{}\",\"forms\":[{}]}}",
                json_escape(word),
                json_escape(&entry.term),
                json_escape(&entry.ipa),
                json_escape(&entry.zh),
                forms_json
            ),
        )
    } else {
        let forms_json = forms
            .iter()
            .map(|form| format!("\"{}\"", json_escape(form)))
            .collect::<Vec<_>>()
            .join(",");
        (
            200,
            format!(
                "{{\"ok\":true,\"found\":false,\"word\":\"{}\",\"forms\":[{}]}}",
                json_escape(word),
                forms_json
            ),
        )
    }
}

fn handle_preview(state: &AppState, body: &str) -> (u16, String) {
    let article = json_string(body, "article").unwrap_or_default();
    let title = json_string(body, "title").unwrap_or_default();
    let grade = json_string(body, "grade").unwrap_or_else(|| "P4".to_string());
    let custom_words = json_string(body, "customWords").unwrap_or_default();
    let annotate_unknown = json_bool(body, "annotateUnknown").unwrap_or(true);
    if article.trim().is_empty() {
        return (400, "{\"error\":\"请先粘贴英文文章。\"}".to_string());
    }

    match render_preview_html(
        state,
        &article,
        &title,
        &custom_words,
        annotate_unknown,
        &grade,
    ) {
        Ok((html, missing)) => (
            200,
            format!(
                "{{\"ok\":true,\"html\":\"{}\",\"missingCount\":{}}}",
                json_escape(&html),
                missing.len()
            ),
        ),
        Err(error) => (400, format!("{{\"error\":\"{}\"}}", json_escape(&error))),
    }
}

fn handle_ai_enhance(body: &str) -> (u16, String) {
    let article = json_string(body, "article").unwrap_or_default();
    let grade = json_string(body, "grade").unwrap_or_else(|| "P4".to_string());
    let endpoint = json_string(body, "endpoint").unwrap_or_default();
    let model = json_string(body, "model").unwrap_or_default();
    let api_key = json_string(body, "apiKey").unwrap_or_default();
    if article.trim().is_empty() {
        return (400, "{\"error\":\"请先粘贴英文文章。\"}".to_string());
    }
    if endpoint.trim().is_empty() || model.trim().is_empty() {
        return (400, "{\"error\":\"请填写接口地址和模型。\"}".to_string());
    }

    match call_ai_annotations(&endpoint, &model, &api_key, &grade, &article) {
        Ok(annotations) if !annotations.trim().is_empty() => (
            200,
            format!(
                "{{\"ok\":true,\"annotations\":\"{}\"}}",
                json_escape(&annotations)
            ),
        ),
        Ok(_) => (400, "{\"error\":\"AI 未返回可用标注。\"}".to_string()),
        Err(error) => (400, format!("{{\"error\":\"{}\"}}", json_escape(&error))),
    }
}

fn handle_network_lexicon(state: &AppState, body: &str) -> (u16, String) {
    let article = json_string(body, "article").unwrap_or_default();
    let grade = json_string(body, "grade").unwrap_or_else(|| "P4".to_string());
    let custom_words = json_string(body, "customWords").unwrap_or_default();
    let endpoint = json_string(body, "endpoint").unwrap_or_default();
    let api_key = json_string(body, "apiKey").unwrap_or_default();
    if article.trim().is_empty() {
        return (400, "{\"error\":\"请先粘贴英文文章。\"}".to_string());
    }
    if endpoint.trim().is_empty() {
        return (400, "{\"error\":\"请填写网络词库接口。\"}".to_string());
    }

    let words = match network_lexicon_candidates(state, &article, &custom_words, &grade) {
        Ok(words) => words,
        Err(error) => return (400, format!("{{\"error\":\"{}\"}}", json_escape(&error))),
    };
    if words.is_empty() {
        return (
            200,
            "{\"ok\":true,\"annotations\":\"\",\"count\":0}".to_string(),
        );
    }

    match call_network_lexicon(&endpoint, &api_key, &words) {
        Ok(annotations) => {
            let count = annotations
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count();
            (
                200,
                format!(
                    "{{\"ok\":true,\"annotations\":\"{}\",\"count\":{}}}",
                    json_escape(&annotations),
                    count
                ),
            )
        }
        Err(error) => (400, format!("{{\"error\":\"{}\"}}", json_escape(&error))),
    }
}

fn handle_generate_pdf(state: &AppState, body: &str) -> (u16, String) {
    let article = json_string(body, "article").unwrap_or_default();
    let title = json_string(body, "title").unwrap_or_default();
    let grade = json_string(body, "grade").unwrap_or_else(|| "P4".to_string());
    let custom_words = json_string(body, "customWords").unwrap_or_default();
    let annotate_unknown = json_bool(body, "annotateUnknown").unwrap_or(true);
    let sizes = text_sizes_from_json(body);
    if article.trim().is_empty() {
        return (400, "{\"error\":\"请先粘贴英文文章。\"}".to_string());
    }

    let filename = format!(
        "{}_{}.pdf",
        safe_title(if title.trim().is_empty() {
            "annotated_article"
        } else {
            &title
        }),
        unique_suffix()
    );
    let output_path = app_output_dir().join(&filename);
    match generate_pdf(
        state,
        &article,
        &output_path,
        &title,
        &custom_words,
        annotate_unknown,
        &grade,
        sizes,
    ) {
        Ok(missing) => {
            let download_url = format!("/download/{}", url_encode(&filename));
            (
                200,
                format!(
                    "{{\"ok\":true,\"filename\":\"{}\",\"downloadUrl\":\"{}\",\"missingCount\":{}}}",
                    json_escape(&filename),
                    json_escape(&download_url),
                    missing.len()
                ),
            )
        }
        Err(error) => (400, format!("{{\"error\":\"{}\"}}", json_escape(&error))),
    }
}

fn handle_generate_docx(state: &AppState, body: &str) -> (u16, String) {
    let article = json_string(body, "article").unwrap_or_default();
    let title = json_string(body, "title").unwrap_or_default();
    let grade = json_string(body, "grade").unwrap_or_else(|| "P4".to_string());
    let custom_words = json_string(body, "customWords").unwrap_or_default();
    let annotate_unknown = json_bool(body, "annotateUnknown").unwrap_or(true);
    let sizes = text_sizes_from_json(body);
    if article.trim().is_empty() {
        return (400, "{\"error\":\"请先粘贴英文文章。\"}".to_string());
    }

    let filename = format!(
        "{}_{}.docx",
        safe_title(if title.trim().is_empty() {
            "annotated_article"
        } else {
            &title
        }),
        unique_suffix()
    );
    let output_path = app_output_dir().join(&filename);
    match generate_docx(
        state,
        &article,
        &output_path,
        &title,
        &custom_words,
        annotate_unknown,
        &grade,
        sizes,
    ) {
        Ok(missing) => {
            let download_url = format!("/download/{}", url_encode(&filename));
            (
                200,
                format!(
                    "{{\"ok\":true,\"filename\":\"{}\",\"downloadUrl\":\"{}\",\"missingCount\":{}}}",
                    json_escape(&filename),
                    json_escape(&download_url),
                    missing.len()
                ),
            )
        }
        Err(error) => (400, format!("{{\"error\":\"{}\"}}", json_escape(&error))),
    }
}

fn network_lexicon_candidates(
    state: &AppState,
    article: &str,
    custom_words: &str,
    grade: &str,
) -> Result<Vec<String>, String> {
    let (mut lexicon, hard_terms, profile, known_words) =
        annotation_context(state, custom_words, grade)?;
    let mut seen = HashSet::new();
    let mut words = Vec::new();
    for token in tokenize(article) {
        if !is_word_token(&token) {
            continue;
        }
        let key = normalize_key(&token);
        if key.len() < 3 || seen.contains(&key) {
            continue;
        }
        let entry = lookup_entry(state, &token, &mut lexicon);
        if entry.as_ref().is_some_and(|entry| entry.hard) {
            continue;
        }
        if should_annotate_word(
            state,
            &token,
            entry.as_ref(),
            &hard_terms,
            profile,
            &known_words,
        ) {
            seen.insert(key.clone());
            words.push(key);
            if words.len() >= 80 {
                break;
            }
        }
    }
    Ok(words)
}

fn call_network_lexicon(endpoint: &str, api_key: &str, words: &[String]) -> Result<String, String> {
    let endpoint = endpoint.trim();
    if !endpoint.contains("{word}") {
        return Err("词库接口必须包含 {word} 占位符。".to_string());
    }
    let temp_dir = std::env::temp_dir().join(format!("cijing-lexicon-{}", unique_suffix()));
    fs::create_dir_all(&temp_dir).map_err(|err| err.to_string())?;
    let words_path = temp_dir.join("words.txt");
    let script_path = temp_dir.join("dict.ps1");
    fs::write(&words_path, words.join("\n")).map_err(|err| err.to_string())?;
    fs::write(&script_path, network_lexicon_powershell_script()).map_err(|err| err.to_string())?;
    let result = run_network_lexicon_powershell(&script_path, endpoint, api_key, &words_path);
    let _ = fs::remove_dir_all(&temp_dir);
    result.map(|text| clean_ai_annotations(&text))
}

fn network_lexicon_powershell_script() -> &'static str {
    r#"
param(
  [string]$EndpointTemplate,
  [string]$ApiKey,
  [string]$WordsPath
)
$ErrorActionPreference = "Stop"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$headers = @{ "Accept" = "application/json" }
if ($ApiKey -and $ApiKey.Trim().Length -gt 0) {
  $headers["Authorization"] = "Bearer $ApiKey"
}

function First-Scalar($value, [int]$depth) {
  if ($null -eq $value -or $depth -gt 6) { return $null }
  if ($value -is [string]) {
    $text = $value.Trim()
    if ($text.Length -gt 0) { return $text }
    return $null
  }
  if ($value -is [System.ValueType]) { return [string]$value }
  if ($value -is [System.Collections.IEnumerable] -and -not ($value -is [string])) {
    foreach ($item in $value) {
      $found = First-Scalar $item ($depth + 1)
      if ($found) { return $found }
    }
    return $null
  }
  foreach ($prop in $value.PSObject.Properties) {
    $found = First-Scalar $prop.Value ($depth + 1)
    if ($found) { return $found }
  }
  return $null
}

function Find-Field($obj, [string[]]$names, [int]$depth) {
  if ($null -eq $obj -or $depth -gt 6) { return $null }
  foreach ($prop in $obj.PSObject.Properties) {
    $name = $prop.Name.ToLowerInvariant()
    foreach ($wanted in $names) {
      if ($name -eq $wanted -or $name.Contains($wanted)) {
        $scalar = First-Scalar $prop.Value 0
        if ($scalar) { return $scalar }
      }
    }
  }
  foreach ($prop in $obj.PSObject.Properties) {
    $found = Find-Field $prop.Value $names ($depth + 1)
    if ($found) { return $found }
  }
  return $null
}

$words = Get-Content -LiteralPath $WordsPath | Where-Object { $_.Trim().Length -gt 0 }
foreach ($word in $words) {
  $encoded = [uri]::EscapeDataString($word)
  $uri = $EndpointTemplate.Replace("{word}", $encoded)
  try {
    $response = Invoke-RestMethod -Uri $uri -Method Get -Headers $headers -TimeoutSec 15
    $ipa = Find-Field $response @("ipa", "phonetic", "pronunciation", "usphone", "ukphone", "phone", "pron") 0
    $zh = Find-Field $response @("zh", "chinese", "translation", "trans", "explain", "meaning", "definition") 0
    if ($ipa -and $zh) {
      $ipa = ($ipa -replace "`r|`n", " ").Trim()
      $zh = ($zh -replace "`r|`n", " ").Trim()
      if ($ipa.Length -gt 0 -and $zh.Length -gt 0) {
        "$word=$ipa=$zh"
      }
    }
  } catch {
  }
}
"#
}

fn run_network_lexicon_powershell(
    script_path: &Path,
    endpoint: &str,
    api_key: &str,
    words_path: &Path,
) -> Result<String, String> {
    let shell = if Command::new("powershell.exe")
        .arg("-NoProfile")
        .arg("-Command")
        .arg("$PSVersionTable.PSVersion")
        .output()
        .is_ok()
    {
        "powershell.exe"
    } else {
        "pwsh"
    };
    let mut child = Command::new(shell)
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &script_path.to_string_lossy(),
            "-EndpointTemplate",
            endpoint,
            "-ApiKey",
            api_key,
            "-WordsPath",
            &words_path.to_string_lossy(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("无法启动网络词库请求：{err}"))?;

    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|err| err.to_string())? {
            break status;
        }
        if started.elapsed() > Duration::from_secs(75) {
            let _ = child.kill();
            return Err("网络词库请求超时。".to_string());
        }
        thread::sleep(Duration::from_millis(200));
    };

    let mut stdout = String::new();
    if let Some(mut stream) = child.stdout.take() {
        let _ = stream.read_to_string(&mut stdout);
    }
    let mut stderr = String::new();
    if let Some(mut stream) = child.stderr.take() {
        let _ = stream.read_to_string(&mut stderr);
    }
    if !status.success() {
        let message = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        return Err(format!("网络词库查询失败：{message}"));
    }
    if stdout.trim().is_empty() {
        return Err("网络词库没有返回可用的中文释义和音标。".to_string());
    }
    Ok(stdout)
}

fn call_ai_annotations(
    endpoint: &str,
    model: &str,
    api_key: &str,
    grade: &str,
    article: &str,
) -> Result<String, String> {
    let system_prompt = concat!(
        "你是英语精读标注助手。请只输出自定义标注行，不要解释，不要 Markdown。",
        "每行格式必须是 word=IPA=中文短释义。",
        "IPA 使用国际音标，不要用 KK 或拼音；中文释义不超过 8 个汉字。",
        "只选择文中需要标注或你能明显改进音标/释义的词。"
    );
    let user_prompt = format!("学生年级：{grade}\n英文文章：\n{article}");
    let request_body = format!(
        concat!(
            "{{\"model\":\"{}\",\"temperature\":0.1,",
            "\"messages\":[",
            "{{\"role\":\"system\",\"content\":\"{}\"}},",
            "{{\"role\":\"user\",\"content\":\"{}\"}}",
            "]}}"
        ),
        json_escape(model),
        json_escape(system_prompt),
        json_escape(&user_prompt)
    );

    let temp_dir = std::env::temp_dir().join(format!("cijing-ai-{}", unique_suffix()));
    fs::create_dir_all(&temp_dir).map_err(|err| err.to_string())?;
    let body_path = temp_dir.join("request.json");
    let script_path = temp_dir.join("call.ps1");
    fs::write(&body_path, request_body).map_err(|err| err.to_string())?;
    fs::write(&script_path, ai_powershell_script()).map_err(|err| err.to_string())?;
    let result = run_ai_powershell(&script_path, endpoint, api_key, &body_path);
    let _ = fs::remove_dir_all(&temp_dir);
    result.map(|text| clean_ai_annotations(&text))
}

fn ai_powershell_script() -> &'static str {
    r#"
param(
  [string]$Endpoint,
  [string]$ApiKey,
  [string]$BodyPath
)
$ErrorActionPreference = "Stop"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$headers = @{ "Content-Type" = "application/json" }
if ($ApiKey -and $ApiKey.Trim().Length -gt 0) {
  $headers["Authorization"] = "Bearer $ApiKey"
}
$body = Get-Content -Raw -LiteralPath $BodyPath
$response = Invoke-RestMethod -Uri $Endpoint -Method Post -Headers $headers -Body $body -TimeoutSec 45
if ($response.choices -and $response.choices.Count -gt 0) {
  $response.choices[0].message.content
} elseif ($response.content) {
  $response.content
} else {
  $response | ConvertTo-Json -Depth 20
}
"#
}

fn run_ai_powershell(
    script_path: &Path,
    endpoint: &str,
    api_key: &str,
    body_path: &Path,
) -> Result<String, String> {
    let shell = if Command::new("powershell.exe")
        .arg("-NoProfile")
        .arg("-Command")
        .arg("$PSVersionTable.PSVersion")
        .output()
        .is_ok()
    {
        "powershell.exe"
    } else {
        "pwsh"
    };
    let mut child = Command::new(shell)
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &script_path.to_string_lossy(),
            "-Endpoint",
            endpoint,
            "-ApiKey",
            api_key,
            "-BodyPath",
            &body_path.to_string_lossy(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("无法启动网络请求：{err}"))?;

    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|err| err.to_string())? {
            break status;
        }
        if started.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            return Err("AI 增强请求超时。".to_string());
        }
        thread::sleep(Duration::from_millis(200));
    };

    let mut stdout = String::new();
    if let Some(mut stream) = child.stdout.take() {
        let _ = stream.read_to_string(&mut stdout);
    }
    let mut stderr = String::new();
    if let Some(mut stream) = child.stderr.take() {
        let _ = stream.read_to_string(&mut stderr);
    }
    if !status.success() {
        let message = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        return Err(format!("AI 增强失败：{message}"));
    }
    Ok(stdout)
}

fn clean_ai_annotations(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty()
                && !line.starts_with("```")
                && line.matches('=').count() >= 2
                && line.len() <= 120
        })
        .map(|line| line.trim_matches(['-', '*', ' ']).to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

fn text_sizes_from_json(body: &str) -> TextSizes {
    let default = TextSizes::default_body();
    let english_hps = json_number(body, "englishHps")
        .map(|value| (value.round() as isize).clamp(18, 48) as usize)
        .unwrap_or(default.english_hps);
    let ipa_hps = json_number(body, "ipaHps")
        .map(|value| (value.round() as isize).clamp(8, 32) as usize)
        .unwrap_or(default.ipa_hps);
    let zh_hps = json_number(body, "zhHps")
        .map(|value| (value.round() as isize).clamp(8, 34) as usize)
        .unwrap_or(default.zh_hps);
    let line_height = json_number(body, "lineHeight")
        .map(|value| value.clamp(1.45, 3.2) as f32)
        .unwrap_or(default.line_height);
    let word_spacing_pt = json_number(body, "wordSpacing")
        .map(|value| value.clamp(-0.5, 12.0) as f32)
        .unwrap_or(default.word_spacing_pt);
    let page_size_code = json_string(body, "pageSize").unwrap_or_else(|| "letter".to_string());
    let page_size = if page_size_code.trim().eq_ignore_ascii_case("custom") {
        PageSize {
            width: json_number(body, "customPageWidth")
                .map(|value| value.clamp(90.0, 500.0) as f32)
                .unwrap_or(210.0),
            height: json_number(body, "customPageHeight")
                .map(|value| value.clamp(120.0, 700.0) as f32)
                .unwrap_or(297.0),
            unit: "mm",
        }
    } else {
        page_size_from_code(&page_size_code)
    };

    TextSizes {
        english_hps,
        ipa_hps,
        zh_hps,
        line_height,
        word_spacing_pt,
        page_size,
    }
}

fn safe_title(title: &str) -> String {
    let mut out = String::new();
    let mut last_underscore = false;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() || is_cjk(ch) || matches!(ch, '_' | '-') {
            out.push(ch);
            last_underscore = false;
        } else if !last_underscore {
            out.push('_');
            last_underscore = true;
        }
    }
    let out = out.trim_matches('_').to_string();
    if out.is_empty() {
        "annotated_article".to_string()
    } else {
        out
    }
}

fn unique_suffix() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{:x}{:04x}", now & 0xffff_ffff, counter & 0xffff)
}

fn json_string(body: &str, key: &str) -> Option<String> {
    let marker = format!("\"{}\"", key);
    let mut index = body.find(&marker)? + marker.len();
    index = body[index..].find(':')? + index + 1;
    skip_json_ws(body, &mut index);
    if body[index..].chars().next()? != '"' {
        return None;
    }
    index += 1;
    let mut out = String::new();
    while index < body.len() {
        let ch = body[index..].chars().next()?;
        index += ch.len_utf8();
        match ch {
            '"' => return Some(out),
            '\\' => {
                let escaped = body[index..].chars().next()?;
                index += escaped.len_utf8();
                match escaped {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'b' => out.push('\u{0008}'),
                    'f' => out.push('\u{000c}'),
                    'u' => {
                        if index + 4 <= body.len() {
                            if let Ok(value) = u16::from_str_radix(&body[index..index + 4], 16)
                                && let Some(decoded) = char::from_u32(value as u32)
                            {
                                out.push(decoded);
                            }
                            index += 4;
                        }
                    }
                    other => out.push(other),
                }
            }
            other => out.push(other),
        }
    }
    None
}

fn json_bool(body: &str, key: &str) -> Option<bool> {
    let marker = format!("\"{}\"", key);
    let mut index = body.find(&marker)? + marker.len();
    index = body[index..].find(':')? + index + 1;
    skip_json_ws(body, &mut index);
    if body[index..].starts_with("true") {
        Some(true)
    } else if body[index..].starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn json_number(body: &str, key: &str) -> Option<f64> {
    let marker = format!("\"{}\"", key);
    let mut index = body.find(&marker)? + marker.len();
    index = body[index..].find(':')? + index + 1;
    skip_json_ws(body, &mut index);
    let start = index;
    while index < body.len() {
        let ch = body[index..].chars().next()?;
        if ch.is_ascii_digit() || matches!(ch, '-' | '+' | '.' | 'e' | 'E') {
            index += ch.len_utf8();
        } else {
            break;
        }
    }
    if index == start {
        return None;
    }
    body[start..index].parse().ok()
}

fn skip_json_ws(body: &str, index: &mut usize) {
    while *index < body.len()
        && body[*index..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
    {
        *index += body[*index..].chars().next().unwrap().len_utf8();
    }
}

fn json_escape(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch < ' ' => out.push_str(&format!("\\u{:04x}", ch as u32)),
            _ => out.push(ch),
        }
    }
    out
}

fn url_encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn url_decode(text: &str) -> String {
    let mut bytes = Vec::new();
    let mut iter = text.as_bytes().iter().copied().peekable();
    while let Some(byte) = iter.next() {
        if byte == b'%' {
            let a = iter.next();
            let b = iter.next();
            if let (Some(a), Some(b)) = (a, b)
                && let Ok(value) = u8::from_str_radix(&format!("{}{}", a as char, b as char), 16)
            {
                bytes.push(value);
                continue;
            }
            bytes.push(byte);
        } else if byte == b'+' {
            bytes.push(b' ');
        } else {
            bytes.push(byte);
        }
    }
    String::from_utf8_lossy(&bytes).to_string()
}

fn handle_client(mut stream: TcpStream, state: Arc<AppState>) {
    let Ok((method, path, body)) = read_request(&mut stream) else {
        return;
    };
    let (status, mime, body_bytes, extra_headers) = route_request(&state, &method, &path, &body);
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Internal Server Error",
    };
    let mut response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nContent-Type: {mime}\r\nCache-Control: no-store\r\nContent-Security-Policy: default-src 'self'; connect-src 'self'; img-src 'self' data:; style-src 'self'; script-src 'self'; base-uri 'none'; frame-ancestors 'none'\r\nCross-Origin-Resource-Policy: same-origin\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n",
        body_bytes.len()
    );
    for header in extra_headers {
        response.push_str(&header);
        response.push_str("\r\n");
    }
    response.push_str("\r\n");
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.write_all(&body_bytes);
}

fn read_request(stream: &mut TcpStream) -> Result<(String, String, String), String> {
    let mut data = Vec::new();
    let mut buf = [0u8; 8192];
    let mut header_end = None;
    let mut content_length = 0usize;
    loop {
        let read = stream.read(&mut buf).map_err(|err| err.to_string())?;
        if read == 0 {
            break;
        }
        data.extend_from_slice(&buf[..read]);
        if header_end.is_none() {
            if data.len() > MAX_HTTP_HEADER_BYTES {
                return Err("HTTP 请求头过大。".to_string());
            }
            header_end = find_subsequence(&data, b"\r\n\r\n");
            if let Some(end) = header_end {
                let headers = String::from_utf8_lossy(&data[..end]);
                content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        if name.eq_ignore_ascii_case("content-length") {
                            value.trim().parse::<usize>().ok()
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0);
                if content_length > MAX_HTTP_BODY_BYTES {
                    return Err("HTTP 请求正文过大。".to_string());
                }
            }
        }
        if let Some(end) = header_end {
            let body_start = end + 4;
            if data.len() >= body_start + content_length {
                break;
            }
        }
    }
    let Some(end) = header_end else {
        return Err("missing HTTP header".to_string());
    };
    let headers = String::from_utf8_lossy(&data[..end]);
    let mut lines = headers.lines();
    let request_line = lines
        .next()
        .ok_or_else(|| "missing request line".to_string())?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or("").to_string();
    let path = request_parts.next().unwrap_or("/").to_string();
    let body_start = end + 4;
    let body_end = body_start + content_length.min(data.len().saturating_sub(body_start));
    let body = String::from_utf8_lossy(&data[body_start..body_end]).to_string();
    Ok((method, path, body))
}

fn find_subsequence(data: &[u8], needle: &[u8]) -> Option<usize> {
    data.windows(needle.len())
        .position(|window| window == needle)
}

fn route_request(
    state: &AppState,
    method: &str,
    raw_path: &str,
    body: &str,
) -> (u16, &'static str, Vec<u8>, Vec<String>) {
    let path = raw_path.split('?').next().unwrap_or("/");
    match (method, path) {
        ("GET", "/") | ("GET", "/index.html") => html(INDEX_HTML),
        ("GET", "/app.js") => text(APP_JS, "text/javascript; charset=utf-8"),
        ("GET", "/editor-tools.js") => text(EDITOR_TOOLS_JS, "text/javascript; charset=utf-8"),
        ("GET", "/styles.css") => text(STYLES_CSS, "text/css; charset=utf-8"),
        ("GET", "/api/profiles") => json(200, profiles_json(state)),
        ("GET", "/api/demo") => json(
            200,
            format!(
                "{{\"title\":\"Lesson 37 The Tea Rose\",\"text\":\"{}\"}}",
                json_escape(DEMO_TEXT)
            ),
        ),
        ("POST", "/api/generate-pdf") => {
            let (status, payload) = handle_generate_pdf(state, body);
            json(status, payload)
        }
        ("POST", "/api/generate-docx") => {
            let (status, payload) = handle_generate_docx(state, body);
            json(status, payload)
        }
        ("POST", "/api/preview") => {
            let (status, payload) = handle_preview(state, body);
            json(status, payload)
        }
        ("POST", "/api/dictionary") => {
            let (status, payload) = handle_dictionary(state, body);
            json(status, payload)
        }
        ("POST", "/api/ai-enhance") => {
            let (status, payload) = handle_ai_enhance(body);
            json(status, payload)
        }
        ("POST", "/api/network-lexicon") => {
            let (status, payload) = handle_network_lexicon(state, body);
            json(status, payload)
        }
        ("GET", path) if path.starts_with("/download/") => serve_download(path),
        _ => json(404, "{\"error\":\"File not found\"}".to_string()),
    }
}

fn html(body: &str) -> (u16, &'static str, Vec<u8>, Vec<String>) {
    text(body, "text/html; charset=utf-8")
}

fn text(body: &str, mime: &'static str) -> (u16, &'static str, Vec<u8>, Vec<String>) {
    (200, mime, body.as_bytes().to_vec(), Vec::new())
}

fn json(status: u16, body: String) -> (u16, &'static str, Vec<u8>, Vec<String>) {
    (
        status,
        "application/json; charset=utf-8",
        body.into_bytes(),
        Vec::new(),
    )
}

fn serve_download(path: &str) -> (u16, &'static str, Vec<u8>, Vec<String>) {
    let filename = url_decode(path.trim_start_matches("/download/"));
    if filename.is_empty()
        || filename.len() > 200
        || filename.chars().any(char::is_control)
        || filename.contains("..")
        || filename.contains('/')
        || filename.contains('\\')
    {
        return json(404, "{\"error\":\"File not found\"}".to_string());
    }
    let file_path = app_output_dir().join(&filename);
    match fs::read(&file_path) {
        Ok(bytes) => (
            200,
            download_mime(&filename),
            bytes,
            vec![format!(
                "Content-Disposition: attachment; filename=\"{}\"",
                filename.replace('"', "")
            )],
        ),
        Err(_) => json(404, "{\"error\":\"File not found\"}".to_string()),
    }
}

fn download_mime(filename: &str) -> &'static str {
    if filename.to_ascii_lowercase().ends_with(".docx") {
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
    } else {
        "application/pdf"
    }
}

fn app_output_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("output")
}

fn bind_server() -> Result<(TcpListener, u16), String> {
    for port in 8765..8795 {
        match TcpListener::bind(("127.0.0.1", port)) {
            Ok(listener) => return Ok((listener, port)),
            Err(_) => continue,
        }
    }
    Err("无法绑定本地端口 8765-8794。".to_string())
}

fn open_browser(url: &str) {
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("cmd").args(["/C", "start", "", url]).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg(url).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = Command::new("xdg-open").arg(url).spawn();
    }
}

#[cfg(target_os = "windows")]
fn should_use_embedded_ui(args: &[String]) -> bool {
    !args
        .iter()
        .any(|arg| arg == "--browser" || arg == "--native" || arg == "--no-open")
}

fn serve(listener: TcpListener, state: Arc<AppState>) {
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let state = Arc::clone(&state);
                thread::spawn(move || handle_client(stream, state));
            }
            Err(error) => eprintln!("连接失败: {error}"),
        }
    }
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();

    #[cfg(target_os = "windows")]
    windows_gui::enable_high_dpi();

    let state = Arc::new(AppState::load()?);

    if args.get(1).is_some_and(|arg| arg == "--generate-demo") {
        let output = args
            .get(2)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("output/demo.pdf"));
        let missing = generate_pdf(
            &state,
            DEMO_TEXT,
            &output,
            "Lesson 37 The Tea Rose",
            "glittered=ˈɡlɪt.ərd=闪闪发光",
            true,
            "P4",
            TextSizes::default_body(),
        )?;
        println!(
            "Generated {} (missing: {})",
            output.display(),
            missing.len()
        );
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    if args.iter().any(|arg| arg == "--native") {
        windows_gui::run_native(state)?;
        return Ok(());
    }

    let no_open = args.iter().any(|arg| arg == "--no-open");
    let (listener, port) = bind_server()?;
    let url = format!("http://127.0.0.1:{port}/");

    // A normal Windows launch hosts the complete web interface in a WebView2
    // child window. The local HTTP server stays private to this process.
    #[cfg(target_os = "windows")]
    if should_use_embedded_ui(&args) {
        let server_state = Arc::clone(&state);
        thread::spawn(move || serve(listener, server_state));
        if let Err(error) = windows_gui::run_webview(&url) {
            eprintln!("{error}");
            windows_gui::run_native(state)?;
        }
        return Ok(());
    }

    println!("词境精读 Rust 版运行中: {url}");
    if !no_open {
        open_browser(&url);
    }
    serve(listener, state);
    Ok(())
}

#[cfg(target_os = "windows")]
mod windows_gui {
    use super::*;
    use std::ffi::c_void;
    use std::num::NonZeroIsize;
    use std::ptr::{null, null_mut};
    use wry::dpi::{PhysicalPosition, PhysicalSize};
    use wry::raw_window_handle::{
        HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
    };
    use wry::{Rect as WebViewRect, WebView, WebViewBuilder, WebViewExtWindows};

    type Bool = i32;
    type Dword = u32;
    type Hbrush = *mut c_void;
    type Hcursor = *mut c_void;
    type Hicon = *mut c_void;
    type Hinstance = *mut c_void;
    type Hmenu = *mut c_void;
    type Hwnd = *mut c_void;
    type Lparam = isize;
    type Lresult = isize;
    type Uint = u32;
    type Wparam = usize;

    const CW_USEDEFAULT: i32 = 0x80000000u32 as i32;
    const GWLP_USERDATA: i32 = -21;
    const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;
    const SPI_GETWORKAREA: Uint = 0x0030;
    const SW_HIDE: i32 = 0;
    const SW_SHOW: i32 = 5;

    const WS_OVERLAPPEDWINDOW: Dword = 0x00cf0000;
    const WS_CHILD: Dword = 0x40000000;
    const WS_VISIBLE: Dword = 0x10000000;
    const WS_TABSTOP: Dword = 0x00010000;
    const WS_BORDER: Dword = 0x00800000;
    const WS_VSCROLL: Dword = 0x00200000;

    const ES_MULTILINE: Dword = 0x0004;
    const ES_AUTOVSCROLL: Dword = 0x0040;
    const ES_AUTOHSCROLL: Dword = 0x0080;
    const ES_WANTRETURN: Dword = 0x1000;
    const CBS_DROPDOWNLIST: Dword = 0x0003;
    const BS_AUTOCHECKBOX: Dword = 0x0003;
    const BS_PUSHBUTTON: Dword = 0x0000;
    const BS_DEFPUSHBUTTON: Dword = 0x0001;

    const WM_NCCREATE: Uint = 0x0081;
    const WM_CREATE: Uint = 0x0001;
    const WM_SIZE: Uint = 0x0005;
    const WM_COMMAND: Uint = 0x0111;
    const WM_DESTROY: Uint = 0x0002;
    const WM_DPICHANGED: Uint = 0x02e0;
    const WM_SETFONT: Uint = 0x0030;
    const WM_WINDOWPOSCHANGED: Uint = 0x0047;
    const WM_APP_HOST_RESIZE: Uint = 0x8001;

    const EN_CHANGE: usize = 0x0300;
    const CB_ADDSTRING: Uint = 0x0143;
    const CB_SETCURSEL: Uint = 0x014e;
    const CB_GETCURSEL: Uint = 0x0147;
    const BM_GETCHECK: Uint = 0x00f0;
    const BM_SETCHECK: Uint = 0x00f1;
    const BST_CHECKED: usize = 1;

    const MB_OK: Uint = 0x0000;
    const MB_ICONINFORMATION: Uint = 0x0040;
    const MB_ICONWARNING: Uint = 0x0030;
    const MB_ICONERROR: Uint = 0x0010;
    const DEFAULT_GUI_FONT: i32 = 17;
    const SWP_NOZORDER: Uint = 0x0004;
    const SWP_NOACTIVATE: Uint = 0x0010;

    const ID_DEMO: usize = 1001;
    const ID_CLEAR: usize = 1002;
    const ID_GENERATE: usize = 1003;
    const ID_OPEN_FILE: usize = 1004;
    const ID_OPEN_FOLDER: usize = 1005;
    const ID_ARTICLE: usize = 2001;
    const ID_GRADE: usize = 2002;
    #[repr(C)]
    struct WndClassW {
        style: Uint,
        lpfn_wnd_proc: Option<unsafe extern "system" fn(Hwnd, Uint, Wparam, Lparam) -> Lresult>,
        cb_cls_extra: i32,
        cb_wnd_extra: i32,
        h_instance: Hinstance,
        h_icon: Hicon,
        h_cursor: Hcursor,
        hbr_background: Hbrush,
        lpsz_menu_name: *const u16,
        lpsz_class_name: *const u16,
    }

    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    struct Msg {
        hwnd: Hwnd,
        message: Uint,
        w_param: Wparam,
        l_param: Lparam,
        time: Dword,
        pt: Point,
    }

    #[repr(C)]
    struct CreateStructW {
        lp_create_params: *mut c_void,
        h_instance: Hinstance,
        h_menu: Hmenu,
        hwnd_parent: Hwnd,
        cy: i32,
        cx: i32,
        y: i32,
        x: i32,
        style: i32,
        lpsz_name: *const u16,
        lpsz_class: *const u16,
        ex_style: Dword,
    }

    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn RegisterClassW(lp_wnd_class: *const WndClassW) -> u16;
        fn CreateWindowExW(
            dw_ex_style: Dword,
            lp_class_name: *const u16,
            lp_window_name: *const u16,
            dw_style: Dword,
            x: i32,
            y: i32,
            n_width: i32,
            n_height: i32,
            hwnd_parent: Hwnd,
            h_menu: Hmenu,
            h_instance: Hinstance,
            lp_param: *mut c_void,
        ) -> Hwnd;
        fn DefWindowProcW(hwnd: Hwnd, msg: Uint, w_param: Wparam, l_param: Lparam) -> Lresult;
        fn DispatchMessageW(lp_msg: *const Msg) -> Lresult;
        fn EnableWindow(hwnd: Hwnd, enable: Bool) -> Bool;
        fn GetClientRect(hwnd: Hwnd, rect: *mut Rect) -> Bool;
        fn GetDpiForWindow(hwnd: Hwnd) -> Uint;
        fn GetDlgItem(hwnd: Hwnd, id: i32) -> Hwnd;
        fn GetMessageW(
            lp_msg: *mut Msg,
            hwnd: Hwnd,
            msg_filter_min: Uint,
            msg_filter_max: Uint,
        ) -> Bool;
        fn GetWindowLongPtrW(hwnd: Hwnd, n_index: i32) -> isize;
        fn GetWindowTextLengthW(hwnd: Hwnd) -> i32;
        fn GetWindowTextW(hwnd: Hwnd, lp_string: *mut u16, n_max_count: i32) -> i32;
        fn LoadCursorW(h_instance: Hinstance, lp_cursor_name: *const u16) -> Hcursor;
        fn MessageBoxW(hwnd: Hwnd, text: *const u16, caption: *const u16, typ: Uint) -> i32;
        fn MoveWindow(hwnd: Hwnd, x: i32, y: i32, width: i32, height: i32, repaint: Bool) -> Bool;
        fn PostMessageW(hwnd: Hwnd, msg: Uint, w_param: Wparam, l_param: Lparam) -> Bool;
        fn PostQuitMessage(exit_code: i32);
        fn SendMessageW(hwnd: Hwnd, msg: Uint, w_param: Wparam, l_param: Lparam) -> Lresult;
        fn SetWindowLongPtrW(hwnd: Hwnd, n_index: i32, dw_new_long: isize) -> isize;
        fn SetWindowPos(
            hwnd: Hwnd,
            insert_after: Hwnd,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            flags: Uint,
        ) -> Bool;
        fn SetWindowTextW(hwnd: Hwnd, text: *const u16) -> Bool;
        fn SetProcessDpiAwarenessContext(value: isize) -> Bool;
        fn SetThreadDpiAwarenessContext(value: isize) -> isize;
        fn ShowWindow(hwnd: Hwnd, cmd_show: i32) -> Bool;
        fn SystemParametersInfoW(
            action: Uint,
            param: Uint,
            value: *mut c_void,
            update: Uint,
        ) -> Bool;
        fn TranslateMessage(lp_msg: *const Msg) -> Bool;
        fn UpdateWindow(hwnd: Hwnd) -> Bool;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetModuleHandleW(lp_module_name: *const u16) -> Hinstance;
    }

    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn GetStockObject(index: i32) -> *mut c_void;
    }

    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ShellExecuteW(
            hwnd: Hwnd,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show_cmd: i32,
        ) -> *mut c_void;
    }

    struct GuiData {
        state: Arc<AppState>,
        title_edit: Hwnd,
        article_edit: Hwnd,
        word_count: Hwnd,
        grade_combo: Hwnd,
        grade_help: Hwnd,
        custom_edit: Hwnd,
        unknown_check: Hwnd,
        status: Hwnd,
        open_file_btn: Hwnd,
        open_folder_btn: Hwnd,
        generated_path: Option<PathBuf>,
    }

    impl GuiData {
        fn new(state: Arc<AppState>) -> Self {
            Self {
                state,
                title_edit: null_mut(),
                article_edit: null_mut(),
                word_count: null_mut(),
                grade_combo: null_mut(),
                grade_help: null_mut(),
                custom_edit: null_mut(),
                unknown_check: null_mut(),
                status: null_mut(),
                open_file_btn: null_mut(),
                open_folder_btn: null_mut(),
                generated_path: None,
            }
        }

        fn text_sizes(&self) -> TextSizes {
            TextSizes::default_body()
        }
    }

    struct WebViewHost(Hwnd);

    impl HasWindowHandle for WebViewHost {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            let hwnd = NonZeroIsize::new(self.0 as isize).ok_or(HandleError::Unavailable)?;
            let raw = RawWindowHandle::Win32(Win32WindowHandle::new(hwnd));
            Ok(unsafe { WindowHandle::borrow_raw(raw) })
        }
    }

    pub fn enable_high_dpi() {
        unsafe {
            SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
    }

    fn logical_client_size(hwnd: Hwnd, physical_width: i32, physical_height: i32) -> (i32, i32) {
        let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96) as i64;
        let to_logical = |value: i32| ((value.max(1) as i64 * 96 + dpi / 2) / dpi) as i32;
        (to_logical(physical_width), to_logical(physical_height))
    }

    unsafe fn initial_window_bounds() -> (i32, i32, i32, i32) {
        let mut work_area = Rect {
            left: 0,
            top: 0,
            right: 1280,
            bottom: 800,
        };
        let loaded = unsafe {
            SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut work_area as *mut Rect).cast(), 0)
        };
        if loaded == 0 {
            return (50, 50, 1180, 720);
        }

        let available_width = (work_area.right - work_area.left).max(640);
        let available_height = (work_area.bottom - work_area.top).max(480);
        let horizontal_margin = (available_width / 32).clamp(16, 48);
        let vertical_margin = (available_height / 32).clamp(16, 36);
        (
            work_area.left + horizontal_margin,
            work_area.top + vertical_margin,
            (available_width - horizontal_margin * 2).max(640),
            (available_height - vertical_margin * 2).max(480),
        )
    }

    pub fn run_webview(url: &str) -> Result<(), String> {
        unsafe {
            let instance = GetModuleHandleW(null());
            if instance.is_null() {
                return Err("无法初始化 Windows 窗口。".to_string());
            }

            let class_name = wide("CijingReaderWebViewWindow");
            let class = WndClassW {
                style: 0,
                lpfn_wnd_proc: Some(webview_window_proc),
                cb_cls_extra: 0,
                cb_wnd_extra: 0,
                h_instance: instance,
                h_icon: null_mut(),
                h_cursor: LoadCursorW(null_mut(), 32512usize as *const u16),
                hbr_background: (16 + 1) as Hbrush,
                lpsz_menu_name: null(),
                lpsz_class_name: class_name.as_ptr(),
            };
            if RegisterClassW(&class) == 0 {
                return Err("注册内嵌界面窗口失败。".to_string());
            }

            let window_title = wide("词境精读");
            let (window_x, window_y, window_width, window_height) = initial_window_bounds();
            let hwnd = CreateWindowExW(
                0,
                class_name.as_ptr(),
                window_title.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                window_x,
                window_y,
                window_width,
                window_height,
                null_mut(),
                null_mut(),
                instance,
                null_mut(),
            );
            if hwnd.is_null() {
                return Err("创建内嵌界面窗口失败。".to_string());
            }
            ShowWindow(hwnd, SW_SHOW);
            UpdateWindow(hwnd);

            let host = WebViewHost(hwnd);
            let allowed_origin = url.to_string();
            let mut client_rect = Rect {
                left: 0,
                top: 0,
                right: window_width,
                bottom: window_height,
            };
            GetClientRect(hwnd, &mut client_rect);
            let initial_client_width = (client_rect.right - client_rect.left).max(1);
            let initial_client_height = (client_rect.bottom - client_rect.top).max(1);
            let (initial_layout_width, initial_layout_height) =
                logical_client_size(hwnd, initial_client_width, initial_client_height);
            let initial_bounds = WebViewRect {
                position: PhysicalPosition::new(0, 0).into(),
                size: PhysicalSize::new(initial_client_width, initial_client_height).into(),
            };
            let responsive_script = format!(
                r#"
                window.__CIJING_SET_HOST_SIZE__ = (layoutWidth, layoutHeight) => {{
                  const width = Math.max(240, layoutWidth);
                  const height = Math.max(320, layoutHeight);
                  const apply = () => {{
                    const root = document.documentElement;
                    if (!root) {{
                      requestAnimationFrame(apply);
                      return;
                    }}
                    root.style.setProperty("--host-width", `${{width}}px`);
                    root.style.setProperty("--host-height", `${{height}}px`);
                    root.classList.toggle("host-medium", width <= 1100);
                    root.classList.toggle("host-small", width <= 720);
                    root.classList.toggle("host-mobile", width <= 480);
                    root.classList.toggle("host-compact-height", height <= 720 && width > 1100);
                    window.dispatchEvent(new Event("resize"));
                  }};
                  apply();
                }};
                document.addEventListener("DOMContentLoaded", () => {{
                  window.__CIJING_SET_HOST_SIZE__({initial_layout_width}, {initial_layout_height});
                }}, {{ once: true }});
                "#
            );
            let webview = match WebViewBuilder::new()
                .with_url(url)
                .with_bounds(initial_bounds)
                .with_initialization_script("window.__CIJING_DESKTOP__ = true;")
                .with_initialization_script(responsive_script)
                .with_clipboard(true)
                .with_navigation_handler(move |candidate| {
                    candidate == "about:blank" || candidate.starts_with(&allowed_origin)
                })
                .with_ipc_handler(|request| {
                    if request.body() == "open-output-folder" {
                        let output_dir = app_output_dir();
                        let _ = fs::create_dir_all(&output_dir);
                        let _ = Command::new("explorer").arg(output_dir).spawn();
                    }
                })
                .build_as_child(&host)
            {
                Ok(webview) => webview,
                Err(error) => {
                    let detail = format!(
                        "无法创建内嵌网页界面（需要 WebView2），将改用兼容窗口。\r\n\r\n{error}"
                    );
                    MessageBoxW(
                        hwnd,
                        wide(&detail).as_ptr(),
                        window_title.as_ptr(),
                        MB_OK | MB_ICONWARNING,
                    );
                    ShowWindow(hwnd, SW_HIDE);
                    return Err(detail);
                }
            };
            let webview_ptr = Box::into_raw(Box::new(webview));
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, webview_ptr as isize);

            let mut msg = Msg {
                hwnd: null_mut(),
                message: 0,
                w_param: 0,
                l_param: 0,
                time: 0,
                pt: Point { x: 0, y: 0 },
            };
            while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        Ok(())
    }

    fn resize_webview_bounds(webview: &WebView, width: i32, height: i32) {
        let bounds = WebViewRect {
            position: PhysicalPosition::new(0, 0).into(),
            size: PhysicalSize::new(width, height).into(),
        };
        if let Err(error) = webview.set_bounds(bounds) {
            eprintln!("调整内嵌界面尺寸失败：{error}");
        }
        if let Err(error) = unsafe { webview.controller().NotifyParentWindowPositionChanged() } {
            eprintln!("通知 WebView 窗口位置变化失败：{error}");
        }
    }

    fn sync_page_host_size(webview: &WebView, width: i32, height: i32) {
        let script = format!(
            r#"
            (() => {{
              const width = Math.max(240, {width});
              const height = Math.max(320, {height});
              const root = document.documentElement;
              if (!root) return;
              root.style.setProperty("--host-width", `${{width}}px`);
              root.style.setProperty("--host-height", `${{height}}px`);
              root.classList.toggle("host-medium", width <= 1100);
              root.classList.toggle("host-small", width <= 720);
              root.classList.toggle("host-mobile", width <= 480);
              root.classList.toggle("host-compact-height", height <= 720 && width > 1100);
              window.dispatchEvent(new Event("resize"));
            }})();
            "#
        );
        if let Err(error) = webview.evaluate_script(&script) {
            eprintln!("同步页面响应式尺寸失败：{error}");
        }
    }

    unsafe extern "system" fn webview_window_proc(
        hwnd: Hwnd,
        msg: Uint,
        w_param: Wparam,
        l_param: Lparam,
    ) -> Lresult {
        match msg {
            WM_SIZE => {
                let webview_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WebView };
                let width = (l_param as u32 & 0xffff) as i32;
                let height = ((l_param as u32 >> 16) & 0xffff) as i32;
                if !webview_ptr.is_null() && width > 0 && height > 0 {
                    let webview = unsafe { &*webview_ptr };
                    resize_webview_bounds(webview, width, height);
                    let (layout_width, layout_height) = logical_client_size(hwnd, width, height);
                    unsafe {
                        PostMessageW(
                            hwnd,
                            WM_APP_HOST_RESIZE,
                            layout_width as Wparam,
                            layout_height as Lparam,
                        )
                    };
                }
                0
            }
            WM_APP_HOST_RESIZE => {
                let webview_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WebView };
                if !webview_ptr.is_null() {
                    let webview = unsafe { &*webview_ptr };
                    sync_page_host_size(webview, w_param as i32, l_param as i32);
                }
                0
            }
            WM_WINDOWPOSCHANGED => {
                let webview_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WebView };
                if !webview_ptr.is_null() {
                    let webview = unsafe { &*webview_ptr };
                    if let Err(error) =
                        unsafe { webview.controller().NotifyParentWindowPositionChanged() }
                    {
                        eprintln!("通知 WebView 窗口位置变化失败：{error}");
                    }
                }
                unsafe { DefWindowProcW(hwnd, msg, w_param, l_param) }
            }
            WM_DPICHANGED => {
                let suggested = l_param as *const Rect;
                if !suggested.is_null() {
                    let suggested = unsafe { &*suggested };
                    unsafe {
                        SetWindowPos(
                            hwnd,
                            null_mut(),
                            suggested.left,
                            suggested.top,
                            suggested.right - suggested.left,
                            suggested.bottom - suggested.top,
                            SWP_NOZORDER | SWP_NOACTIVATE,
                        )
                    };
                }
                0
            }
            WM_DESTROY => {
                let webview_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WebView };
                if !webview_ptr.is_null() {
                    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
                    unsafe { drop(Box::from_raw(webview_ptr)) };
                }
                unsafe { PostQuitMessage(0) };
                0
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, w_param, l_param) },
        }
    }

    pub fn run_native(state: Arc<AppState>) -> Result<(), String> {
        unsafe {
            let instance = GetModuleHandleW(null());
            if instance.is_null() {
                return Err("无法初始化 Windows 窗口。".to_string());
            }

            let class_name = wide("CijingReaderRustWindow");
            let class = WndClassW {
                style: 0,
                lpfn_wnd_proc: Some(window_proc),
                cb_cls_extra: 0,
                cb_wnd_extra: 0,
                h_instance: instance,
                h_icon: null_mut(),
                h_cursor: LoadCursorW(null_mut(), 32512usize as *const u16),
                hbr_background: (16 + 1) as Hbrush,
                lpsz_menu_name: null(),
                lpsz_class_name: class_name.as_ptr(),
            };
            if RegisterClassW(&class) == 0 {
                return Err("注册窗口类失败。".to_string());
            }

            let data_ptr = Box::into_raw(Box::new(GuiData::new(state)));
            let window_title = wide("词境精读");
            let hwnd = CreateWindowExW(
                0,
                class_name.as_ptr(),
                window_title.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                1180,
                760,
                null_mut(),
                null_mut(),
                instance,
                data_ptr.cast(),
            );
            if hwnd.is_null() {
                drop(Box::from_raw(data_ptr));
                return Err("创建窗口失败。".to_string());
            }
            SetWindowTextW(hwnd, window_title.as_ptr());
            ShowWindow(hwnd, SW_SHOW);
            UpdateWindow(hwnd);

            let mut msg = Msg {
                hwnd: null_mut(),
                message: 0,
                w_param: 0,
                l_param: 0,
                time: 0,
                pt: Point { x: 0, y: 0 },
            };
            while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        Ok(())
    }

    unsafe extern "system" fn window_proc(
        hwnd: Hwnd,
        msg: Uint,
        w_param: Wparam,
        l_param: Lparam,
    ) -> Lresult {
        match msg {
            WM_NCCREATE => {
                let createstruct = l_param as *const CreateStructW;
                if !createstruct.is_null() {
                    let data_ptr = unsafe { (*createstruct).lp_create_params };
                    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, data_ptr as isize) };
                    1
                } else {
                    0
                }
            }
            WM_CREATE => {
                if let Some(data) = unsafe { data_mut(hwnd) } {
                    unsafe { create_controls(hwnd, data) };
                    unsafe { layout_controls(hwnd, data, 1180, 760) };
                    unsafe { update_grade_help(data) };
                    unsafe { update_word_count(data) };
                }
                0
            }
            WM_SIZE => {
                if let Some(data) = unsafe { data_mut(hwnd) } {
                    let width = (l_param as u32 & 0xffff) as i32;
                    let height = ((l_param as u32 >> 16) & 0xffff) as i32;
                    unsafe { layout_controls(hwnd, data, width, height) };
                }
                0
            }
            WM_COMMAND => {
                if let Some(data) = unsafe { data_mut(hwnd) } {
                    let id = w_param & 0xffff;
                    let code = (w_param >> 16) & 0xffff;
                    match id {
                        ID_DEMO => unsafe { insert_demo(data) },
                        ID_CLEAR => unsafe { clear_all(data) },
                        ID_GENERATE => unsafe { generate(hwnd, data) },
                        ID_OPEN_FILE => unsafe { open_generated(hwnd, data, false) },
                        ID_OPEN_FOLDER => unsafe { open_generated(hwnd, data, true) },
                        ID_ARTICLE if code == EN_CHANGE => unsafe { update_word_count(data) },
                        ID_GRADE => unsafe { update_grade_help(data) },
                        _ => {}
                    }
                }
                0
            }
            WM_DESTROY => {
                let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut GuiData };
                if !ptr.is_null() {
                    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
                    unsafe { drop(Box::from_raw(ptr)) };
                }
                unsafe { PostQuitMessage(0) };
                0
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, w_param, l_param) },
        }
    }

    unsafe fn data_mut(hwnd: Hwnd) -> Option<&'static mut GuiData> {
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut GuiData };
        if ptr.is_null() {
            None
        } else {
            Some(unsafe { &mut *ptr })
        }
    }

    unsafe fn create_controls(hwnd: Hwnd, data: &mut GuiData) {
        let font = unsafe { GetStockObject(DEFAULT_GUI_FONT) };

        unsafe {
            create_child(
                hwnd,
                "STATIC",
                "英文文章",
                WS_CHILD | WS_VISIBLE,
                3001,
                font,
            )
        };
        data.word_count =
            unsafe { create_child(hwnd, "STATIC", "0 words", WS_CHILD | WS_VISIBLE, 3002, font) };
        data.article_edit = unsafe {
            create_child(
                hwnd,
                "EDIT",
                "",
                WS_CHILD
                    | WS_VISIBLE
                    | WS_TABSTOP
                    | WS_BORDER
                    | ES_MULTILINE
                    | ES_AUTOVSCROLL
                    | ES_WANTRETURN
                    | WS_VSCROLL,
                ID_ARTICLE,
                font,
            )
        };
        unsafe { create_child(hwnd, "STATIC", "标题", WS_CHILD | WS_VISIBLE, 3003, font) };
        data.title_edit = unsafe {
            create_child(
                hwnd,
                "EDIT",
                "",
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL,
                3004,
                font,
            )
        };

        unsafe {
            create_child(
                hwnd,
                "STATIC",
                "学生年级",
                WS_CHILD | WS_VISIBLE,
                3005,
                font,
            )
        };
        data.grade_combo = unsafe {
            create_child(
                hwnd,
                "COMBOBOX",
                "",
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST,
                ID_GRADE,
                font,
            )
        };
        for profile in &data.state.profiles {
            let text = wide(&profile.label);
            unsafe { SendMessageW(data.grade_combo, CB_ADDSTRING, 0, text.as_ptr() as Lparam) };
        }
        let default_index = data
            .state
            .profiles
            .iter()
            .position(|profile| profile.code == "P4")
            .unwrap_or(0);
        unsafe { SendMessageW(data.grade_combo, CB_SETCURSEL, default_index, 0) };
        data.grade_help =
            unsafe { create_child(hwnd, "STATIC", "", WS_CHILD | WS_VISIBLE, 3006, font) };

        unsafe {
            create_child(
                hwnd,
                "STATIC",
                "自定义标注词",
                WS_CHILD | WS_VISIBLE,
                3007,
                font,
            )
        };
        data.custom_edit = unsafe {
            create_child(
                hwnd,
                "EDIT",
                "",
                WS_CHILD
                    | WS_VISIBLE
                    | WS_TABSTOP
                    | WS_BORDER
                    | ES_MULTILINE
                    | ES_AUTOVSCROLL
                    | ES_WANTRETURN
                    | WS_VSCROLL,
                3008,
                font,
            )
        };
        data.unknown_check = unsafe {
            create_child(
                hwnd,
                "BUTTON",
                "记录词典未收录词（正文不显示占位符）",
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX,
                3009,
                font,
            )
        };
        unsafe { SendMessageW(data.unknown_check, BM_SETCHECK, BST_CHECKED, 0) };

        unsafe { create_child(hwnd, "BUTTON", "演示", button_style(false), ID_DEMO, font) };
        unsafe { create_child(hwnd, "BUTTON", "清空", button_style(false), ID_CLEAR, font) };
        unsafe {
            create_child(
                hwnd,
                "BUTTON",
                "输出 PDF",
                button_style(true),
                ID_GENERATE,
                font,
            )
        };
        data.open_file_btn = unsafe {
            create_child(
                hwnd,
                "BUTTON",
                "打开 PDF",
                button_style(false),
                ID_OPEN_FILE,
                font,
            )
        };
        data.open_folder_btn = unsafe {
            create_child(
                hwnd,
                "BUTTON",
                "打开文件夹",
                button_style(false),
                ID_OPEN_FOLDER,
                font,
            )
        };
        unsafe { EnableWindow(data.open_file_btn, 0) };
        unsafe { EnableWindow(data.open_folder_btn, 0) };
        data.status =
            unsafe { create_child(hwnd, "STATIC", "就绪", WS_CHILD | WS_VISIBLE, 3010, font) };
    }

    fn button_style(default_button: bool) -> Dword {
        WS_CHILD
            | WS_VISIBLE
            | WS_TABSTOP
            | if default_button {
                BS_DEFPUSHBUTTON
            } else {
                BS_PUSHBUTTON
            }
    }

    unsafe fn create_child(
        parent: Hwnd,
        class: &str,
        text: &str,
        style: Dword,
        id: usize,
        font: *mut c_void,
    ) -> Hwnd {
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                wide(class).as_ptr(),
                wide(text).as_ptr(),
                style,
                0,
                0,
                10,
                10,
                parent,
                id as Hmenu,
                null_mut(),
                null_mut(),
            )
        };
        unsafe { SendMessageW(hwnd, WM_SETFONT, font as Wparam, 1) };
        hwnd
    }

    unsafe fn layout_controls(hwnd: Hwnd, data: &mut GuiData, width: i32, height: i32) {
        let margin = 18;
        let gap = 12;
        let right_w = 350;
        let top = 18;
        let button_h = 34;
        let label_h = 22;
        let input_h = 28;
        let left_w = (width - right_w - margin * 3).max(420);
        let left_x = margin;
        let right_x = margin * 2 + left_w;
        let left_available_h = (height - top - margin).max(480);
        let article_h = (left_available_h - label_h - 8).max(260);

        unsafe { move_by_id(hwnd, 3001, left_x, top, 160, label_h) };
        unsafe { MoveWindow(data.word_count, left_x + left_w - 90, top, 90, label_h, 1) };
        unsafe {
            MoveWindow(
                data.article_edit,
                left_x,
                top + label_h + 8,
                left_w,
                article_h,
                1,
            )
        };

        let mut y = top;
        unsafe { move_by_id(hwnd, 3003, right_x, y, right_w, label_h) };
        y += label_h + 4;
        unsafe { MoveWindow(data.title_edit, right_x, y, right_w, input_h, 1) };
        y += input_h + gap;

        unsafe { move_by_id(hwnd, 3005, right_x, y, right_w, label_h) };
        y += label_h + 4;
        unsafe { MoveWindow(data.grade_combo, right_x, y, right_w, 220, 1) };
        y += input_h + 6;
        unsafe { MoveWindow(data.grade_help, right_x, y, right_w, 44, 1) };
        y += 52;

        unsafe { move_by_id(hwnd, 3007, right_x, y, right_w, label_h) };
        y += label_h + 4;
        unsafe { MoveWindow(data.custom_edit, right_x, y, right_w, 96, 1) };
        y += 96 + gap;
        unsafe { MoveWindow(data.unknown_check, right_x, y, right_w, 38, 1) };
        y += 42;

        let half = (right_w - 8) / 2;
        unsafe { move_by_id(hwnd, ID_DEMO, right_x, y, half, button_h) };
        unsafe { move_by_id(hwnd, ID_CLEAR, right_x + half + 8, y, half, button_h) };
        y += button_h + 8;
        unsafe { move_by_id(hwnd, ID_GENERATE, right_x, y, right_w, button_h + 4) };
        y += button_h + 14;
        unsafe { MoveWindow(data.open_file_btn, right_x, y, half, button_h, 1) };
        unsafe {
            MoveWindow(
                data.open_folder_btn,
                right_x + half + 8,
                y,
                half,
                button_h,
                1,
            )
        };
        y += button_h + 10;
        unsafe { MoveWindow(data.status, right_x, y, right_w, 120, 1) };
    }

    unsafe fn move_by_id(hwnd: Hwnd, id: usize, x: i32, y: i32, w: i32, h: i32) {
        let child = unsafe { GetDlgItem(hwnd, id as i32) };
        if !child.is_null() {
            unsafe { MoveWindow(child, x, y, w, h, 1) };
        }
    }

    unsafe fn insert_demo(data: &mut GuiData) {
        unsafe { set_text(data.title_edit, "Lesson 37 The Tea Rose") };
        unsafe { set_text(data.article_edit, DEMO_TEXT) };
        unsafe { set_text(data.custom_edit, "glittered=ˈɡlɪt.ərd=闪闪发光") };
        let default_index = data
            .state
            .profiles
            .iter()
            .position(|profile| profile.code == "P4")
            .unwrap_or(0);
        unsafe { SendMessageW(data.grade_combo, CB_SETCURSEL, default_index, 0) };
        unsafe { update_grade_help(data) };
        unsafe { update_word_count(data) };
        unsafe { set_text(data.status, "已插入演示文本") };
    }

    unsafe fn clear_all(data: &mut GuiData) {
        unsafe { set_text(data.title_edit, "") };
        unsafe { set_text(data.article_edit, "") };
        unsafe { set_text(data.custom_edit, "") };
        unsafe { set_text(data.status, "已清空") };
        data.generated_path = None;
        unsafe { EnableWindow(data.open_file_btn, 0) };
        unsafe { EnableWindow(data.open_folder_btn, 0) };
        unsafe { update_word_count(data) };
    }

    unsafe fn generate(hwnd: Hwnd, data: &mut GuiData) {
        let article = unsafe { get_text(data.article_edit) };
        if article.trim().is_empty() {
            unsafe { message(hwnd, "请先粘贴英文文章。", MB_ICONWARNING) };
            return;
        }
        unsafe { set_text(data.status, "生成中...") };
        let title = unsafe { get_text(data.title_edit) };
        let custom = unsafe { get_text(data.custom_edit) };
        let grade_index = unsafe { SendMessageW(data.grade_combo, CB_GETCURSEL, 0, 0) };
        let grade_index = if grade_index >= 0 {
            grade_index as usize
        } else {
            0
        };
        let grade_code = data
            .state
            .profiles
            .get(grade_index)
            .map(|profile| profile.code.clone())
            .unwrap_or_else(|| "P4".to_string());
        let annotate_unknown =
            unsafe { SendMessageW(data.unknown_check, BM_GETCHECK, 0, 0) as usize == BST_CHECKED };
        let output_dir = app_output_dir();
        let filename = format!(
            "{}_{}.pdf",
            safe_title(if title.trim().is_empty() {
                "annotated_article"
            } else {
                &title
            }),
            unique_suffix()
        );
        let output_path = output_dir.join(filename);
        match generate_pdf(
            &data.state,
            &article,
            &output_path,
            &title,
            &custom,
            annotate_unknown,
            &grade_code,
            data.text_sizes(),
        ) {
            Ok(missing) => {
                data.generated_path = Some(output_path.clone());
                unsafe { EnableWindow(data.open_file_btn, 1) };
                unsafe { EnableWindow(data.open_folder_btn, 1) };
                let text = if missing.is_empty() {
                    format!("PDF 已生成：\r\n{}", output_path.display())
                } else {
                    format!(
                        "PDF 已生成：\r\n{}\r\n未收录词 {} 个。",
                        output_path.display(),
                        missing.len()
                    )
                };
                unsafe { set_text(data.status, &text) };
                unsafe { message(hwnd, "PDF 已生成。", MB_ICONINFORMATION) };
            }
            Err(error) => {
                unsafe { set_text(data.status, "生成失败") };
                unsafe { message(hwnd, &format!("生成失败：{error}"), MB_ICONERROR) };
            }
        }
    }

    unsafe fn open_generated(hwnd: Hwnd, data: &mut GuiData, folder: bool) {
        let Some(path) = &data.generated_path else {
            unsafe { message(hwnd, "还没有生成 PDF。", MB_ICONWARNING) };
            return;
        };
        let target = if folder {
            path.parent().unwrap_or(path).to_path_buf()
        } else {
            path.clone()
        };
        let op = wide("open");
        let file = wide(&target.to_string_lossy());
        unsafe { ShellExecuteW(hwnd, op.as_ptr(), file.as_ptr(), null(), null(), SW_SHOW) };
    }

    unsafe fn update_word_count(data: &mut GuiData) {
        let text = unsafe { get_text(data.article_edit) };
        let count = text
            .split(|ch: char| !ch.is_ascii_alphabetic() && ch != '-' && ch != '\'')
            .filter(|part| part.chars().any(|ch| ch.is_ascii_alphabetic()))
            .count();
        unsafe { set_text(data.word_count, &format!("{count} words")) };
    }

    unsafe fn update_grade_help(data: &mut GuiData) {
        let selected = unsafe { SendMessageW(data.grade_combo, CB_GETCURSEL, 0, 0) };
        let selected = if selected >= 0 { selected as usize } else { 0 };
        if let Some(profile) = data.state.profiles.get(selected) {
            unsafe {
                set_text(
                    data.grade_help,
                    &format!(
                        "预估词汇量约 {} 词。\r\n{}",
                        profile.estimated_vocab, profile.note
                    ),
                )
            };
        }
    }

    unsafe fn get_text(hwnd: Hwnd) -> String {
        let len = unsafe { GetWindowTextLengthW(hwnd) }.max(0) as usize;
        let mut buf = vec![0u16; len + 1];
        let read =
            unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) }.max(0) as usize;
        String::from_utf16_lossy(&buf[..read])
    }

    unsafe fn set_text(hwnd: Hwnd, text: &str) {
        let text = wide(text);
        unsafe { SetWindowTextW(hwnd, text.as_ptr()) };
    }

    unsafe fn message(hwnd: Hwnd, text: &str, icon: Uint) {
        unsafe {
            MessageBoxW(
                hwnd,
                wide(text).as_ptr(),
                wide("词境精读").as_ptr(),
                MB_OK | icon,
            )
        };
    }

    fn app_output_dir() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
            .join("output")
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    fn state() -> &'static AppState {
        static STATE: OnceLock<AppState> = OnceLock::new();
        STATE.get_or_init(|| AppState::load().expect("embedded dictionaries should load"))
    }

    #[test]
    fn normalizes_inflections_and_compound_words() {
        assert!(candidate_lemmas("studies").contains(&"study".to_string()));
        assert!(candidate_lemmas("running").contains(&"run".to_string()));
        assert_eq!(normalize_key("  Well–Known! "), "well-known");
    }

    #[test]
    fn parses_custom_annotations_and_forced_terms() {
        let (entries, forced, ignored) =
            parse_custom_annotations("glittered=ˈɡlɪt.ərd=闪闪发光\n*rose\ngarden\n!beautiful");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].term, "glittered");
        assert!(entries[0].hard);
        assert_eq!(forced, vec!["rose", "garden"]);
        assert_eq!(ignored, vec!["beautiful"]);
    }

    #[test]
    fn looks_up_local_dictionary_and_serves_editor_tools() {
        let (status, payload) = handle_dictionary(state(), r#"{"word":"roses"}"#);
        assert_eq!(status, 200);
        assert!(payload.contains("\"found\":true"));
        assert!(payload.contains("\"term\":\"rose\""));

        let (status, mime, body, _) = route_request(state(), "GET", "/editor-tools.js", "");
        assert_eq!(status, 200);
        assert_eq!(mime, "text/javascript; charset=utf-8");
        assert!(body.starts_with(b"(() =>"));
    }

    #[test]
    fn clamps_layout_settings_and_converts_page_sizes() {
        let sizes = text_sizes_from_json(
            r#"{"englishHps":100,"ipaHps":2,"lineHeight":9,"pageSize":"custom","customPageWidth":210,"customPageHeight":297}"#,
        );
        assert_eq!(sizes.english_hps, 48);
        assert_eq!(sizes.ipa_hps, 8);
        assert_eq!(sizes.line_height, 3.2);
        assert_eq!(page_size_twips(sizes.page_size), (11906, 16838));
    }

    #[test]
    fn builds_valid_docx_package_with_requested_page_size() {
        let sizes = TextSizes {
            page_size: PageSize {
                width: 210.0,
                height: 297.0,
                unit: "mm",
            },
            ..TextSizes::default_body()
        };
        let bytes = build_docx(&["<w:p/>".to_string()], sizes).expect("DOCX should build");
        assert!(bytes.starts_with(b"PK\x03\x04"));
        let package_text = String::from_utf8_lossy(&bytes);
        assert!(package_text.contains("word/document.xml"));
        assert!(package_text.contains("w:pgSz w:w=\"11906\" w:h=\"16838\""));
    }

    #[test]
    fn renders_safe_preview_and_generates_annotated_docx() {
        let (preview, _) = render_preview_html(
            state(),
            "The glittered rose <script>alert('x')</script>.",
            "Reading Test",
            "glittered=ˈɡlɪt.ərd=闪闪发光",
            true,
            "P4",
        )
        .expect("preview should render");
        assert!(preview.contains("preview-page"));
        assert!(preview.contains("preview-token"));
        assert!(!preview.contains("<script>"));
        assert!(preview.contains("&lt;"));

        let output = std::env::temp_dir().join(format!("cijing-test-{}.docx", unique_suffix()));
        let result = generate_docx(
            state(),
            DEMO_TEXT,
            &output,
            "Lesson 37 The Tea Rose",
            "glittered=ˈɡlɪt.ərd=闪闪发光",
            true,
            "P4",
            TextSizes::default_body(),
        );
        assert!(result.is_ok());
        let metadata = fs::metadata(&output).expect("generated DOCX should exist");
        assert!(metadata.len() > 1_000);
        let _ = fs::remove_file(output);
    }

    #[test]
    fn serves_only_expected_download_types() {
        assert_eq!(download_mime("article.pdf"), "application/pdf");
        assert_eq!(
            download_mime("article.docx"),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn opens_embedded_ui_by_default_on_windows() {
        assert!(should_use_embedded_ui(&["词境精读.exe".to_string()]));
        assert!(!should_use_embedded_ui(&[
            "词境精读.exe".to_string(),
            "--browser".to_string(),
        ]));
        assert!(!should_use_embedded_ui(&[
            "词境精读.exe".to_string(),
            "--native".to_string(),
        ]));
        assert!(!should_use_embedded_ui(&[
            "词境精读.exe".to_string(),
            "--no-open".to_string(),
        ]));
    }
}
