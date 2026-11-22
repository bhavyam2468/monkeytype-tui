use rand::Rng;

pub const ENGLISH_200: &[&str] = &[
    "the", "be", "of", "and", "a", "to", "in", "he", "have", "it",
    "that", "for", "they", "i", "with", "as", "not", "on", "she", "at",
    "by", "this", "we", "you", "do", "but", "from", "or", "which", "one",
    "would", "all", "will", "there", "say", "who", "make", "when", "can", "more",
    "if", "no", "man", "out", "other", "so", "what", "time", "up", "go",
    "about", "than", "into", "could", "state", "only", "new", "year", "some", "take",
    "come", "these", "know", "see", "use", "get", "like", "then", "first", "any",
    "work", "now", "may", "such", "give", "over", "think", "most", "even", "find",
    "day", "also", "after", "way", "many", "must", "look", "before", "great", "back",
    "through", "long", "where", "much", "should", "well", "people", "down", "own", "just",
    "because", "good", "each", "those", "feel", "seem", "how", "high", "too", "place",
    "little", "world", "very", "still", "nation", "hand", "old", "life", "tell", "write",
    "become", "here", "show", "house", "both", "between", "need", "mean", "call", "develop",
    "under", "last", "right", "move", "thing", "general", "school", "never", "same", "another",
    "begin", "while", "number", "part", "turn", "real", "leave", "might", "want", "point",
    "form", "off", "child", "few", "small", "since", "against", "ask", "late", "home",
    "interest", "large", "person", "end", "open", "public", "follow", "during", "present", "without",
    "again", "hold", "govern", "around", "possible", "head", "consider", "word", "program", "problem",
    "however", "lead", "system", "set", "order", "eye", "plan", "run", "keep", "face",
    "fact", "group", "play", "stand", "increase", "early", "course", "change", "help", "line",
];

pub const QUOTES: &[(&str, &str)] = &[
    (
        "The only way to do great work is to love what you do. If you haven't found it yet, keep looking. Don't settle.",
        "Steve Jobs",
    ),
    (
        "Simplicity is the soul of efficiency. Clean code always looks like it was written by someone who cares.",
        "Austin Freeman & Robert C. Martin",
    ),
    (
        "Programs must be written for people to read, and only incidentally for machines to execute.",
        "Harold Abelson",
    ),
    (
        "Any fool can write code that a computer can understand. Good programmers write code that humans can understand.",
        "Martin Fowler",
    ),
    (
        "Perfection is achieved, not when there is nothing more to add, but when there is nothing left to take away.",
        "Antoine de Saint-Exupery",
    ),
    (
        "We are what we repeatedly do. Excellence, then, is not an act, but a habit.",
        "Will Durant",
    ),
    (
        "It does not matter how slowly you go as long as you do not stop.",
        "Confucius",
    ),
    (
        "First, solve the problem. Then, write the code.",
        "John Johnson",
    ),
];

const TRAILING_PUNCT: &[char] = &['.', ',', ',', ',', ';', ':', '!', '?'];

pub fn generate_words<R: Rng>(
    count: usize,
    punctuation: bool,
    numbers: bool,
    rng: &mut R,
) -> Vec<String> {
    let mut words = Vec::with_capacity(count);
    let mut capitalize_next = punctuation;

    for i in 0..count {
        let mut word = if numbers && rng.gen_bool(0.12) {
            let digits = rng.gen_range(1..=4);
            let low = if digits == 1 { 0 } else { 10_u32.pow(digits - 1) };
            let high = 10_u32.pow(digits) - 1;
            rng.gen_range(low..=high).to_string()
        } else {
            let raw = ENGLISH_200[rng.gen_range(0..ENGLISH_200.len())];
            if capitalize_next {
                capitalize_next = false;
                capitalize_first(raw)
            } else {
                raw.to_string()
            }
        };

        if punctuation {
            let roll: f64 = rng.gen();
            if i + 1 == count {
                word.push('.');
            } else if roll < 0.14 {
                let p = TRAILING_PUNCT[rng.gen_range(0..TRAILING_PUNCT.len())];
                word.push(p);
                if matches!(p, '.' | '!' | '?') {
                    capitalize_next = true;
                }
            } else if roll < 0.18 && word.len() > 2 {
                word = format!("\"{word}\"");
            }
        }

        words.push(word);
    }

    words
}

pub fn pick_quote<R: Rng>(rng: &mut R) -> (Vec<String>, String) {
    let (text, author) = QUOTES[rng.gen_range(0..QUOTES.len())];
    (
        text.split_whitespace().map(ToString::to_string).collect(),
        author.to_string(),
    )
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}
