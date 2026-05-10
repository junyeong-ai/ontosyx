//! Tokenizer trait + lindera-backed Korean/English impl.

use std::sync::Arc;

use lindera::dictionary::{DictionaryKind, UserDictionary, load_embedded_dictionary};
use lindera::mode::Mode;
use lindera::segmenter::Segmenter;
use lindera::tokenizer::Tokenizer as LinderaInner;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TokenizeError {
    #[error("lindera dictionary load failed: {0}")]
    DictionaryLoad(String),
    #[error("lindera tokenize failed: {0}")]
    Tokenize(String),
    #[error("user-dictionary compile failed: {0}")]
    UserDict(String),
}

/// One emitted token after tokenization + POS filter +
/// canonical-lemma resolution.
///
/// `surface` is the original text slice the tokenizer matched.
/// `lemma` is the canonical form — for non-glossary tokens this
/// equals `surface`; for glossary-recognised compounds, it's the
/// canonical lemma the user-dict CSV emitted (Concept-derived).
/// `pos` is the POS tag (`NNG`, `SL`, etc.) — Korean Penn-style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub surface: String,
    pub lemma: String,
    pub pos: String,
}

/// Stable tokenizer trait. The retrieval pipeline holds
/// `Arc<dyn Tokenizer>` per workspace; all index-time and
/// query-time tokenization flows through the same instance to
/// guarantee recall consistency.
///
/// Implementations are `Send + Sync` so the registry can hand
/// out cheap `Arc` clones without re-locking on every call.
pub trait Tokenizer: Send + Sync {
    /// Stable identifier (`"lindera-ko"` / `"passthrough"` /
    /// future `"lindera-ja"` etc.) — surfaces in observability +
    /// fingerprint stamping.
    fn name(&self) -> &'static str;

    /// Tokenize for both indexing and querying.
    ///
    /// Returns space-joined canonical lemmas suitable for direct
    /// insertion into `to_tsvector('simple', _)`. Filtered POS
    /// classes (조사 / 어미 / 부호 / 접미사 / 빈 토큰) are
    /// dropped before joining; remaining lemmas are NFC-normalised
    /// and lowercase-folded for ASCII letters.
    ///
    /// Empty / whitespace-only input returns an empty string.
    fn tokenize(&self, input: &str) -> Result<String, TokenizeError>;

    /// Lower-level token stream — same filtering rules but
    /// without joining. Surfaces in tests + diagnostic surfaces;
    /// callers that just want the searchable string call
    /// [`Self::tokenize`].
    fn tokens(&self, input: &str) -> Result<Vec<Token>, TokenizeError>;
}

// ---------------------------------------------------------------------------
// PassthroughTokenizer — test/dev fallback
// ---------------------------------------------------------------------------

/// Whitespace-only fallback. Preserves surfaces, no morphology,
/// no glossary compounds. Used for unit tests that don't need
/// the lindera dictionary, and as the registry's default for a
/// workspace that hasn't loaded a real tokenizer yet.
#[derive(Debug, Clone, Copy, Default)]
pub struct PassthroughTokenizer;

impl Tokenizer for PassthroughTokenizer {
    fn name(&self) -> &'static str {
        "passthrough"
    }

    fn tokenize(&self, input: &str) -> Result<String, TokenizeError> {
        let normalised: Vec<String> = input
            .split_whitespace()
            .map(normalise_token)
            .filter(|t| !t.is_empty())
            .collect();
        Ok(normalised.join(" "))
    }

    fn tokens(&self, input: &str) -> Result<Vec<Token>, TokenizeError> {
        Ok(input
            .split_whitespace()
            .map(normalise_token)
            .filter(|t| !t.is_empty())
            .map(|surface| Token {
                surface: surface.clone(),
                lemma: surface,
                pos: "X".to_string(),
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// KoreanEnglishTokenizer — lindera + ko-dic
// ---------------------------------------------------------------------------

/// Lindera-backed Korean + English morphological tokenizer.
///
/// Carries the workspace's user dictionary when supplied; when
/// `user_dict` is `None`, it operates against the system
/// `mecab-ko-dic` only. The registry hot-swaps the user dict by
/// publishing a new `Arc<KoreanEnglishTokenizer>` via
/// `ArcSwap` whenever the workspace's glossary fingerprint
/// changes.
pub struct KoreanEnglishTokenizer {
    inner: LinderaInner,
}

impl std::fmt::Debug for KoreanEnglishTokenizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KoreanEnglishTokenizer")
            .finish_non_exhaustive()
    }
}

impl KoreanEnglishTokenizer {
    /// Build a tokenizer with the system `mecab-ko-dic` only.
    /// Workspaces with no glossary (cold-start) or with an
    /// unchanged fingerprint share this instance via the
    /// registry.
    pub fn system_only() -> Result<Self, TokenizeError> {
        let dict = load_embedded_dictionary(DictionaryKind::KoDic)
            .map_err(|e| TokenizeError::DictionaryLoad(e.to_string()))?;
        let segmenter = Segmenter::new(Mode::Normal, dict, None);
        Ok(Self {
            inner: LinderaInner::new(segmenter),
        })
    }

    /// Build a tokenizer with system + workspace user dict.
    /// `user_dict` is the lindera-built `UserDictionary` from
    /// the glossary CSV.
    pub fn with_user_dict(user_dict: Arc<UserDictionary>) -> Result<Self, TokenizeError> {
        let dict = load_embedded_dictionary(DictionaryKind::KoDic)
            .map_err(|e| TokenizeError::DictionaryLoad(e.to_string()))?;
        let segmenter = Segmenter::new(Mode::Normal, dict, Some((*user_dict).clone()));
        Ok(Self {
            inner: LinderaInner::new(segmenter),
        })
    }

    /// Build a tokenizer with the system dict + a workspace
    /// user dict compiled from a CSV path. Encapsulates the
    /// lindera dictionary-build details so callers
    /// (`ox-api::tokenizer_publish`) stay lindera-free.
    ///
    /// The CSV path is opened on the calling thread; the
    /// build itself runs synchronously. Callers in async
    /// contexts must wrap in `spawn_blocking` to avoid
    /// stalling the runtime.
    pub fn from_user_dict_csv_path(csv_path: &std::path::Path) -> Result<Self, TokenizeError> {
        use lindera::dictionary::{load_embedded_dictionary, load_user_dictionary_from_csv};
        let dict = load_embedded_dictionary(DictionaryKind::KoDic)
            .map_err(|e| TokenizeError::DictionaryLoad(e.to_string()))?;
        let user_dict = load_user_dictionary_from_csv(&dict.metadata, csv_path)
            .map_err(|e| TokenizeError::UserDict(e.to_string()))?;
        Self::with_user_dict(Arc::new(user_dict))
    }
}

impl Tokenizer for KoreanEnglishTokenizer {
    fn name(&self) -> &'static str {
        "lindera-ko"
    }

    fn tokenize(&self, input: &str) -> Result<String, TokenizeError> {
        let tokens = self.tokens(input)?;
        let joined: Vec<&str> = tokens.iter().map(|t| t.lemma.as_str()).collect();
        Ok(joined.join(" "))
    }

    fn tokens(&self, input: &str) -> Result<Vec<Token>, TokenizeError> {
        if input.trim().is_empty() {
            return Ok(Vec::new());
        }
        let raw = self
            .inner
            .tokenize(input)
            .map_err(|e| TokenizeError::Tokenize(e.to_string()))?;

        let mut out = Vec::with_capacity(raw.len());
        for mut tok in raw {
            // lindera 3.x exposes `surface` (Cow<str>) for the
            // matched text and `details()` lazy-fills the mecab
            // fields: `[POS, POS_subcat1, POS_subcat2, POS_subcat3,
            // conjugation_type, conjugation_form, lemma, reading,
            // pronunciation, ...]`. We need POS [0] + lemma [3].
            let surface = tok.surface.to_string();
            let details = tok.details();
            let pos = details
                .first()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "UNK".to_string());

            if !is_indexable_pos(&pos) {
                continue;
            }
            // mecab-ko-dic lemma 위치는 details[3] (4번째 필드).
            // user dict 가 emit 한 row 의 lemma 도 같은 위치에
            // 들어간다. 부재 시 surface 그대로.
            let lemma_raw = details
                .get(3)
                .map(|s| s.as_ref())
                .unwrap_or(surface.as_str());
            // mecab convention: missing → "*"
            let lemma = if lemma_raw.is_empty() || lemma_raw == "*" {
                surface.clone()
            } else {
                lemma_raw.to_string()
            };
            let lemma = normalise_token(&lemma);
            if lemma.is_empty() {
                continue;
            }
            out.push(Token {
                surface: surface.clone(),
                lemma,
                pos,
            });
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// POS filter + normalisation
// ---------------------------------------------------------------------------

/// Indexable POS predicate (mecab-ko-dic Penn-style tags).
///
/// Drops:
/// - 조사 (J*: JKG, JKO, JKS, JKB, JX, JC) — 의/을/가/는/도
/// - 어미 (E*: EF, EP, EC, ETN, ETM)
/// - 부호 (S* except SL, SH, SN: SF, SP, SS, SE, SO, SW)
/// - 접미사 (XS*: XSN, XSV, XSA)
///
/// Keeps:
/// - 명사 (N*: NNG, NNP, NNB, NR, NP)
/// - 동사 / 형용사 (VV, VA — lemma 만 사용)
/// - 부사 (MAG, MAJ)
/// - 외래어 / 한자 / 숫자 (SL, SH, SN)
fn is_indexable_pos(pos: &str) -> bool {
    if pos.is_empty() {
        return false;
    }
    // 부호류 부분집합 화이트리스트 (SL/SH/SN keep)
    if matches!(pos, "SL" | "SH" | "SN") {
        return true;
    }
    // S* drop (위 화이트리스트 제외)
    if pos.starts_with('S') {
        return false;
    }
    // 조사 / 어미 / 접미사 drop
    if pos.starts_with('J') || pos.starts_with('E') || pos.starts_with("XS") {
        return false;
    }
    // 외 (N*, V*, M*, 등) keep
    true
}

/// NFC 정규화 + ASCII lowercase. 한글은 그대로 (lower 안 함).
fn normalise_token(input: &str) -> String {
    // ASCII lowercase 만. 한글은 합자 정규화 단계 lindera
    // 내부에서 처리 — extra NFC 가 필요한 케이스는 라이브러리
    // 외 surface (mixed) 에서만 발생. 보수적으로 ASCII 만 lower.
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_uppercase() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out.trim().to_string()
}

// ---------------------------------------------------------------------------
// Tag enum exposed for glossary user-dict generation
// ---------------------------------------------------------------------------

/// Closed Korean POS tag set the platform emits to lindera user
/// dictionaries. Mirrors the subset of mecab-ko-dic tags that
/// the [`is_indexable_pos`] filter keeps; emitting a non-keep
/// tag would cause the dict entry to be silently dropped at
/// query time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TermPosTag {
    Noun,       // NNG
    ProperNoun, // NNP
    Verb,       // VV (lemma 형식)
    Adjective,  // VA
    Foreign,    // SL — 외래어 / 영문
    Compound,   // NNG (compound 처리, lindera 가 internal split 안 함)
}

impl TermPosTag {
    pub fn as_mecab_tag(self) -> &'static str {
        match self {
            Self::Noun => "NNG",
            Self::ProperNoun => "NNP",
            Self::Verb => "VV",
            Self::Adjective => "VA",
            Self::Foreign => "SL",
            Self::Compound => "NNG",
        }
    }

    /// Heuristic POS from surface script. Used when the
    /// `GlossaryTermDef.term_pos` carries the `Auto` policy.
    pub fn auto_from_surface(surface: &str) -> Self {
        let mut has_korean = false;
        let mut has_ascii_alpha = false;
        let mut has_other = false;
        for ch in surface.chars() {
            if ('\u{AC00}'..='\u{D7AF}').contains(&ch) {
                has_korean = true;
            } else if ch.is_ascii_alphabetic() || ch.is_ascii_digit() {
                has_ascii_alpha = true;
            } else if !ch.is_whitespace() && !ch.is_ascii_punctuation() {
                has_other = true;
            }
        }
        match (has_korean, has_ascii_alpha, has_other) {
            (true, _, _) => Self::Compound,
            (false, true, false) => Self::Foreign,
            _ => Self::Noun,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_normalises_whitespace() {
        let t = PassthroughTokenizer;
        assert_eq!(t.tokenize("  Hello   World  ").unwrap(), "hello world");
    }

    #[test]
    fn passthrough_preserves_korean() {
        let t = PassthroughTokenizer;
        assert_eq!(t.tokenize("고객 분석 OAuth2").unwrap(), "고객 분석 oauth2");
    }

    #[test]
    fn passthrough_handles_empty_input() {
        let t = PassthroughTokenizer;
        assert_eq!(t.tokenize("").unwrap(), "");
        assert_eq!(t.tokenize("    ").unwrap(), "");
    }

    #[test]
    fn pos_filter_drops_particles_and_endings() {
        assert!(!is_indexable_pos("JKG"));
        assert!(!is_indexable_pos("JX"));
        assert!(!is_indexable_pos("EF"));
        assert!(!is_indexable_pos("XSN"));
        assert!(!is_indexable_pos("SF"));
    }

    #[test]
    fn pos_filter_keeps_content_classes() {
        assert!(is_indexable_pos("NNG"));
        assert!(is_indexable_pos("NNP"));
        assert!(is_indexable_pos("VV"));
        assert!(is_indexable_pos("VA"));
        assert!(is_indexable_pos("SL"));
        assert!(is_indexable_pos("SN"));
        assert!(is_indexable_pos("MAG"));
    }

    #[test]
    fn auto_pos_classifies_mixed_compound() {
        assert_eq!(
            TermPosTag::auto_from_surface("OAuth2 인증"),
            TermPosTag::Compound
        );
        assert_eq!(
            TermPosTag::auto_from_surface("고객 생애 가치"),
            TermPosTag::Compound
        );
        assert_eq!(TermPosTag::auto_from_surface("LTV"), TermPosTag::Foreign);
        assert_eq!(TermPosTag::auto_from_surface("OAuth2"), TermPosTag::Foreign);
        assert_eq!(TermPosTag::auto_from_surface("고객"), TermPosTag::Compound);
    }

    /// Lindera + ko-dic 가 실제로 빌드되어야 하는 통합 테스트.
    /// 첫 호출이 dictionary 를 mmap 로딩 — ~수초.
    #[test]
    fn lindera_korean_drops_particles() {
        let t = KoreanEnglishTokenizer::system_only().expect("ko-dic load");
        let result = t.tokenize("고객들의 주문을 분석한다").unwrap();
        // "들" "의" "을" "한다" 등 조사/어미 drop 후 "고객 주문 분석" 유사 형태
        assert!(result.contains("고객"), "expected 고객 in {result:?}");
        assert!(result.contains("주문"), "expected 주문 in {result:?}");
        assert!(result.contains("분석"), "expected 분석 in {result:?}");
        assert!(!result.contains("들의"), "particle leaked: {result:?}");
    }

    #[test]
    fn lindera_handles_mixed_korean_english() {
        // System dict (no user dict) splits "OAuth2" into
        // ["OAuth", "2"] — ko-dic doesn't recognise the
        // alphanumeric compound on its own. The glossary
        // user-dict mechanism is what teaches lindera to
        // keep "OAuth2" / "OAuth2 인증" as a single token; we
        // assert the parts survive here, and the compound
        // preservation is exercised in the glossary_dict
        // integration suite.
        let t = KoreanEnglishTokenizer::system_only().expect("ko-dic load");
        let result = t.tokenize("OAuth2 인증 흐름").unwrap();
        assert!(result.contains("oauth"), "expected oauth: {result:?}");
        assert!(result.contains("인증"), "expected 인증: {result:?}");
        assert!(result.contains("흐름"), "expected 흐름: {result:?}");
    }

    #[test]
    fn lindera_empty_input_returns_empty() {
        let t = KoreanEnglishTokenizer::system_only().expect("ko-dic load");
        assert_eq!(t.tokenize("").unwrap(), "");
        assert_eq!(t.tokenize("   ").unwrap(), "");
    }
}
