#![cfg_attr(windows, windows_subsystem = "windows")]

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const INDEX_HTML: &str = include_str!("../assets/web/index.html");
const UI_LANGUAGE_PACKS_JS: &str = include_str!("../assets/web/ui-language-packs.js");
const I18N_JS: &str = include_str!("../assets/web/i18n.js");
const APP_JS: &str = include_str!("../assets/web/app.js");
const EDITOR_TOOLS_JS: &str = include_str!("../assets/web/editor-tools.js");
const STYLES_CSS: &str = include_str!("../assets/web/styles.css");
const BRAND_LOGO_PNG: &[u8] = include_bytes!("../assets/brand/yujie-logo-64.png");
#[cfg(target_os = "windows")]
const BRAND_ICON_ICO: &[u8] = include_bytes!("../assets/brand/yujie-logo.ico");
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
const REMOTE_TRANSLATION_COOLDOWN: Duration = Duration::from_secs(5 * 60);

fn background_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

fn spawn_background_powershell(args: &[String]) -> std::io::Result<std::process::Child> {
    let mut last_not_found = None;
    for program in ["powershell.exe", "pwsh.exe", "pwsh"] {
        match background_command(program)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => return Ok(child),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                last_not_found = Some(error);
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_not_found.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "PowerShell is not installed")
    }))
}

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
    remote_translation: Mutex<RemoteTranslationState>,
}

#[derive(Default)]
struct RemoteTranslationState {
    cooldown_until: Option<Instant>,
    in_flight: bool,
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
            remote_translation: Mutex::new(RemoteTranslationState::default()),
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

fn target_language_name(code: &str) -> Option<&'static str> {
    Some(match code {
        "zh-Hans" => "简体中文",
        "zh-Hant" => "繁体中文",
        "en" => "英语",
        "ja" => "日语",
        "ko" => "韩语",
        "es" => "西班牙语",
        "fr" => "法语",
        "de" => "德语",
        "pt-BR" => "巴西葡萄牙语",
        "pt-PT" => "欧洲葡萄牙语",
        "ru" => "俄语",
        "ar" => "阿拉伯语",
        "hi" => "印地语",
        "vi" => "越南语",
        "th" => "泰语",
        "id" => "印度尼西亚语",
        "ms" => "马来语",
        "fil" => "菲律宾语",
        "my" => "缅甸语",
        "km" => "高棉语",
        "lo" => "老挝语",
        "mn" => "蒙古语",
        "mi" => "毛利语",
        "jv" => "爪哇语",
        "su" => "巽他语",
        "ceb" => "宿务语",
        "bo" => "藏语",
        "ug" => "维吾尔语",
        "bn" => "孟加拉语",
        "ur" => "乌尔都语",
        "pa" => "旁遮普语",
        "ta" => "泰米尔语",
        "te" => "泰卢固语",
        "mr" => "马拉地语",
        "gu" => "古吉拉特语",
        "kn" => "卡纳达语",
        "ml" => "马拉雅拉姆语",
        "ne" => "尼泊尔语",
        "si" => "僧伽罗语",
        "uz" => "乌兹别克语",
        "kk" => "哈萨克语",
        "ps" => "普什图语",
        "sd" => "信德语",
        "ky" => "吉尔吉斯语",
        "tg" => "塔吉克语",
        "tk" => "土库曼语",
        "it" => "意大利语",
        "nl" => "荷兰语",
        "pl" => "波兰语",
        "tr" => "土耳其语",
        "uk" => "乌克兰语",
        "cs" => "捷克语",
        "ro" => "罗马尼亚语",
        "hu" => "匈牙利语",
        "el" => "希腊语",
        "sv" => "瑞典语",
        "da" => "丹麦语",
        "no" => "挪威语",
        "fi" => "芬兰语",
        "sk" => "斯洛伐克语",
        "sl" => "斯洛文尼亚语",
        "hr" => "克罗地亚语",
        "sr" => "塞尔维亚语",
        "bg" => "保加利亚语",
        "lt" => "立陶宛语",
        "lv" => "拉脱维亚语",
        "et" => "爱沙尼亚语",
        "ca" => "加泰罗尼亚语",
        "eu" => "巴斯克语",
        "gl" => "加利西亚语",
        "ga" => "爱尔兰语",
        "cy" => "威尔士语",
        "is" => "冰岛语",
        "sq" => "阿尔巴尼亚语",
        "mk" => "马其顿语",
        "be" => "白俄罗斯语",
        "mt" => "马耳他语",
        "lb" => "卢森堡语",
        "fa" => "波斯语",
        "he" => "希伯来语",
        "hy" => "亚美尼亚语",
        "ka" => "格鲁吉亚语",
        "az" => "阿塞拜疆语",
        "ku" => "库尔德语",
        "ht" => "海地克里奥尔语",
        "sw" => "斯瓦希里语",
        "af" => "南非语",
        "am" => "阿姆哈拉语",
        "so" => "索马里语",
        "ha" => "豪萨语",
        "yo" => "约鲁巴语",
        "zu" => "祖鲁语",
        "ig" => "伊博语",
        "om" => "奥罗莫语",
        "xh" => "科萨语",
        "rw" => "卢旺达语",
        "mg" => "马达加斯加语",
        "ny" => "齐切瓦语",
        _ => return None,
    })
}

fn target_language(body: &str) -> String {
    let code = json_string(body, "targetLanguage").unwrap_or_else(|| "zh-Hans".to_string());
    if target_language_name(&code).is_some() {
        code
    } else {
        "zh-Hans".to_string()
    }
}

fn pronunciation_scheme(body: &str) -> String {
    let scheme = json_string(body, "pronunciationScheme").unwrap_or_else(|| "ipa-us".to_string());
    match scheme.as_str() {
        "ipa-us" | "ipa-uk" | "ipa" | "target-friendly" | "syllable" | "none" => scheme,
        _ => "ipa-us".to_string(),
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
        let phones: Vec<String> = parts
            .take_while(|phone| !phone.starts_with('#'))
            .map(ToOwned::to_owned)
            .collect();
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
        if parts.len() >= 3 && !parts[0].is_empty() && !parts[2].is_empty() {
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
    use_offline_translation: bool,
    use_pronunciation: bool,
) -> Option<LexiconEntry> {
    for candidate in candidate_lemmas(word) {
        if let Some(entry) = lexicon.get(&candidate) {
            let mut resolved = entry.clone();
            if use_pronunciation && resolved.ipa.trim().is_empty() {
                resolved.ipa = lookup_generated_ipa(state, word).unwrap_or_default();
            }
            if use_offline_translation && resolved.zh.trim().is_empty() {
                resolved.zh = lookup_generated_translation(state, word).unwrap_or_default();
            }
            return Some(resolved);
        }
    }
    let generated_ipa = if use_pronunciation {
        lookup_generated_ipa(state, word).unwrap_or_default()
    } else {
        String::new()
    };
    let translation = if use_offline_translation {
        lookup_generated_translation(state, word).unwrap_or_default()
    } else {
        String::new()
    };
    if generated_ipa.is_empty() && translation.is_empty() {
        return None;
    }
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

fn normalize_pronunciation_source(raw: &str) -> String {
    raw.trim()
        .trim_start_matches(['(', '[', '/'])
        .trim_end_matches([')', ']', '/'])
        .trim()
        .to_string()
}

fn british_ipa_approximation(source: &str) -> String {
    let normalized = source
        .replace("ɝ", "ɜː")
        .replace("ɚ", "ə")
        .replace("oʊ", "əʊ");
    let chars: Vec<char> = normalized.chars().collect();
    let mut out = String::new();
    for (index, ch) in chars.iter().copied().enumerate() {
        if ch == 'ɛ' {
            out.push('e');
            continue;
        }
        out.push(ch);
        if matches!(ch, 'ɑ' | 'u' | 'i') && chars.get(index + 1) != Some(&'ː') {
            out.push('ː');
        }
    }
    out
}

fn generic_ipa(source: &str) -> String {
    source.replace("ɝ", "ɜr").replace("ɚ", "ər")
}

fn readable_respelling(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::new();
    let mut index = 0usize;
    while index < chars.len() {
        let pair = if index + 1 < chars.len() {
            Some((chars[index], chars[index + 1]))
        } else {
            None
        };
        let paired = match pair {
            Some(('t', 'ʃ')) => Some("ch"),
            Some(('d', 'ʒ')) => Some("j"),
            Some(('a', 'ʊ')) => Some("ow"),
            Some(('a', 'ɪ')) => Some("eye"),
            Some(('e', 'ɪ')) => Some("ay"),
            Some(('o', 'ʊ')) => Some("oh"),
            Some(('ɔ', 'ɪ')) => Some("oy"),
            _ => None,
        };
        if let Some(value) = paired {
            out.push_str(value);
            index += 2;
            continue;
        }
        match chars[index] {
            'ɝ' => out.push_str("ur"),
            'ɚ' => out.push_str("er"),
            'ɑ' => out.push_str("ah"),
            'æ' => out.push('a'),
            'ɔ' => out.push_str("aw"),
            'ɛ' => out.push_str("eh"),
            'ɪ' => out.push_str("ih"),
            'i' => out.push_str("ee"),
            'ʊ' => out.push('u'),
            'u' => out.push_str("oo"),
            'ʌ' | 'ə' => out.push_str("uh"),
            'θ' | 'ð' => out.push_str("th"),
            'ʃ' => out.push_str("sh"),
            'ʒ' => out.push_str("zh"),
            'ŋ' => out.push_str("ng"),
            'ɡ' => out.push('g'),
            'j' => out.push('y'),
            'ˈ' => out.push('\''),
            'ˌ' => out.push(','),
            '.' | '·' => out.push('-'),
            other => out.push(other),
        }
        index += 1;
    }
    out
}

fn syllable_pronunciation(source: &str) -> String {
    let mut chars = Vec::new();
    let mut boundaries = HashSet::new();
    for ch in source.chars() {
        if matches!(ch, '.' | '·') {
            boundaries.insert(chars.len());
        } else {
            chars.push(ch);
        }
    }

    let is_vowel = |ch: char| {
        matches!(
            ch,
            'a' | 'e'
                | 'i'
                | 'o'
                | 'u'
                | 'ɑ'
                | 'æ'
                | 'ɔ'
                | 'ɛ'
                | 'ɪ'
                | 'ʊ'
                | 'ʌ'
                | 'ə'
                | 'ɚ'
                | 'ɝ'
                | 'ɜ'
        )
    };
    let is_diphthong = |first: char, second: char| {
        matches!(
            (first, second),
            ('a', 'ʊ') | ('a', 'ɪ') | ('e', 'ɪ') | ('o', 'ʊ') | ('ɔ', 'ɪ')
        )
    };
    let mut vowel_spans = Vec::new();
    let mut index = 0usize;
    while index < chars.len() {
        if is_vowel(chars[index]) {
            let end = if index + 1 < chars.len() && is_diphthong(chars[index], chars[index + 1]) {
                index + 1
            } else {
                index
            };
            vowel_spans.push((index, end));
            index = end + 1;
        } else {
            index += 1;
        }
    }

    for pair in vowel_spans.windows(2) {
        let (_, previous_end) = pair[0];
        let (next_start, _) = pair[1];
        let mut between = (previous_end + 1)..next_start;
        let stress_boundary = between
            .clone()
            .find(|position| matches!(chars[*position], 'ˈ' | 'ˌ'));
        let consonant_boundary = between.rfind(|position| !matches!(chars[*position], 'ˈ' | 'ˌ'));
        boundaries.insert(stress_boundary.or(consonant_boundary).unwrap_or(next_start));
    }

    let mut out = String::new();
    for (index, ch) in chars.into_iter().enumerate() {
        if boundaries.contains(&index) && !out.is_empty() && !out.ends_with('·') {
            out.push('·');
        }
        out.push(ch);
    }
    out.trim_matches('·').to_string()
}

fn format_pronunciation(raw: &str, scheme: &str) -> String {
    if scheme == "none" {
        return String::new();
    }
    let source = normalize_pronunciation_source(raw);
    if source.is_empty() {
        return String::new();
    }
    let formatted = match scheme {
        "ipa-uk" => british_ipa_approximation(&source),
        "ipa" => generic_ipa(&source),
        "target-friendly" => readable_respelling(&source),
        "syllable" => syllable_pronunciation(&source),
        _ => source,
    };
    if formatted.is_empty() {
        String::new()
    } else {
        format!("({formatted})")
    }
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
    let contains_cjk = value.chars().any(is_cjk);

    for raw_clause in value.split(['\n', ';', '；', '。']) {
        let mut clause = strip_pos_prefix(raw_clause.trim()).to_string();
        if clause.is_empty() {
            continue;
        }
        if !contains_cjk {
            clause = clause
                .trim_matches(|ch: char| ch.is_whitespace() || " ,，、:：".contains(ch))
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if !clause.is_empty() {
                return take_chars(&clause, max_length.max(48));
            }
            continue;
        }
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
    target_language: &str,
    pronunciation_scheme: &str,
    sizes: TextSizes,
) -> Result<Vec<String>, String> {
    let article_text = prepare_article_text(article_text, title)?;
    let title = title.trim();
    let (mut lexicon, hard_terms, profile, known_words) =
        annotation_context(state, custom_annotations, grade_code, target_language)?;

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
            target_language == "zh-Hans",
            pronunciation_scheme,
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
            target_language == "zh-Hans",
            pronunciation_scheme,
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

#[derive(Clone, Copy)]
struct PreviewOptions<'a> {
    title: &'a str,
    custom_annotations: &'a str,
    annotate_unknown: bool,
    grade_code: &'a str,
    target_language: &'a str,
    pronunciation_scheme: &'a str,
}

fn render_preview_html(
    state: &AppState,
    article_text: &str,
    options: PreviewOptions<'_>,
) -> Result<(String, Vec<String>), String> {
    let article_text = prepare_article_text(article_text, options.title)?;
    let title = options.title.trim();
    let (mut lexicon, hard_terms, profile, known_words) = annotation_context(
        state,
        options.custom_annotations,
        options.grade_code,
        options.target_language,
    )?;
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
            options.annotate_unknown,
            &mut missing_terms,
            options.target_language == "zh-Hans",
            options.pronunciation_scheme,
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
            options.annotate_unknown,
            &mut missing_terms,
            options.target_language == "zh-Hans",
            options.pronunciation_scheme,
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
    target_language: &str,
    pronunciation_scheme: &str,
    sizes: TextSizes,
) -> Result<Vec<String>, String> {
    let (preview_html, missing) = render_preview_html(
        state,
        article_text,
        PreviewOptions {
            title,
            custom_annotations,
            annotate_unknown,
            grade_code,
            target_language,
            pronunciation_scheme,
        },
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
            "<title>语界精读 PDF</title><style>",
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
    let mut child = background_command(browser)
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
        if background_command(name).arg("--version").output().is_ok() {
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
    target_language: &str,
) -> Result<AnnotationContext<'a>, String> {
    let mut lexicon = state.seed_lexicon.clone();
    if target_language != "zh-Hans" {
        for entry in lexicon.values_mut() {
            entry.zh.clear();
        }
    }
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
    use_offline_translation: bool,
    pronunciation_scheme: &str,
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
        let entry = lookup_entry(
            state,
            &token,
            lexicon,
            use_offline_translation,
            pronunciation_scheme != "none",
        );
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
        let ipa = format_pronunciation(&entry.ipa, pronunciation_scheme);
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
    let target_language = target_language(body);
    let pronunciation_scheme = pronunciation_scheme(body);
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
    if target_language != "zh-Hans" {
        for entry in lexicon.values_mut() {
            entry.zh.clear();
        }
    }
    let entry = lookup_entry(
        state,
        word,
        &mut lexicon,
        target_language == "zh-Hans",
        true,
    );
    if let Some(entry) = entry {
        let formatted_ipa = format_pronunciation(&entry.ipa, &pronunciation_scheme);
        let formatted_ipa = normalize_pronunciation_source(&formatted_ipa);
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
                json_escape(&formatted_ipa),
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
    let target_language = target_language(body);
    let pronunciation_scheme = pronunciation_scheme(body);
    if article.trim().is_empty() {
        return (400, "{\"error\":\"请先粘贴英文文章。\"}".to_string());
    }

    match render_preview_html(
        state,
        &article,
        PreviewOptions {
            title: &title,
            custom_annotations: &custom_words,
            annotate_unknown,
            grade_code: &grade,
            target_language: &target_language,
            pronunciation_scheme: &pronunciation_scheme,
        },
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

fn handle_builtin_translate(state: &AppState, body: &str) -> (u16, String) {
    let article = json_string(body, "article").unwrap_or_default();
    let grade = json_string(body, "grade").unwrap_or_else(|| "P4".to_string());
    let custom_words = json_string(body, "customWords").unwrap_or_default();
    let target_language = target_language(body);
    if article.trim().is_empty() {
        return (400, "{\"error\":\"请先粘贴英文文章。\"}".to_string());
    }
    if matches!(target_language.as_str(), "zh-Hans") {
        return (
            200,
            "{\"ok\":true,\"annotations\":\"\",\"count\":0,\"fallback\":false,\"warning\":\"\",\"reason\":\"\",\"actualLanguage\":\"zh-Hans\",\"retryAfterMs\":0}"
                .to_string(),
        );
    }
    let words = match builtin_translation_candidates(state, &article, &custom_words, &grade) {
        Ok(words) => words,
        Err(error) => return (400, format!("{{\"error\":\"{}\"}}", json_escape(&error))),
    };
    if words.is_empty() {
        return (
            200,
            format!(
                "{{\"ok\":true,\"annotations\":\"\",\"count\":0,\"fallback\":false,\"warning\":\"\",\"reason\":\"\",\"actualLanguage\":\"{}\",\"retryAfterMs\":0}}",
                json_escape(&target_language)
            ),
        );
    }

    let outcome = call_builtin_translation(state, &target_language, &words);
    let count = outcome
        .annotations
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    (
        200,
        format!(
            "{{\"ok\":true,\"annotations\":\"{}\",\"count\":{},\"fallback\":{},\"warning\":\"{}\",\"reason\":\"{}\",\"actualLanguage\":\"{}\",\"retryAfterMs\":{}}}",
            json_escape(&outcome.annotations),
            count,
            outcome.fallback,
            json_escape(outcome.warning),
            json_escape(outcome.reason),
            json_escape(&outcome.actual_language),
            outcome.retry_after_ms
        ),
    )
}

fn handle_generate_pdf(state: &AppState, body: &str) -> (u16, String) {
    let article = json_string(body, "article").unwrap_or_default();
    let title = json_string(body, "title").unwrap_or_default();
    let grade = json_string(body, "grade").unwrap_or_else(|| "P4".to_string());
    let custom_words = json_string(body, "customWords").unwrap_or_default();
    let annotate_unknown = json_bool(body, "annotateUnknown").unwrap_or(true);
    let target_language = target_language(body);
    let pronunciation_scheme = pronunciation_scheme(body);
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
        &target_language,
        &pronunciation_scheme,
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
    let target_language = target_language(body);
    let pronunciation_scheme = pronunciation_scheme(body);
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
        &target_language,
        &pronunciation_scheme,
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

fn builtin_translation_candidates(
    state: &AppState,
    article: &str,
    custom_words: &str,
    grade: &str,
) -> Result<Vec<String>, String> {
    let (custom_entries, _, _) = parse_custom_annotations(custom_words);
    let manual_terms: HashSet<String> = custom_entries
        .iter()
        .flat_map(|entry| key_variants(&entry.term))
        .collect();
    let (mut lexicon, hard_terms, profile, known_words) =
        annotation_context(state, custom_words, grade, "zh-Hans")?;
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
        let entry = lookup_entry(state, &token, &mut lexicon, true, true);
        if candidate_lemmas(&token)
            .iter()
            .any(|candidate| manual_terms.contains(candidate))
        {
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
            if words.len() >= 48 {
                break;
            }
        }
    }
    Ok(words)
}

struct BuiltinTranslationOutcome {
    annotations: String,
    fallback: bool,
    warning: &'static str,
    reason: &'static str,
    actual_language: String,
    retry_after_ms: u64,
}

enum RemoteTranslationDecision {
    Attempt,
    CoolingDown(u64),
    Busy,
}

fn begin_remote_translation(state: &AppState) -> RemoteTranslationDecision {
    let mut remote = state
        .remote_translation
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let now = Instant::now();
    if let Some(deadline) = remote.cooldown_until
        && now < deadline
    {
        let retry_after_ms = deadline
            .saturating_duration_since(now)
            .as_millis()
            .clamp(1_000, u64::MAX as u128) as u64;
        return RemoteTranslationDecision::CoolingDown(retry_after_ms);
    }
    remote.cooldown_until = None;
    if remote.in_flight {
        return RemoteTranslationDecision::Busy;
    }
    remote.in_flight = true;
    RemoteTranslationDecision::Attempt
}

fn finish_remote_translation(state: &AppState, succeeded: bool) {
    let mut remote = state
        .remote_translation
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    remote.in_flight = false;
    remote.cooldown_until = (!succeeded).then(|| Instant::now() + REMOTE_TRANSLATION_COOLDOWN);
}

fn unavailable_translation_outcome(
    warning: &'static str,
    reason: &'static str,
    retry_after_ms: u64,
) -> BuiltinTranslationOutcome {
    BuiltinTranslationOutcome {
        annotations: String::new(),
        fallback: true,
        warning,
        reason,
        actual_language: String::new(),
        retry_after_ms,
    }
}

fn call_builtin_translation(
    state: &AppState,
    target_language: &str,
    words: &[String],
) -> BuiltinTranslationOutcome {
    let Some(provider_language) = builtin_translation_language(target_language) else {
        return unavailable_translation_outcome(
            "所选语言暂不受在线翻译支持，未混入中文释义。",
            "unsupported",
            0,
        );
    };
    if target_language == "en" {
        return BuiltinTranslationOutcome {
            annotations: words
                .iter()
                .map(|word| {
                    let ipa = lookup_generated_ipa(state, word).unwrap_or_default();
                    format!("{word}={ipa}={word}")
                })
                .collect::<Vec<_>>()
                .join("\n"),
            fallback: false,
            warning: "",
            reason: "",
            actual_language: target_language.to_string(),
            retry_after_ms: 0,
        };
    }
    match begin_remote_translation(state) {
        RemoteTranslationDecision::CoolingDown(retry_after_ms) => {
            return unavailable_translation_outcome(
                "在线翻译服务正在冷却，未混入中文释义。",
                "cooldown",
                retry_after_ms,
            );
        }
        RemoteTranslationDecision::Busy => {
            return unavailable_translation_outcome(
                "在线翻译服务正忙，未混入中文释义。",
                "busy",
                1_500,
            );
        }
        RemoteTranslationDecision::Attempt => {}
    }

    let temp_dir = std::env::temp_dir().join(format!("yujie-translate-{}", unique_suffix()));
    if fs::create_dir_all(&temp_dir).is_err() {
        finish_remote_translation(state, false);
        return unavailable_translation_outcome(
            "在线翻译服务暂时不可用，未混入中文释义。",
            "unavailable",
            REMOTE_TRANSLATION_COOLDOWN.as_millis() as u64,
        );
    }
    let words_path = temp_dir.join("words.txt");
    let script_path = temp_dir.join("translate.ps1");
    if fs::write(&words_path, words.join("\n")).is_err()
        || fs::write(&script_path, builtin_translation_powershell_script()).is_err()
    {
        let _ = fs::remove_dir_all(&temp_dir);
        finish_remote_translation(state, false);
        return unavailable_translation_outcome(
            "在线翻译服务暂时不可用，未混入中文释义。",
            "unavailable",
            REMOTE_TRANSLATION_COOLDOWN.as_millis() as u64,
        );
    }
    let result = run_builtin_translation_powershell(&script_path, provider_language, &words_path);
    let _ = fs::remove_dir_all(&temp_dir);
    match result {
        Ok(text) => {
            let annotations = text
                .lines()
                .filter_map(|line| {
                    let (word, translation) = line.split_once('\t')?;
                    let word = normalize_key(word);
                    let translation = translation
                        .replace(['\r', '\n'], " ")
                        .replace(['=', '|'], " ")
                        .trim()
                        .to_string();
                    if word.is_empty() || translation.is_empty() {
                        return None;
                    }
                    let ipa = lookup_generated_ipa(state, &word).unwrap_or_default();
                    Some(format!("{word}={ipa}={translation}"))
                })
                .collect::<Vec<_>>()
                .join("\n");
            let translated_count = annotations
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count();
            if translated_count == 0 {
                finish_remote_translation(state, false);
                unavailable_translation_outcome(
                    "在线翻译未返回可用的目标语释义，未混入中文释义。",
                    "incomplete",
                    REMOTE_TRANSLATION_COOLDOWN.as_millis() as u64,
                )
            } else {
                finish_remote_translation(state, true);
                BuiltinTranslationOutcome {
                    annotations,
                    fallback: false,
                    warning: "",
                    reason: "",
                    actual_language: target_language.to_string(),
                    retry_after_ms: 0,
                }
            }
        }
        Err(error) => {
            finish_remote_translation(state, false);
            let (warning, reason) = match error.as_str() {
                "TRANSLATION_RATE_LIMITED" => {
                    ("在线翻译服务请求过多，未混入中文释义。", "rate_limited")
                }
                "TRANSLATION_INCOMPLETE" | "TRANSLATION_EMPTY" => (
                    "在线翻译未返回可用的目标语释义，未混入中文释义。",
                    "incomplete",
                ),
                _ => ("在线翻译服务暂时不可用，未混入中文释义。", "unavailable"),
            };
            unavailable_translation_outcome(
                warning,
                reason,
                REMOTE_TRANSLATION_COOLDOWN.as_millis() as u64,
            )
        }
    }
}

fn builtin_translation_language(code: &str) -> Option<String> {
    target_language_name(code)?;
    Some(
        match code {
            "zh-Hans" => "zh-CN",
            "zh-Hant" => "zh-TW",
            "jv" => "jav",
            other => other,
        }
        .to_string(),
    )
}

fn builtin_translation_powershell_script() -> &'static str {
    r#"
param(
  [Parameter(Mandatory = $true)][string]$TargetLanguage,
  [Parameter(Mandatory = $true)][string]$WordsPath
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$utf8 = [Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = $utf8
$headers = @{
  "Accept" = "application/json"
  "User-Agent" = "YujieReader/1.4"
}

$words = @(
  Get-Content -LiteralPath $WordsPath |
    ForEach-Object { $_.Trim() } |
    Where-Object { $_.Length -gt 0 } |
    Select-Object -Unique
)
if ($words.Count -eq 0) {
  exit 0
}

$nonce = [Guid]::NewGuid().ToString("N").Substring(0, 12).ToUpperInvariant()
$prefix = "__YJW_${nonce}_"
$script:result = [ordered]@{}
$script:edgeRateLimited = $false
$script:googleRateLimited = $false
$script:myMemoryRateLimited = $false
$script:providerUnavailable = $false
$entries = [Collections.Generic.List[object]]::new()

for ($index = 0; $index -lt $words.Count; $index += 1) {
  $id = "{0:D3}" -f $index
  $word = [string]$words[$index]
  $entries.Add([pscustomobject]@{
    Word = $word
    Start = "${prefix}S_${id}__"
    End = "${prefix}E_${id}__"
  })
}

function Add-Translation {
  param([string]$Word, [string]$Value)
  $clean = [Net.WebUtility]::HtmlDecode([string]$Value)
  $clean = ($clean -replace "[\r\n\t]+", " ").Trim()
  if (
    [string]::IsNullOrWhiteSpace($clean) -or
    $clean -match "^(MYMEMORY WARNING|INVALID TARGET LANGUAGE|QUERY LENGTH LIMIT)"
  ) {
    return $false
  }
  $script:result[$Word] = $clean
  return $true
}

function Get-EdgeLanguage {
  $edgeLanguage = $TargetLanguage
  if ($edgeLanguage -eq "mn") {
    return "mn-Cyrl"
  }
  if ($edgeLanguage -eq "no") {
    return "nb"
  }
  if ($edgeLanguage -eq "sr") {
    return "sr-Cyrl"
  }
  if ($edgeLanguage -eq "ny") {
    return "nya"
  }
  if ($edgeLanguage -eq "zh-TW") {
    return "zh-Hant"
  }
  if ($edgeLanguage -eq "pt-BR") {
    return "pt"
  }
  if ($edgeLanguage -eq "pt-PT") {
    return "pt-pt"
  }
  if ($edgeLanguage -in @("jav", "su", "ceb", "tg", "om")) {
    return ""
  }
  return $edgeLanguage
}

function Invoke-EdgeBatches {
  param([object[]]$Items)
  $edgeLanguage = Get-EdgeLanguage
  if ($Items.Count -eq 0 -or [string]::IsNullOrWhiteSpace($edgeLanguage)) {
    return
  }
  try {
    $token = [string](Invoke-RestMethod `
      -Uri "https://edge.microsoft.com/translate/auth" `
      -Method Get `
      -Headers $headers `
      -TimeoutSec 8)
    if ([string]::IsNullOrWhiteSpace($token)) {
      $script:providerUnavailable = $true
      return
    }
  } catch {
    $statusCode = 0
    if ($null -ne $_.Exception.Response) {
      try {
        $statusCode = [int]$_.Exception.Response.StatusCode
      } catch {
        $statusCode = 0
      }
    }
    if ($statusCode -eq 429) {
      $script:edgeRateLimited = $true
    } else {
      $script:providerUnavailable = $true
    }
    return
  }

  for ($offset = 0; $offset -lt $Items.Count; $offset += 25) {
    $last = [Math]::Min($offset + 24, $Items.Count - 1)
    $batch = @($Items[$offset..$last])
    $texts = @(
      $batch |
        ForEach-Object { [pscustomobject]@{ Text = $_.Word } }
    )
    $body = ConvertTo-Json -InputObject $texts -Compress
    try {
      $rawResponse = Invoke-RestMethod `
        -Uri (
          "https://api-edge.cognitive.microsofttranslator.com/translate" +
          "?api-version=3.0&from=en&to=" +
          [Uri]::EscapeDataString($edgeLanguage)
        ) `
        -Method Post `
        -Headers @{
          "Authorization" = "Bearer $token"
          "X-ClientTraceId" = [Guid]::NewGuid().ToString()
        } `
        -ContentType "application/json; charset=UTF-8" `
        -Body ([Text.Encoding]::UTF8.GetBytes($body)) `
        -TimeoutSec 8
      $response = @($rawResponse)
      $limit = [Math]::Min($batch.Count, $response.Count)
      for ($index = 0; $index -lt $limit; $index += 1) {
        $value = [string]$response[$index].translations[0].text
        if (-not [string]::IsNullOrWhiteSpace($value)) {
          [void](Add-Translation -Word $batch[$index].Word -Value $value)
        }
      }
    } catch {
      $statusCode = 0
      if ($null -ne $_.Exception.Response) {
        try {
          $statusCode = [int]$_.Exception.Response.StatusCode
        } catch {
          $statusCode = 0
        }
      }
      if ($statusCode -eq 429) {
        $script:edgeRateLimited = $true
      } else {
        $script:providerUnavailable = $true
      }
      return
    }
  }
}

function Invoke-MyMemoryBatch {
  param([object[]]$Items)
  if ($Items.Count -eq 0) {
    return $true
  }
  $payload = (($Items | ForEach-Object { $_.Word }) -join " | ")
  try {
    $uri = "https://api.mymemory.translated.net/get?q=" +
      [Uri]::EscapeDataString($payload) +
      "&langpair=en%7C" +
      [Uri]::EscapeDataString($TargetLanguage)
    $response = Invoke-RestMethod `
      -Uri $uri `
      -Method Get `
      -Headers $headers `
      -TimeoutSec 8
    $responseStatus = [int]$response.responseStatus
    $translated = [Net.WebUtility]::HtmlDecode(
      [string]$response.responseData.translatedText
    )
    if (
      $responseStatus -eq 429 -or
      [string]$response.quotaFinished -eq "True" -or
      $translated -match "^MYMEMORY WARNING"
    ) {
      $script:myMemoryRateLimited = $true
      return $false
    }
    if (
      $responseStatus -ne 200 -or
      [string]::IsNullOrWhiteSpace($translated) -or
      $translated -match "^(INVALID TARGET LANGUAGE|QUERY LENGTH LIMIT)"
    ) {
      $script:providerUnavailable = $true
      return $false
    }
    $parts = @($translated -split "\|")
    if ($parts.Count -ne $Items.Count) {
      $script:providerUnavailable = $true
      return $false
    }
    $pending = [ordered]@{}
    for ($index = 0; $index -lt $Items.Count; $index += 1) {
      $value = ($parts[$index] -replace "[\r\n\t]+", " ").Trim()
      if (
        [string]::IsNullOrWhiteSpace($value) -or
        $value -match "^(MYMEMORY WARNING|INVALID TARGET LANGUAGE|QUERY LENGTH LIMIT)"
      ) {
        continue
      }
      $pending[$Items[$index].Word] = $value
    }
    foreach ($translation in $pending.GetEnumerator()) {
      [void](Add-Translation -Word $translation.Key -Value $translation.Value)
    }
    return $pending.Count -gt 0
  } catch {
    $statusCode = 0
    if ($null -ne $_.Exception.Response) {
      try {
        $statusCode = [int]$_.Exception.Response.StatusCode
      } catch {
        $statusCode = 0
      }
    }
    if ($statusCode -eq 429) {
      $script:myMemoryRateLimited = $true
    } else {
      $script:providerUnavailable = $true
    }
    return $false
  }
}

function Invoke-MyMemoryBatches {
  param([object[]]$Items)
  $current = [Collections.Generic.List[object]]::new()
  foreach ($item in $Items) {
    $candidateWords = @($current | ForEach-Object { $_.Word }) + @($item.Word)
    $candidate = ($candidateWords -join " | ")
    if (
      $current.Count -gt 0 -and
      [Text.Encoding]::UTF8.GetByteCount($candidate) -gt 420
    ) {
      [void](Invoke-MyMemoryBatch -Items $current.ToArray())
      $current.Clear()
    }
    $current.Add($item)
  }
  if ($current.Count -gt 0) {
    [void](Invoke-MyMemoryBatch -Items $current.ToArray())
  }
}

function Get-TokenCount {
  param([string]$Text, [string]$Token)
  $count = 0
  $offset = 0
  while ($offset -le $Text.Length) {
    $found = $Text.IndexOf($Token, $offset, [StringComparison]::Ordinal)
    if ($found -lt 0) {
      break
    }
    $count += 1
    $offset = $found + $Token.Length
  }
  return $count
}

function Invoke-GoogleProvider {
  param([string]$Payload)
  $googleLanguage = $TargetLanguage
  if ($googleLanguage -eq "jav") {
    $googleLanguage = "jw"
  } elseif ($googleLanguage -eq "fil") {
    $googleLanguage = "tl"
  } elseif ($googleLanguage -in @("pt-BR", "pt-PT")) {
    $googleLanguage = "pt"
  }
  try {
    $response = Invoke-RestMethod `
      -Uri "https://translate.googleapis.com/translate_a/single" `
      -Method Post `
      -Headers $headers `
      -ContentType "application/x-www-form-urlencoded; charset=UTF-8" `
      -Body @{
        client = "gtx"
        sl = "en"
        tl = $googleLanguage
        dt = "t"
        q = $Payload
      } `
      -TimeoutSec 8
    if ($null -eq $response -or $null -eq $response[0]) {
      return $null
    }
    $translated = (
      $response[0] |
        ForEach-Object {
          if ($null -ne $_ -and $null -ne $_[0]) {
            [string]$_[0]
          }
        }
    ) -join ""
    if ([string]::IsNullOrWhiteSpace($translated)) {
      return $null
    }
    return $translated
  } catch {
    $statusCode = 0
    if ($null -ne $_.Exception.Response) {
      try {
        $statusCode = [int]$_.Exception.Response.StatusCode
      } catch {
        $statusCode = 0
      }
    }
    if ($statusCode -eq 429) {
      $script:googleRateLimited = $true
    } else {
      $script:providerUnavailable = $true
    }
    return $null
  }
}

function Try-ProcessGoogleBatch {
  param([object[]]$Items)
  $records = [Collections.Generic.List[string]]::new()
  foreach ($item in $Items) {
    $records.Add($item.Start + $item.Word + $item.End)
  }
  $translated = Invoke-GoogleProvider -Payload ($records -join "`n")
  if ([string]::IsNullOrWhiteSpace($translated)) {
    return $false
  }
  $pending = [ordered]@{}
  foreach ($item in $Items) {
    if ((Get-TokenCount -Text $translated -Token $item.Start) -ne 1) {
      return $false
    }
    if ((Get-TokenCount -Text $translated -Token $item.End) -ne 1) {
      return $false
    }
    $startIndex = $translated.IndexOf($item.Start, [StringComparison]::Ordinal)
    $valueStart = $startIndex + $item.Start.Length
    $endIndex = $translated.IndexOf(
      $item.End,
      $valueStart,
      [StringComparison]::Ordinal
    )
    if ($startIndex -lt 0 -or $endIndex -lt $valueStart) {
      return $false
    }
    $value = $translated.Substring($valueStart, $endIndex - $valueStart).Trim()
    if ([string]::IsNullOrWhiteSpace($value) -or $value.Contains($prefix)) {
      return $false
    }
    $pending[$item.Word] = $value
  }
  foreach ($translation in $pending.GetEnumerator()) {
    $script:result[$translation.Key] = $translation.Value
  }
  return $true
}

Invoke-EdgeBatches -Items ($entries.ToArray())
$myMemoryMissing = @(
  $entries |
    Where-Object { -not $script:result.Contains($_.Word) }
)
if ($myMemoryMissing.Count -gt 0) {
  Invoke-MyMemoryBatches -Items $myMemoryMissing
}
$missing = @(
  $entries |
    Where-Object { -not $script:result.Contains($_.Word) }
)
if ($missing.Count -gt 0) {
  [void](Try-ProcessGoogleBatch -Items $missing)
}

if ($script:result.Count -eq 0) {
  if (
    $script:edgeRateLimited -or
    $script:googleRateLimited -or
    $script:myMemoryRateLimited
  ) {
    [Console]::Error.WriteLine("RATE_LIMITED")
    exit 29
  }
  [Console]::Error.WriteLine("PROVIDER_UNAVAILABLE")
  exit 30
}
foreach ($word in $words) {
  if ($script:result.Contains($word)) {
    "$word`t$($script:result[$word])"
  }
}
"#
}

fn run_builtin_translation_powershell(
    script_path: &Path,
    target_language: String,
    words_path: &Path,
) -> Result<String, String> {
    let args = vec![
        "-NoProfile".to_string(),
        "-NonInteractive".to_string(),
        "-ExecutionPolicy".to_string(),
        "Bypass".to_string(),
        "-File".to_string(),
        script_path.to_string_lossy().into_owned(),
        "-TargetLanguage".to_string(),
        target_language,
        "-WordsPath".to_string(),
        words_path.to_string_lossy().into_owned(),
    ];
    let mut child =
        spawn_background_powershell(&args).map_err(|err| format!("无法启动内置翻译：{err}"))?;

    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|err| err.to_string())? {
            break status;
        }
        if started.elapsed() > Duration::from_secs(20) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("TRANSLATION_TIMEOUT".to_string());
        }
        thread::sleep(Duration::from_millis(200));
    };

    let mut stdout_bytes = Vec::new();
    if let Some(mut stream) = child.stdout.take() {
        let _ = stream.read_to_end(&mut stdout_bytes);
    }
    let stdout = String::from_utf8_lossy(&stdout_bytes);
    let mut stderr_bytes = Vec::new();
    if let Some(mut stream) = child.stderr.take() {
        let _ = stream.read_to_end(&mut stderr_bytes);
    }
    let stderr = String::from_utf8_lossy(&stderr_bytes);
    if !status.success() {
        if stderr.contains("RATE_LIMITED") || stdout.contains("RATE_LIMITED") {
            return Err("TRANSLATION_RATE_LIMITED".to_string());
        }
        if stderr.contains("TRANSLATION_INCOMPLETE") {
            return Err("TRANSLATION_INCOMPLETE".to_string());
        }
        return Err("TRANSLATION_UNAVAILABLE".to_string());
    }
    if stdout.trim().is_empty() {
        return Err("TRANSLATION_EMPTY".to_string());
    }
    Ok(stdout.into_owned())
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
        ("GET", "/ui-language-packs.js") => {
            text(UI_LANGUAGE_PACKS_JS, "text/javascript; charset=utf-8")
        }
        ("GET", "/i18n.js") => text(I18N_JS, "text/javascript; charset=utf-8"),
        ("GET", "/app.js") => text(APP_JS, "text/javascript; charset=utf-8"),
        ("GET", "/editor-tools.js") => text(EDITOR_TOOLS_JS, "text/javascript; charset=utf-8"),
        ("GET", "/styles.css") => text(STYLES_CSS, "text/css; charset=utf-8"),
        ("GET", "/brand/logo-64.png") => binary(BRAND_LOGO_PNG, "image/png"),
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
        ("POST", "/api/builtin-translate") => {
            let (status, payload) = handle_builtin_translate(state, body);
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

fn binary(body: &[u8], mime: &'static str) -> (u16, &'static str, Vec<u8>, Vec<String>) {
    (200, mime, body.to_vec(), Vec::new())
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
        let _ = background_command("cmd")
            .args(["/C", "start", "", url])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = background_command("open").arg(url).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = background_command("xdg-open").arg(url).spawn();
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
            "zh-Hans",
            "ipa-us",
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

    println!("语界精读 Rust 版运行中: {url}");
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
    const IMAGE_ICON: Uint = 1;
    const LR_LOADFROMFILE: Uint = 0x0010;
    const LR_DEFAULTSIZE: Uint = 0x0040;

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
        fn LoadImageW(
            h_instance: Hinstance,
            name: *const u16,
            image_type: Uint,
            width: i32,
            height: i32,
            load_flags: Uint,
        ) -> *mut c_void;
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

    fn load_brand_icon() -> Hicon {
        let icon_path = std::env::temp_dir().join("yujie-reader-icon.ico");
        if fs::write(&icon_path, BRAND_ICON_ICO).is_err() {
            return null_mut();
        }
        let icon_path = wide(&icon_path.to_string_lossy());
        unsafe {
            LoadImageW(
                null_mut(),
                icon_path.as_ptr(),
                IMAGE_ICON,
                0,
                0,
                LR_LOADFROMFILE | LR_DEFAULTSIZE,
            ) as Hicon
        }
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
            let brand_icon = load_brand_icon();
            let class = WndClassW {
                style: 0,
                lpfn_wnd_proc: Some(webview_window_proc),
                cb_cls_extra: 0,
                cb_wnd_extra: 0,
                h_instance: instance,
                h_icon: brand_icon,
                h_cursor: LoadCursorW(null_mut(), 32512usize as *const u16),
                hbr_background: (16 + 1) as Hbrush,
                lpsz_menu_name: null(),
                lpsz_class_name: class_name.as_ptr(),
            };
            if RegisterClassW(&class) == 0 {
                return Err("注册内嵌界面窗口失败。".to_string());
            }

            let window_title = wide("语界精读");
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
                        let _ = background_command("explorer").arg(output_dir).spawn();
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
            let brand_icon = load_brand_icon();
            let class = WndClassW {
                style: 0,
                lpfn_wnd_proc: Some(window_proc),
                cb_cls_extra: 0,
                cb_wnd_extra: 0,
                h_instance: instance,
                h_icon: brand_icon,
                h_cursor: LoadCursorW(null_mut(), 32512usize as *const u16),
                hbr_background: (16 + 1) as Hbrush,
                lpsz_menu_name: null(),
                lpsz_class_name: class_name.as_ptr(),
            };
            if RegisterClassW(&class) == 0 {
                return Err("注册窗口类失败。".to_string());
            }

            let data_ptr = Box::into_raw(Box::new(GuiData::new(state)));
            let window_title = wide("语界精读");
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
            "zh-Hans",
            "ipa-us",
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
                wide("语界精读").as_ptr(),
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

        let (status, mime, body, _) = route_request(state(), "GET", "/i18n.js", "");
        assert_eq!(status, 200);
        assert_eq!(mime, "text/javascript; charset=utf-8");
        assert!(
            body.windows(b"YujieI18n".len())
                .any(|window| window == b"YujieI18n")
        );

        let (status, mime, body, _) = route_request(state(), "GET", "/ui-language-packs.js", "");
        assert_eq!(status, 200);
        assert_eq!(mime, "text/javascript; charset=utf-8");
        assert_eq!(body, UI_LANGUAGE_PACKS_JS.as_bytes());
    }

    #[test]
    fn removes_user_api_and_network_lexicon_entry_points() {
        for path in ["/api/ai-enhance", "/api/network-lexicon"] {
            let (status, _, _, _) = route_request(state(), "POST", path, "{}");
            assert_eq!(status, 404);
        }
        for removed_id in [
            "aiEndpoint",
            "aiModel",
            "aiKey",
            "lexiconEndpoint",
            "lexiconKey",
        ] {
            assert!(!INDEX_HTML.contains(removed_id));
        }
    }

    #[test]
    fn validates_global_language_and_pronunciation_preferences() {
        assert_eq!(target_language_name("es"), Some("西班牙语"));
        assert_eq!(target_language_name("sw"), Some("斯瓦希里语"));
        assert_eq!(target_language_name("jv"), Some("爪哇语"));
        assert_eq!(target_language_name("ps"), Some("普什图语"));
        assert_eq!(target_language_name("ig"), Some("伊博语"));
        assert_eq!(target_language_name("not-a-language"), None);
        assert_eq!(
            target_language(r#"{"targetLanguage":"not-a-language"}"#),
            "zh-Hans"
        );
        assert_eq!(
            pronunciation_scheme(r#"{"pronunciationScheme":"none"}"#),
            "none"
        );
        assert_eq!(
            pronunciation_scheme(r#"{"pronunciationScheme":"unknown"}"#),
            "ipa-us"
        );
        assert_eq!(
            builtin_translation_language("zh-Hant").as_deref(),
            Some("zh-TW")
        );
        assert_eq!(
            builtin_translation_language("pt-BR").as_deref(),
            Some("pt-BR")
        );
        assert_eq!(builtin_translation_language("jv").as_deref(), Some("jav"));
        assert_eq!(builtin_translation_language("fil").as_deref(), Some("fil"));
        assert_eq!(builtin_translation_language("not-a-language"), None);
        let translation_script = builtin_translation_powershell_script();
        assert!(translation_script.contains("__YJW_"));
        assert!(translation_script.contains("edge.microsoft.com/translate/auth"));
        assert!(translation_script.contains("api-edge.cognitive.microsofttranslator.com"));
        assert!(translation_script.contains("$offset += 25"));
        assert!(translation_script.contains("api.mymemory.translated.net/get"));
        assert!(translation_script.contains("-Method Get"));
        assert!(translation_script.contains("-Method Post"));
        assert!(translation_script.contains("-join \" | \""));
        assert!(translation_script.contains("GetByteCount($candidate) -gt 420"));
        assert!(translation_script.contains("-TimeoutSec 8"));
        assert!(translation_script.contains("RATE_LIMITED"));
        assert!(translation_script.contains("PROVIDER_UNAVAILABLE"));
        assert!(!translation_script.contains("function Process-Entries"));
        assert!(!translation_script.contains("throw $script:lastProviderError"));
    }

    #[test]
    fn unavailable_target_translation_never_uses_chinese_or_leaks_script_details() {
        let app_state = state();
        {
            let mut remote = app_state.remote_translation.lock().unwrap();
            remote.cooldown_until = Some(Instant::now() + Duration::from_secs(60));
            remote.in_flight = false;
        }

        let body = r#"{
            "article":"The ubiquitous phenomenon bewildered everyone.",
            "grade":"P1",
            "targetLanguage":"es"
        }"#;
        let (status, payload) = handle_builtin_translate(app_state, body);

        {
            let mut remote = app_state.remote_translation.lock().unwrap();
            remote.cooldown_until = None;
            remote.in_flight = false;
        }
        assert_eq!(status, 200);
        assert!(payload.contains("\"fallback\":true"));
        assert!(payload.contains("\"warning\":\"在线翻译服务正在冷却"));
        assert!(payload.contains("\"reason\":\"cooldown\""));
        assert!(payload.contains("\"actualLanguage\":\"\""));
        assert!(payload.contains("\"retryAfterMs\":"));
        assert!(payload.contains("\"annotations\":\"\""));
        assert!(!payload.contains("ECDICT"));
        assert!(payload.contains("未混入中文释义"));
        assert!(!payload.contains("translate.ps1"));
        assert!(!payload.contains("AppData"));
        assert!(!payload.contains("FullyQualifiedErrorId"));

        {
            let mut remote = app_state.remote_translation.lock().unwrap();
            remote.cooldown_until = None;
            remote.in_flight = true;
        }
        let (busy_status, busy_payload) = handle_builtin_translate(app_state, body);
        {
            let mut remote = app_state.remote_translation.lock().unwrap();
            remote.in_flight = false;
        }
        assert_eq!(busy_status, 200);
        assert!(busy_payload.contains("\"reason\":\"busy\""));
        assert!(busy_payload.contains("\"retryAfterMs\":1500"));
    }

    #[test]
    fn local_chinese_dictionary_still_contains_definition_and_ipa() {
        let translation =
            lookup_generated_translation(state(), "beautiful").expect("definition should exist");
        let ipa = lookup_generated_ipa(state(), "beautiful").expect("IPA should exist");
        assert!(translation.contains("美"));
        assert!(!ipa.is_empty());
        assert!(lookup_generated_translation(state(), "zzzxqvnonword").is_none());
    }

    #[test]
    fn uses_only_embedded_ui_language_packs() {
        let translation_options = INDEX_HTML
            .split_once("id=\"translationLanguage\"")
            .and_then(|(_, tail)| tail.split_once("</select>"))
            .map(|(select, _)| select.matches("<option value=").count())
            .unwrap_or_default();
        assert_eq!(translation_options, 98);
        for code in ["jv", "ps", "ku", "ht", "om", "ug"] {
            assert!(INDEX_HTML.contains(&format!("<option value=\"{code}\">")));
            assert!(I18N_JS.contains(&format!("{code}: [")));
        }
        assert!(
            INDEX_HTML.contains("id=\"previewCanvas\" class=\"preview-canvas\" data-i18n-skip")
        );
        assert!(
            I18N_JS
                .contains("Object.keys(sanitized).length === Object.keys(englishCatalog).length")
        );
        let (status, _, _, _) = route_request(state(), "POST", "/api/ui-language-pack", "{}");
        assert_eq!(status, 404);
        assert!(!I18N_JS.contains("fetch(\"/api/ui-language-pack"));
    }

    #[test]
    fn uses_embedded_fast_language_switching_paths() {
        let bundle_position = INDEX_HTML
            .find("src=\"/ui-language-packs.js\"")
            .expect("embedded language bundle should load");
        let i18n_position = INDEX_HTML
            .find("src=\"/i18n.js\"")
            .expect("i18n runtime should load");
        assert!(bundle_position < i18n_position);
        assert!(UI_LANGUAGE_PACKS_JS.contains("\"version\":\"2-95eyir\""));
        assert!(!UI_LANGUAGE_PACKS_JS.contains("\"packs\":{}"));
        assert!(UI_LANGUAGE_PACKS_JS.contains("\"zh-Hant\":{"));
        assert!(UI_LANGUAGE_PACKS_JS.contains("\"ny\":{"));
        assert!(I18N_JS.contains("global.YujieUiLanguagePacks"));
        assert!(I18N_JS.contains("exportCatalog"));
        assert!(APP_JS.contains("const AUTO_TRANSLATION_CACHE_VERSION = 2"));
        assert!(APP_JS.contains("const MAX_AUTO_TRANSLATION_CACHE_ENTRIES = 12"));
        assert!(APP_JS.contains("translateAbortController"));
        assert!(APP_JS.contains("findCachedAutoTranslations"));
        assert!(APP_JS.contains("looksLikeChineseFallback"));
        assert!(APP_JS.contains("actualLanguage"));
        assert!(APP_JS.contains("未使用中文回退"));
        assert!(!APP_JS.contains("transientTranslationFallbacks"));
        assert!(!APP_JS.contains("select.disabled = true"));
        assert!(I18N_JS.contains("control.disabled = false"));
        assert!(EDITOR_TOOLS_JS.contains("\"autoTranslationCache\""));
        assert!(INDEX_HTML.contains("id=\"autoTranslationCache\""));
    }

    #[test]
    fn parses_batched_translation_script_without_network() {
        let temp_dir = std::env::temp_dir().join(format!("yujie-script-test-{}", unique_suffix()));
        fs::create_dir_all(&temp_dir).unwrap();
        let script_path = temp_dir.join("translate.ps1");
        let words_path = temp_dir.join("words.txt");
        fs::write(&script_path, builtin_translation_powershell_script()).unwrap();
        fs::write(&words_path, "").unwrap();
        let output = background_command("powershell.exe")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                &script_path.to_string_lossy(),
                "-TargetLanguage",
                "es",
                "-WordsPath",
                &words_path.to_string_lossy(),
            ])
            .output()
            .unwrap();
        let _ = fs::remove_dir_all(&temp_dir);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn renders_every_enabled_pronunciation_scheme() {
        let source = "ˈbjutəfəl";
        let us = format_pronunciation(source, "ipa-us");
        let uk = format_pronunciation(source, "ipa-uk");
        let generic = format_pronunciation(source, "ipa");
        let friendly = format_pronunciation(source, "target-friendly");
        let syllable = format_pronunciation(source, "syllable");

        for rendered in [&us, &uk, &generic, &friendly, &syllable] {
            assert!(!rendered.is_empty());
        }
        assert_ne!(us, uk);
        assert_ne!(us, friendly);
        assert_ne!(us, syllable);
        assert!(syllable.contains('·'));
        assert_eq!(format_pronunciation(source, "none"), "");
    }

    #[test]
    fn cmudict_comments_are_not_loaded_as_phonemes() {
        assert!(
            state()
                .pronunciations
                .values()
                .flatten()
                .flatten()
                .all(|phone| !phone.starts_with('#'))
        );
    }

    #[test]
    fn non_chinese_preview_avoids_offline_chinese_and_can_hide_pronunciation() {
        let body = r#"{
                "article":"The glittered rose.",
                "grade":"P4",
                "customWords":"glittered==destello",
                "targetLanguage":"es",
                "pronunciationScheme":"none"
            }"#;
        let custom = json_string(body, "customWords").expect("custom words should parse");
        let (entries, _, _) = parse_custom_annotations(&custom);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].zh, "destello");
        let (mut lexicon, _, _, _) =
            annotation_context(state(), &custom, "P4", "es").expect("context should build");
        let entry = lookup_entry(state(), "glittered", &mut lexicon, false, false)
            .expect("custom translation should override offline entries");
        assert_eq!(entry.zh, "destello");
        let (status, payload) = handle_preview(state(), body);
        assert_eq!(status, 200);
        assert!(payload.contains("destello"), "{payload}");
        assert!(!payload.contains("闪闪发光"));
        assert!(!payload.contains("ˈɡlɪt"));
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
            PreviewOptions {
                title: "Reading Test",
                custom_annotations: "glittered=ˈɡlɪt.ərd=闪闪发光",
                annotate_unknown: true,
                grade_code: "P4",
                target_language: "zh-Hans",
                pronunciation_scheme: "ipa-us",
            },
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
            "zh-Hans",
            "ipa-us",
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
        assert!(should_use_embedded_ui(&["语界精读.exe".to_string()]));
        assert!(!should_use_embedded_ui(&[
            "语界精读.exe".to_string(),
            "--browser".to_string(),
        ]));
        assert!(!should_use_embedded_ui(&[
            "语界精读.exe".to_string(),
            "--native".to_string(),
        ]));
        assert!(!should_use_embedded_ui(&[
            "语界精读.exe".to_string(),
            "--no-open".to_string(),
        ]));
    }
}
