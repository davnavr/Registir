//! Provides functions for parsing SAILAR assembly.

use crate::ast;
use crate::lexer::{self, Token};
use std::iter::Iterator;
use std::ops::Range;

#[derive(Clone, Debug, thiserror::Error)]
pub enum ErrorKind {
    #[error("unknown token")]
    UnknownToken,
    #[error("{0} is not a valid format version kind")]
    InvalidFormatVersionKind(String),
    #[error("expected format version kind")]
    ExpectedFormatVersionKind,
    #[error("expected integer format version")]
    ExpectedFormatVersion,
    #[error("invalid format version: {0}")]
    InvalidFormatVersion(std::num::ParseIntError),
}

#[derive(Clone, Debug, thiserror::Error)]
#[error("{kind}")]
pub struct Error {
    kind: Box<ErrorKind>,
    location: ast::LocationRange,
}

impl Error {
    pub fn new<K: Into<ErrorKind>, L: Into<ast::LocationRange>>(kind: K, location: L) -> Self {
        Self {
            kind: Box::new(kind.into()),
            location: location.into(),
        }
    }

    #[inline]
    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }

    #[inline]
    pub fn location(&self) -> &ast::LocationRange {
        &self.location
    }
}

struct Input<'o, 's, S: Iterator> {
    source: std::iter::Peekable<S>,
    locations: &'o lexer::OffsetMap<'s>,
}

type SliceInput<'o, 's> = Input<'o, 's, std::slice::Iter<'o, (Token<'s>, Range<usize>)>>;

type LocatedToken<'o, 's> = (&'o Token<'s>, ast::LocationRange);

impl<'o, 's, S: Iterator<Item = &'o (Token<'s>, Range<usize>)>> Input<'o, 's, S> {
    fn new<I: std::iter::IntoIterator<IntoIter = S>>(source: I, locations: &'o lexer::OffsetMap<'s>) -> Self {
        Self {
            source: source.into_iter().peekable(),
            locations,
        }
    }

    fn token_from_offsets(token: Option<&S::Item>, locations: &'o lexer::OffsetMap<'s>) -> Option<LocatedToken<'o, 's>> {
        token.map(|(token, offsets)| {
            (
                token,
                ast::LocationRange::new(
                    locations.get_location(offsets.start).unwrap(),
                    locations.get_location(offsets.end).unwrap(),
                ),
            )
        })
    }

    fn next_token(&mut self) -> Option<LocatedToken<'o, 's>> {
        Self::token_from_offsets(self.source.next().as_ref(), self.locations)
    }

    fn next_token_if<F: FnOnce(&'o Token<'s>) -> bool>(&mut self, condition: F) -> Option<LocatedToken<'o, 's>> {
        Self::token_from_offsets(self.source.next_if(|(token, _)| condition(token)).as_ref(), self.locations)
    }

    fn peek_next_token(&mut self) -> Option<LocatedToken<'o, 's>> {
        Self::token_from_offsets(self.source.peek(), self.locations)
    }
}

#[derive(Debug)]
pub struct Output<'s> {
    tree: Vec<ast::Located<ast::Directive<'s>>>,
    errors: Vec<Error>,
}

impl<'s> Output<'s> {
    #[inline]
    pub fn tree(&self) -> &[ast::Located<ast::Directive<'s>>] {
        &self.tree
    }

    #[inline]
    pub fn errors(&self) -> &[Error] {
        &self.errors
    }
}
macro_rules! push_error {
    ($errors: expr, $kind: expr, $location: expr) => {
        $errors.push(Error::new($kind, $location))
    };
}

macro_rules! fail_continue {
    ($errors: expr, $kind: expr, $location: expr) => {{
        push_error!($errors, $kind, $location);
        continue;
    }};
}

macro_rules! fail_skip_line {
    ($errors: expr, $kind: expr, $location: expr, $input: expr) => {{
        push_error!($errors, $kind, $location);

        loop {
            match $input.next_token() {
                Some((Token::Newline, _)) | None => break,
                Some(_) => (),
            }
        }

        continue;
    }};
}

macro_rules! match_exhausted {
    ($errors: expr, $kind: expr, $token: expr, $input: expr) => {
        fail_continue!(
            $errors,
            $kind,
            if let Some((_, location)) = $token {
                location
            } else {
                $input.locations.get_last().unwrap().into()
            }
        )
    };
}

fn expect_new_line_or_end(errors: &mut Vec<Error>, input: &mut SliceInput) {
    //match input.
}

/// Transfers a sequence of tokens into an abstract syntax tree.
pub fn parse<'s>(input: &lexer::Output<'s>) -> Output<'s> {
    let mut input = SliceInput::new(input.tokens(), input.locations());
    let mut tree = Vec::default();
    let mut errors = Vec::default();

    while let Some((token, location)) = input.next_token() {
        let start_location = location.start().clone();
        let end_location;

        match token {
            Token::ArrayDirective => tree.push(ast::Located::new(
                ast::Directive::Array,
                start_location,
                location.end().clone(),
            )),
            Token::FormatDirective => {
                let format_kind = match input.next_token() {
                    Some((Token::Word("major"), _)) => ast::FormatVersionKind::Major,
                    Some((Token::Word("minor"), _)) => ast::FormatVersionKind::Minor,
                    Some((Token::Word(bad), location)) => {
                        fail_skip_line!(errors, ErrorKind::InvalidFormatVersionKind(bad.to_string()), location, input)
                    }
                    bad => match_exhausted!(errors, ErrorKind::ExpectedFormatVersionKind, bad, input),
                };

                let format_version = match input.next_token() {
                    Some((Token::LiteralInteger(digits), location)) => {
                        end_location = location.end().clone();
                        match u8::try_from(digits) {
                            Ok(version) => version,
                            Err(e) => fail_skip_line!(errors, ErrorKind::InvalidFormatVersion(e), location, input),
                        }
                    }
                    bad => match_exhausted!(errors, ErrorKind::ExpectedFormatVersion, bad, input),
                };

                // TODO: Have helper/macro that checks for newline or EOF

                tree.push(ast::Located::new(
                    ast::Directive::Format(format_kind, format_version),
                    start_location,
                    end_location,
                ));
            }
            Token::Unknown => fail_continue!(errors, ErrorKind::UnknownToken, location),
            Token::Newline => (),
            bad => todo!("handle {:?}, {:?}", bad, &errors),
        }
    }

    Output { tree, errors }
}
