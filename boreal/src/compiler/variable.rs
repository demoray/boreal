use std::ops::Range;

use ::regex::bytes::{Regex, RegexBuilder};

use boreal_parser::{VariableDeclaration, VariableDeclarationValue};
use boreal_parser::{VariableFlags, VariableModifiers};

use super::base64::encode_base64;
use super::CompilationError;

mod atom;
pub use atom::atom_rank;
mod hex_string;
mod regex;

/// A compiled variable used in a rule.
#[derive(Debug)]
pub struct Variable {
    /// Name of the variable, without the '$'.
    ///
    /// Anonymous variables are just named "".
    pub name: String,

    /// Is the variable marked as private.
    pub is_private: bool,

    /// Set of literals extracted from the variable.
    ///
    /// Will be used by the AC pass to scan for the variable.
    pub literals: Vec<Vec<u8>>,

    /// Flags related to variable modifiers.
    flags: VariableFlags,

    /// Type of matching for the variable.
    matcher_type: MatcherType,

    /// Regex of the non wide version of the regex.
    ///
    /// This is only set for the specific case of a regex variable, with a wide modifier, that
    /// contains word boundaries.
    /// In this case, the regex expression cannot be "widened", and this regex is used to post
    /// check matches.
    non_wide_regex: Option<Regex>,
}

#[derive(Debug)]
enum MatcherType {
    /// The literals cover entirely the variable.
    Literals,
    /// The regex can confirm matches from AC literal matches.
    Atomized {
        left_validator: Option<Regex>,
        right_validator: Option<Regex>,
    },

    /// The regex cannot confirm matches from AC literal matches.
    Raw(Regex),
}

/// State of an aho-corasick match on a [`Matcher`] literals.
#[derive(Clone, Debug)]
pub enum AcMatchStatus {
    /// The literal yields multiple matches (can be empty).
    Multiple(Vec<Range<usize>>),

    /// The literal yields a single match (None if invalid).
    ///
    /// This is an optim to avoid allocating a Vec for the very common case of returning a
    /// single match.
    Single(Range<usize>),

    /// The literal does not give any match.
    None,

    /// Unknown status for the match, will need to be confirmed on its own.
    Unknown,
}

pub(crate) fn compile_variable(decl: VariableDeclaration) -> Result<Variable, CompilationError> {
    let VariableDeclaration {
        name,
        value,
        mut modifiers,
        span,
    } = decl;

    if !modifiers.flags.contains(VariableFlags::WIDE) {
        modifiers.flags.insert(VariableFlags::ASCII);
    }

    let CompiledVariable {
        literals,
        matcher_type,
        non_wide_regex,
    } = match value {
        VariableDeclarationValue::Bytes(s) => Ok(compile_bytes(s, &modifiers)),
        VariableDeclarationValue::Regex(boreal_parser::Regex {
            ast,
            case_insensitive,
            dot_all,
            span: _,
        }) => {
            if case_insensitive {
                modifiers.flags.insert(VariableFlags::NOCASE);
            }
            regex::compile_regex(&ast, case_insensitive, dot_all, modifiers.flags)
        }
        VariableDeclarationValue::HexString(hex_string) => {
            // Fullword and wide is not compatible with hex strings
            modifiers.flags.remove(VariableFlags::FULLWORD);
            modifiers.flags.remove(VariableFlags::WIDE);

            if hex_string::can_use_only_literals(&hex_string) {
                Ok(CompiledVariable {
                    literals: hex_string::hex_string_to_only_literals(hex_string),
                    matcher_type: MatcherType::Literals,
                    non_wide_regex: None,
                })
            } else {
                let ast = hex_string::hex_string_to_ast(hex_string);
                regex::compile_regex(&ast, false, true, modifiers.flags)
            }
        }
    }
    .map_err(|error| CompilationError::VariableCompilation {
        variable_name: name.clone(),
        span: span.clone(),
        error,
    })?;

    Ok(Variable {
        name,
        is_private: modifiers.flags.contains(VariableFlags::PRIVATE),
        literals,
        flags: modifiers.flags,
        matcher_type,
        non_wide_regex,
    })
}

struct CompiledVariable {
    literals: Vec<Vec<u8>>,
    matcher_type: MatcherType,
    non_wide_regex: Option<Regex>,
}

fn compile_regex_expr(
    expr: &str,
    case_insensitive: bool,
    dot_all: bool,
) -> Result<Regex, VariableCompilationError> {
    RegexBuilder::new(expr)
        .unicode(false)
        .octal(false)
        .multi_line(false)
        .case_insensitive(case_insensitive)
        .dot_matches_new_line(dot_all)
        .build()
        .map_err(|err| VariableCompilationError::Regex(err.to_string()))
}

fn compile_bytes(value: Vec<u8>, modifiers: &VariableModifiers) -> CompiledVariable {
    let mut literals = Vec::with_capacity(2);

    if modifiers.flags.contains(VariableFlags::WIDE) {
        if modifiers.flags.contains(VariableFlags::ASCII) {
            literals.push(string_to_wide(&value));
            literals.push(value);
        } else {
            literals.push(string_to_wide(&value));
        }
    } else {
        literals.push(value);
    }

    if modifiers.flags.contains(VariableFlags::XOR) {
        // For each literal, for each byte in the xor range, build a new literal
        let xor_range = modifiers.xor_range.0..=modifiers.xor_range.1;
        let xor_range_len = xor_range.len(); // modifiers.xor_range.1.saturating_sub(modifiers.xor_range.0) + 1;
        let mut new_literals: Vec<Vec<u8>> = Vec::with_capacity(literals.len() * xor_range_len);
        for lit in literals {
            for xor_byte in xor_range.clone() {
                new_literals.push(lit.iter().map(|c| c ^ xor_byte).collect());
            }
        }
        return CompiledVariable {
            literals: new_literals,
            matcher_type: MatcherType::Literals,
            non_wide_regex: None,
        };
    }

    if modifiers.flags.contains(VariableFlags::BASE64)
        || modifiers.flags.contains(VariableFlags::BASE64WIDE)
    {
        let mut old_literals = Vec::with_capacity(literals.len() * 3);
        std::mem::swap(&mut old_literals, &mut literals);

        if modifiers.flags.contains(VariableFlags::BASE64) {
            for lit in &old_literals {
                for offset in 0..=2 {
                    if let Some(lit) = encode_base64(lit, &modifiers.base64_alphabet, offset) {
                        if modifiers.flags.contains(VariableFlags::BASE64WIDE) {
                            literals.push(string_to_wide(&lit));
                        }
                        literals.push(lit);
                    }
                }
            }
        } else if modifiers.flags.contains(VariableFlags::BASE64WIDE) {
            for lit in &old_literals {
                for offset in 0..=2 {
                    if let Some(lit) = encode_base64(lit, &modifiers.base64_alphabet, offset) {
                        literals.push(string_to_wide(&lit));
                    }
                }
            }
        }
    }

    CompiledVariable {
        literals,
        matcher_type: MatcherType::Literals,
        non_wide_regex: None,
    }
}

impl Variable {
    /// Confirm that an AC match is a match on the given literal.
    ///
    /// This is needed because the AC might optimize literals and get false positive matches.
    /// This function is used to confirm the tentative match does match the literal with the given
    /// index.
    pub fn confirm_ac_literal(&self, mem: &[u8], mat: &Range<usize>, literal_index: usize) -> bool {
        let literal = &self.literals[literal_index];

        if self.flags.contains(VariableFlags::NOCASE) {
            if !literal.eq_ignore_ascii_case(&mem[mat.start..mat.end]) {
                return false;
            }
        } else if literal != &mem[mat.start..mat.end] {
            return false;
        }

        true
    }

    pub fn process_ac_match(
        &self,
        mem: &[u8],
        mat: Range<usize>,
        mut start_position: usize,
    ) -> AcMatchStatus {
        match &self.matcher_type {
            MatcherType::Literals => match self.validate_and_update_match(mem, mat) {
                Some(m) => AcMatchStatus::Single(m),
                None => AcMatchStatus::None,
            },
            MatcherType::Atomized {
                left_validator,
                right_validator,
            } => {
                let end = match right_validator {
                    Some(validator) => match validator.find(&mem[mat.start..]) {
                        Some(m) => mat.start + m.end(),
                        None => return AcMatchStatus::None,
                    },
                    None => mat.end,
                };

                match left_validator {
                    None => {
                        let mat = mat.start..end;
                        match self.validate_and_update_match(mem, mat) {
                            Some(m) => AcMatchStatus::Single(m),
                            None => AcMatchStatus::None,
                        }
                    }
                    Some(validator) => {
                        // The left validator can yield multiple matches.
                        // For example, `a.?bb`, with the `bb` atom, can match as many times as there are
                        // 'a' characters before the `bb` atom.
                        //
                        // XXX: This only works if the left validator does not contain any greedy repetitions!
                        let mut matches = Vec::new();
                        while let Some(m) = validator.find(&mem[start_position..mat.end]) {
                            let m = (m.start() + start_position)..end;
                            start_position = m.start + 1;
                            if let Some(m) = self.validate_and_update_match(mem, m) {
                                matches.push(m);
                            }
                        }
                        AcMatchStatus::Multiple(matches)
                    }
                }
            }
            MatcherType::Raw(_) => AcMatchStatus::Unknown,
        }
    }

    pub fn find_next_match_at(&self, mem: &[u8], mut offset: usize) -> Option<Range<usize>> {
        let regex = match &self.matcher_type {
            MatcherType::Raw(r) => r,
            _ => {
                // This variable should have been covered by the variable set, so we should
                // not be able to reach this code.
                debug_assert!(false);
                return None;
            }
        };

        while offset < mem.len() {
            let mat = regex.find_at(mem, offset).map(|m| m.range())?;

            match self.validate_and_update_match(mem, mat.clone()) {
                Some(m) => return Some(m),
                None => {
                    offset = mat.start + 1;
                }
            }
        }
        None
    }

    fn validate_and_update_match(&self, mem: &[u8], mat: Range<usize>) -> Option<Range<usize>> {
        if self.flags.contains(VariableFlags::FULLWORD) && !check_fullword(mem, &mat, self.flags) {
            return None;
        }

        match self.non_wide_regex.as_ref() {
            Some(regex) => apply_wide_word_boundaries(mat, mem, regex),
            None => Some(mat),
        }
    }
}

/// Check the match respects a possible fullword modifier for the variable.
fn check_fullword(mem: &[u8], mat: &Range<usize>, flags: VariableFlags) -> bool {
    // TODO: We need to know if the match is done on an ascii or wide string to properly check for
    // fullword constraints. This is done in a very ugly way, by going through the match.
    // A better way would be to know which alternation in the match was found.
    let mut match_is_wide = false;

    if flags.contains(VariableFlags::WIDE) {
        match_is_wide = is_match_wide(mat, mem);
        if match_is_wide {
            if mat.start > 1 && mem[mat.start - 1] == b'\0' && is_ascii_alnum(mem[mat.start - 2]) {
                return false;
            }
            if mat.end + 1 < mem.len() && is_ascii_alnum(mem[mat.end]) && mem[mat.end + 1] == b'\0'
            {
                return false;
            }
        }
    }
    if flags.contains(VariableFlags::ASCII) && !match_is_wide {
        if mat.start > 0 && is_ascii_alnum(mem[mat.start - 1]) {
            return false;
        }
        if mat.end < mem.len() && is_ascii_alnum(mem[mat.end]) {
            return false;
        }
    }

    true
}

/// Check the match respects the word boundaries inside the variable.
fn apply_wide_word_boundaries(
    mut mat: Range<usize>,
    mem: &[u8],
    regex: &Regex,
) -> Option<Range<usize>> {
    // The match can be on a non wide regex, if the variable was both ascii and wide. Make sure
    // the match is wide.
    if !is_match_wide(&mat, mem) {
        return Some(mat);
    }

    // Take the previous and next byte, so that word boundaries placed at the beginning or end of
    // the regex can be checked.
    // Note that we must check that the previous/next byte is "wide" as well, otherwise it is not
    // valid.
    let start = if mat.start >= 2 && mem[mat.start - 1] == b'\0' {
        mat.start - 2
    } else {
        mat.start
    };

    // Remove the wide bytes, and then use the non wide regex to check for word boundaries.
    // Since when checking word boundaries, we might match more than the initial match (because of
    // non greedy repetitions bounded by word boundaries), we need to add more data at the end.
    // How much? We cannot know, but including too much would be too much of a performance tank.
    // This is arbitrarily capped at 500 for the moment (or until the string is no longer wide)...
    // TODO bench this
    let unwiden_mem = unwide(&mem[start..std::cmp::min(mem.len(), mat.end + 500)]);

    let expected_start = if start < mat.start { 1 } else { 0 };
    match regex.find(&unwiden_mem) {
        Some(m) if m.start() == expected_start => {
            // Modify the match end. This is needed because the application of word boundary
            // may modify the match. Since we matched on non wide mem though, double the size.
            mat.end = mat.start + 2 * (m.end() - m.start());
            Some(mat)
        }
        _ => None,
    }
}

fn unwide(mem: &[u8]) -> Vec<u8> {
    let mut res = Vec::new();

    for b in mem.chunks_exact(2) {
        if b[1] != b'\0' {
            break;
        }
        res.push(b[0]);
    }

    res
}

// Is a match a wide string or an ascii one
fn is_match_wide(mat: &Range<usize>, mem: &[u8]) -> bool {
    if (mat.end - mat.start) % 2 != 0 {
        return false;
    }
    if mat.is_empty() {
        return true;
    }

    !mem[(mat.start + 1)..mat.end]
        .iter()
        .step_by(2)
        .any(|c| *c != b'\0')
}

fn is_ascii_alnum(c: u8) -> bool {
    (b'0'..=b'9').contains(&c) || (b'A'..=b'Z').contains(&c) || (b'a'..=b'z').contains(&c)
}

/// Convert an ascii string to a wide string
fn string_to_wide(s: &[u8]) -> Vec<u8> {
    let mut res = Vec::with_capacity(s.len() * 2);
    for b in s {
        res.push(*b);
        res.push(b'\0');
    }
    res
}

/// Error during the compilation of a variable.
#[derive(Debug)]
pub enum VariableCompilationError {
    /// Error when compiling a regex variable.
    Regex(String),

    /// Logic error while attempting to extract atoms from a variable.
    ///
    /// This really should not happen, and indicates a bug in the extraction code.
    AtomsExtractionError,

    /// Structural error when applying the `wide` modifier to a regex.
    ///
    /// This really should not happen, and indicates a bug in the code
    /// applying this modifier.
    WidenError,
}

impl std::fmt::Display for VariableCompilationError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Regex(e) => e.fmt(f),
            // This should not happen. Please report it upstream if it does.
            Self::AtomsExtractionError => write!(f, "unable to extract atoms"),
            // This should not happen. Please report it upstream if it does.
            Self::WidenError => write!(f, "unable to apply the wide modifier to the regex"),
        }
    }
}
