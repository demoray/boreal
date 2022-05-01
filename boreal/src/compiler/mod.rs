//! Compilation of a parsed expression into an optimized one.
use std::collections::HashMap;

use codespan_reporting::diagnostic::Diagnostic;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term;

use boreal_parser as parser;

mod base64;
mod error;
pub use error::CompilationError;
mod expression;
pub use expression::*;
mod variable;
pub use variable::*;
mod module;
pub use module::*;
mod rule;
pub use rule::*;

use crate::Scanner;

/// Object used to compile rules.rovides methods to
#[derive(Debug, Default)]
pub struct Compiler {
    /// List of compiled rules.
    rules: Vec<Rule>,

    /// Modules declared in the scanner, added with [`Compiler::add_module`].
    ///
    /// These are modules that can be imported and used in the namespaces.
    available_modules: HashMap<String, Module>,
}

impl Compiler {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a module
    pub fn add_module<M: crate::module::Module>(&mut self, module: M) {
        let m = compile_module(module);
        // Ignore the result: that would mean the same module is already registered.
        // FIXME: this is done to allow the double "import" in a rule, but this can be improved.
        let _res = self.available_modules.insert(m.name.clone(), m);
    }

    /// Add rules to the scanner from a string.
    ///
    /// # Errors
    ///
    /// If parsing of the rules fails, an error is returned.
    pub fn add_rules_from_str(&mut self, s: &str) -> Result<(), AddRuleError> {
        let file = parser::parse_str(s).map_err(AddRuleError::ParseError)?;
        self.add_file(file)
            .map_err(AddRuleError::CompilationError)?;
        Ok(())
    }

    /// Add rules in the scanner.
    fn add_file(&mut self, file: parser::YaraFile) -> Result<(), CompilationError> {
        let mut symbols = HashMap::new();

        for component in file.components {
            match component {
                parser::YaraFileComponent::Include(_) => todo!(),
                parser::YaraFileComponent::Import(import) => {
                    match self.available_modules.get(&import) {
                        Some(module) => {
                            // Ignore result: if the import was already done, it's fine.
                            let _r = symbols.insert(import.clone(), module);
                        }
                        None => return Err(CompilationError::UnknownImport(import.clone())),
                    };
                }
                parser::YaraFileComponent::Rule(rule) => {
                    self.rules.push(compile_rule(*rule, &symbols)?);
                }
            }
        }

        Ok(())
    }

    #[must_use]
    pub fn into_scanner(self) -> Scanner {
        Scanner::new(self.rules)
    }
}

#[derive(Debug)]
pub enum AddRuleError {
    /// Error while parsing a rule.
    ParseError(boreal_parser::Error),
    /// Error while compiling a rule.
    CompilationError(CompilationError),
}

impl AddRuleError {
    /// Convert to a displayable, single-lined description.
    ///
    /// # Arguments
    ///
    /// * `input_name`: a name for the input, used at the beginning of the
    ///   description: `<filename>:<line>:<column>: <description>`.
    /// * `input`: the input given to [`parse_str`] that generated the error.
    #[must_use]
    pub fn to_short_description(&self, input_name: &str, input: &str) -> String {
        // Generate a small report using codespan_reporting
        let mut writer = term::termcolor::Buffer::no_color();
        let config = term::Config {
            display_style: term::DisplayStyle::Short,
            ..term::Config::default()
        };

        let files = SimpleFile::new(&input_name, &input);
        // TODO: handle error better here?
        let _res = term::emit(&mut writer, &config, &files, &self.to_diagnostic());
        String::from_utf8_lossy(writer.as_slice()).to_string()
    }

    /// Convert to a [`Diagnostic`].
    ///
    /// This can be used to display the error in a more user-friendly manner
    /// than the simple `to_short_description`. It does require depending
    /// on the `codespan_reporting` crate to make use of this diagnostic
    /// however.
    #[must_use]
    pub fn to_diagnostic(&self) -> Diagnostic<()> {
        match self {
            Self::ParseError(err) => err.to_diagnostic(),
            Self::CompilationError(err) => err.to_diagnostic(),
        }
    }
}

#[cfg(test)]
mod tests;
