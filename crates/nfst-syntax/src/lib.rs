//! Shared diagnostic and source-span vocabulary used by every nfst grammar
//! crate. Kept deliberately small: when the second consumer (`nfst-lexc`,
//! `nfst-pmatch`, …) lands, anything that needs to be shared can graduate into
//! this crate then.

use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceId(pub u32);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Span {
    pub source: SourceId,
    pub range: Range<usize>,
}

impl Span {
    pub const fn new(source: SourceId, range: Range<usize>) -> Self {
        Self { source, range }
    }

    pub const fn anonymous(range: Range<usize>) -> Self {
        Self {
            source: SourceId(0),
            range,
        }
    }

    pub const fn start(&self) -> usize {
        self.range.start
    }

    pub const fn end(&self) -> usize {
        self.range.end
    }

    pub const fn len(&self) -> usize {
        self.range.end - self.range.start
    }

    pub const fn is_empty(&self) -> bool {
        self.range.start == self.range.end
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

/// A value paired with the source span it was derived from.
///
/// Equality is by value only — spans are deliberately excluded so that AST
/// snapshots stay readable. Compare spans explicitly via `.span` when a test
/// cares about source position.
#[derive(Clone, Debug)]
pub struct Spanned<T> {
    pub value: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    pub const fn new(value: T, span: Span) -> Self {
        Self { value, span }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Spanned<U> {
        Spanned {
            value: f(self.value),
            span: self.span,
        }
    }

    pub fn as_ref(&self) -> Spanned<&T> {
        Spanned {
            value: &self.value,
            span: self.span.clone(),
        }
    }

    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<T: PartialEq> PartialEq for Spanned<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T: Eq> Eq for Spanned<T> {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub span: Span,
    pub message: String,
    pub notes: Vec<String>,
}

impl Diagnostic {
    pub fn error(span: Span, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            span,
            message: message.into(),
            notes: Vec::new(),
        }
    }

    pub fn warning(span: Span, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            span,
            message: message.into(),
            notes: Vec::new(),
        }
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_basics() {
        let s = Span::anonymous(3..7);
        assert_eq!(s.start(), 3);
        assert_eq!(s.end(), 7);
        assert_eq!(s.len(), 4);
        assert!(!s.is_empty());
    }

    #[test]
    fn diagnostic_builder() {
        let d =
            Diagnostic::error(Span::anonymous(0..1), "bad token").with_note("expected expression");
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.message, "bad token");
        assert_eq!(d.notes, vec!["expected expression".to_string()]);
    }
}
