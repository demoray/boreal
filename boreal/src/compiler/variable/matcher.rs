use std::ops::Range;

use super::AcMatchStatus;

pub mod raw;
pub mod validator;
mod widener;

#[derive(Debug)]
pub(crate) struct Matcher {
    /// Set of literals extracted from the variable.
    ///
    /// Will be used by the AC pass to scan for the variable.
    ///
    /// If both ascii and wide are set in the flags, it is expected that the literals are
    /// composed of:
    /// - the ascii literals first
    /// - the wide literals second
    ///
    /// This is required in order to know which kind is a literal match, as checking its value
    /// would be buggy.
    pub literals: Vec<Vec<u8>>,

    /// Flags related to variable modifiers.
    pub flags: Flags,

    pub kind: MatcherKind,
}

#[derive(Copy, Clone, Debug)]
pub struct Flags {
    pub fullword: bool,
    pub ascii: bool,
    pub wide: bool,
    pub nocase: bool,
}

#[derive(Debug)]
pub(crate) enum MatcherKind {
    /// The literals cover entirely the variable.
    Literals,
    /// The regex can confirm matches from AC literal matches.
    Atomized { validator: validator::Validator },

    /// The regex cannot confirm matches from AC literal matches.
    Raw(raw::RawMatcher),
}

/// Type of a match.
#[derive(Copy, Clone, Debug)]
pub enum MatchType {
    /// The match is on ascii literals.
    Ascii,

    /// The match is on the wide literals for an wide only variable.
    WideStandard,

    /// The match is on wide versions of the literals for an ascii and wide variable.
    WideAlternate,
}

impl MatchType {
    pub fn is_wide(self) -> bool {
        match self {
            MatchType::Ascii => false,
            MatchType::WideStandard | MatchType::WideAlternate => true,
        }
    }
}

#[derive(Debug)]
pub enum Matches {
    /// The literal yields multiple matches (can be empty).
    Multiple(Vec<Range<usize>>),

    /// The literal yields a single match (None if invalid).
    ///
    /// This is an optim to avoid allocating a Vec for the very common case of returning a
    /// single match.
    Single(Range<usize>),

    /// The literal does not give any match.
    None,
}

impl Matcher {
    /// Confirm that an AC match is a match on the given literal.
    ///
    /// This is needed because the AC might optimize literals and get false positive matches.
    /// This function is used to confirm the tentative match does match the literal with the given
    /// index.
    pub fn confirm_ac_literal(
        &self,
        mem: &[u8],
        mat: &Range<usize>,
        literal_index: usize,
    ) -> Option<MatchType> {
        let literal = &self.literals[literal_index];

        if self.flags.nocase {
            if !literal.eq_ignore_ascii_case(&mem[mat.start..mat.end]) {
                return None;
            }
        } else if literal != &mem[mat.start..mat.end] {
            return None;
        }

        match (self.flags.ascii, self.flags.wide) {
            (false, true) => Some(MatchType::WideStandard),
            // If the variable has both ascii and wide, then the ascii literals are in the first
            // half, and the wide ones in the second half.
            (true, true) if literal_index >= self.literals.len() / 2 => {
                Some(MatchType::WideAlternate)
            }
            _ => Some(MatchType::Ascii),
        }
    }

    pub fn process_ac_match(
        &self,
        mem: &[u8],
        mat: Range<usize>,
        start_position: usize,
        match_type: MatchType,
    ) -> AcMatchStatus {
        match &self.kind {
            MatcherKind::Literals => {
                if self.validate_fullword(mem, &mat, match_type) {
                    AcMatchStatus::Single(mat)
                } else {
                    AcMatchStatus::None
                }
            }
            MatcherKind::Atomized { validator } => {
                match validator.validate_match(mem, mat, start_position, match_type) {
                    Matches::None => AcMatchStatus::None,
                    Matches::Single(m) => {
                        if self.validate_fullword(mem, &m, match_type) {
                            AcMatchStatus::Single(m)
                        } else {
                            AcMatchStatus::None
                        }
                    }
                    Matches::Multiple(ms) => AcMatchStatus::Multiple(
                        ms.into_iter()
                            .filter(|m| self.validate_fullword(mem, m, match_type))
                            .collect(),
                    ),
                }
            }
            MatcherKind::Raw(_) => AcMatchStatus::Unknown,
        }
    }

    pub fn find_next_match_at(&self, mem: &[u8], mut offset: usize) -> Option<Range<usize>> {
        let regex = match &self.kind {
            MatcherKind::Raw(r) => r,
            _ => {
                // This variable should have been covered by the AC pass, so we should
                // not be able to reach this code.
                debug_assert!(false);
                return None;
            }
        };

        while offset < mem.len() {
            let (mat, match_type) = regex.find_next_match_at(mem, offset, self.flags)?;

            if self.validate_fullword(mem, &mat, match_type) {
                return Some(mat);
            }

            offset = mat.start + 1;
        }
        None
    }

    fn validate_fullword(&self, mem: &[u8], mat: &Range<usize>, match_type: MatchType) -> bool {
        !self.flags.fullword || check_fullword(mem, mat, match_type)
    }
}

/// Check the match respects a possible fullword modifier for the variable.
fn check_fullword(mem: &[u8], mat: &Range<usize>, match_type: MatchType) -> bool {
    if match_type.is_wide() {
        if mat.start > 1
            && mem[mat.start - 1] == b'\0'
            && mem[mat.start - 2].is_ascii_alphanumeric()
        {
            return false;
        }
        if mat.end + 1 < mem.len()
            && mem[mat.end].is_ascii_alphanumeric()
            && mem[mat.end + 1] == b'\0'
        {
            return false;
        }
    } else {
        if mat.start > 0 && mem[mat.start - 1].is_ascii_alphanumeric() {
            return false;
        }
        if mat.end < mem.len() && mem[mat.end].is_ascii_alphanumeric() {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{test_type_traits, test_type_traits_non_clonable};

    #[test]
    fn test_types_traits() {
        test_type_traits_non_clonable(Matcher {
            literals: vec![],
            flags: Flags {
                fullword: false,
                ascii: false,
                wide: false,
                nocase: false,
            },
            kind: MatcherKind::Literals,
        });
        test_type_traits_non_clonable(MatcherKind::Literals);
        test_type_traits(Flags {
            fullword: false,
            ascii: false,
            wide: false,
            nocase: false,
        });
        test_type_traits(MatchType::Ascii);
        test_type_traits_non_clonable(Matches::None);
    }
}
