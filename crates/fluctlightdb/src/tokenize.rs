use std::collections::HashSet;

/// Rich lexical + structural tokens (entorhinal cortex input).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RichToken {
    pub surface: String,
    pub kind: TokenKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    Content,
    Context,
    Outcome,
    Bigram,
    Trigram,
    Structural,
}

const STOP: &[&str] = &[
    "the", "a", "an", "and", "or", "to", "of", "in", "on", "at", "is", "was", "it", "for", "with",
];

/// Multi-layer tokenization: unigrams + n-grams + role prefixes (EC layer input).
pub fn tokenize_rich(content: &str, context: &str, outcome: Option<&str>) -> Vec<RichToken> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    let mut push = |surface: String, kind: TokenKind| {
        if surface.len() < 2 || STOP.contains(&surface.as_str()) {
            return;
        }
        if seen.insert(surface.clone()) {
            out.push(RichToken { surface, kind });
        }
    };

    for w in words(content) {
        push(format!("c:{w}"), TokenKind::Content);
    }
    for w in words(context) {
        push(format!("x:{w}"), TokenKind::Context);
    }
    if let Some(o) = outcome {
        for w in words(o) {
            push(format!("o:{w}"), TokenKind::Outcome);
        }
    }

    let content_words = words(content);
    for pair in content_words.windows(2) {
        push(
            format!("bg:c:{}_{}", pair[0], pair[1]),
            TokenKind::Bigram,
        );
    }
    for tri in content_words.windows(3) {
        push(
            format!("tg:c:{}_{}_{}", tri[0], tri[1], tri[2]),
            TokenKind::Trigram,
        );
    }

    push(
        format!("ctx@{}", slug(context)),
        TokenKind::Structural,
    );
    push(
        format!("sum@{}", slug(&content.chars().take(48).collect::<String>())),
        TokenKind::Structural,
    );

    out
}

pub fn tokenize(text: &str) -> Vec<String> {
    words(text)
}

fn slug(s: &str) -> String {
    words(s).into_iter().take(6).collect::<Vec<_>>().join("_")
}

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 2)
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rich_tokens_include_roles_and_ngrams() {
        let t = tokenize_rich(
            "agent completed task successfully",
            "workspace",
            Some("saved"),
        );
        assert!(t.iter().any(|x| x.surface.starts_with("c:agent")));
        assert!(t.iter().any(|x| x.surface.starts_with("x:workspace")));
        assert!(t.iter().any(|x| x.surface.starts_with("o:saved")));
        assert!(t.iter().any(|x| x.kind == TokenKind::Bigram));
    }
}
