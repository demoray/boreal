use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::thread::JoinHandle;

use boreal::Compiler;
use boreal::{module::Value as ModuleValue, Scanner};

use clap::{command, value_parser, Arg, ArgAction, ArgMatches, Command};
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term::{
    self,
    termcolor::{ColorChoice, StandardStream},
};
use crossbeam_channel::{bounded, Receiver, Sender};
use walkdir::WalkDir;

fn build_command() -> Command {
    command!()
        .arg(
            Arg::new("no_follow_symlinks")
                .short('N')
                .long("no-follow-symlinks")
                .action(ArgAction::SetTrue)
                .help("Do not follow symlinks when scanning"),
        )
        .arg(
            Arg::new("print_module_data")
                .short('D')
                .long("print-module-data")
                .action(ArgAction::SetTrue)
                .help("Print module data"),
        )
        .arg(
            Arg::new("recursive")
                .short('r')
                .long("recursive")
                .action(ArgAction::SetTrue)
                .help("Recursively search directories"),
        )
        .arg(
            Arg::new("skip_larger")
                .short('z')
                .long("skip-larger")
                .value_name("MAX_SIZE")
                .value_parser(value_parser!(u64))
                .help("Skip files larger than the given size when scanning a directory"),
        )
        .arg(
            Arg::new("threads")
                .short('p')
                .long("threads")
                .value_name("NUMBER")
                .value_parser(value_parser!(usize))
                .help("Number of threads to use when scanning directories"),
        )
        .arg(
            Arg::new("rules_file")
                .value_parser(value_parser!(PathBuf))
                .required_unless_present("module_names")
                .help("Path to a yara file containing rules"),
        )
        .arg(
            Arg::new("input")
                .value_parser(value_parser!(PathBuf))
                .required_unless_present("module_names")
                .help("File or directory to scan"),
        )
        .arg(
            Arg::new("fail_on_warnings")
                .long("fail-on-warnings")
                .action(ArgAction::SetTrue)
                .help("Fail compilation of rules on warnings"),
        )
        .arg(
            Arg::new("module_names")
                .short('M')
                .long("module-names")
                .action(ArgAction::SetTrue)
                .help("Display the names of all available modules"),
        )
}

fn display_diagnostic(path: &Path, err: &boreal::compiler::AddRuleError) {
    let writer = StandardStream::stderr(ColorChoice::Auto);
    let config = term::Config::default();

    let files = match &err.path {
        Some(path) => {
            let contents = std::fs::read_to_string(path).unwrap_or_else(|_| String::new());
            SimpleFile::new(path.display().to_string(), contents)
        }
        None => SimpleFile::new(path.display().to_string(), String::new()),
    };
    let writer = &mut writer.lock();
    if let Err(e) = term::emit(writer, &config, &files, &err.to_diagnostic()) {
        eprintln!("cannot emit diagnostics: {e}");
    }
}

fn main() -> ExitCode {
    let args = build_command().get_matches();

    if args.get_flag("module_names") {
        let compiler = Compiler::new();

        let mut names: Vec<_> = compiler.available_modules().collect();
        names.sort_unstable();

        for name in names {
            println!("{name}");
        }

        return ExitCode::SUCCESS;
    }

    let scanner = {
        let rules_file: &PathBuf = args.get_one("rules_file").unwrap();

        #[cfg(feature = "authenticode")]
        // Safety: this is done before any multithreading context, so there is no risk of racing
        // other calls into OpenSSL.
        let mut compiler = unsafe { Compiler::new_with_pe_signatures() };
        #[cfg(not(feature = "authenticode"))]
        let mut compiler = Compiler::new();

        if args.get_flag("fail_on_warnings") {
            compiler.set_params(boreal::compiler::CompilerParams::default().fail_on_warnings(true));
        }

        match compiler.add_rules_file(rules_file) {
            Ok(status) => {
                for warn in status.warnings() {
                    display_diagnostic(rules_file, warn);
                }
            }
            Err(err) => {
                display_diagnostic(rules_file, &err);
                return ExitCode::FAILURE;
            }
        }

        compiler.into_scanner()
    };

    let input: &PathBuf = args.get_one("input").unwrap();
    if input.is_dir() {
        let mut walker = WalkDir::new(input).follow_links(!args.get_flag("no_follow_symlinks"));
        if !args.get_flag("recursive") {
            walker = walker.max_depth(1);
        }

        let (thread_pool, sender) = ThreadPool::new(&scanner, &args);

        for entry in walker {
            let entry = match entry {
                Ok(v) => v,
                Err(err) => {
                    eprintln!("{err}");
                    continue;
                }
            };

            if !entry.file_type().is_file() {
                continue;
            }

            if let Some(max_size) = args.get_one::<u64>("skip_larger") {
                if *max_size > 0 && entry.depth() > 0 {
                    let file_length = entry.metadata().ok().map_or(0, |meta| meta.len());
                    if file_length >= *max_size {
                        eprintln!(
                            "skipping {} ({} bytes) because it's larger than {} bytes.",
                            entry.path().display(),
                            file_length,
                            max_size
                        );
                        continue;
                    }
                }
            }

            sender.send(entry.path().to_path_buf()).unwrap();
        }

        drop(sender);
        thread_pool.join();

        ExitCode::SUCCESS
    } else {
        match scan_file(&scanner, input, args.get_flag("print_module_data")) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("Cannot scan {}: {}", input.display(), err);
                ExitCode::FAILURE
            }
        }
    }
}

fn scan_file(scanner: &Scanner, path: &Path, print_module_data: bool) -> std::io::Result<()> {
    let res = scanner.scan_file(path)?;

    if print_module_data {
        for (module_name, module_value) in res.module_values {
            // A module value must be an object. Filter out empty ones, it means the module has not
            // generated any values.
            if let ModuleValue::Object(map) = &*module_value {
                if !map.is_empty() {
                    print!("{module_name}");
                    print_module_value(&module_value, 4);
                }
            }
        }
    }
    for rule in res.matched_rules {
        println!("{} {}", &rule.name, path.display());
    }

    Ok(())
}

struct ThreadPool {
    threads: Vec<JoinHandle<()>>,
}

impl ThreadPool {
    fn new(scanner: &Scanner, args: &ArgMatches) -> (Self, Sender<PathBuf>) {
        let nb_cpus = if let Some(nb) = args.get_one::<usize>("threads") {
            std::cmp::min(1, *nb)
        } else {
            std::thread::available_parallelism()
                .map(|v| v.get())
                .unwrap_or(32)
        };

        let (sender, receiver) = bounded(nb_cpus * 5);
        (
            Self {
                threads: (0..nb_cpus)
                    .map(|_| {
                        Self::worker_thread(scanner, &receiver, args.get_flag("print_module_data"))
                    })
                    .collect(),
            },
            sender,
        )
    }

    fn join(self) {
        for handle in self.threads {
            handle.join().unwrap();
        }
    }

    fn worker_thread(
        scanner: &Scanner,
        receiver: &Receiver<PathBuf>,
        print_module_data: bool,
    ) -> JoinHandle<()> {
        let scanner = scanner.clone();
        let receiver = receiver.clone();

        std::thread::spawn(move || {
            while let Ok(path) = receiver.recv() {
                if let Err(err) = scan_file(&scanner, &path, print_module_data) {
                    eprintln!("Cannot scan file {}: {}", path.display(), err);
                }
            }
        })
    }
}

/// Print a module value.
///
/// This is a recursive function.
/// The invariants are:
///   - on entry, the previous line is unfinished (no newline written yet)
///   - on exit, the line has been ended (last written char is a newline)
/// This is so that the caller can either:
/// - print " = ..." for primitive values
/// - print "\n..." for compound values
fn print_module_value(value: &ModuleValue, indent: usize) {
    match value {
        ModuleValue::Integer(i) => println!(" = {i} (0x{i:x})"),
        ModuleValue::Float(v) => println!(" = {v}"),
        ModuleValue::Bytes(bytes) => {
            println!(" = {}", ByteString(bytes));
        }
        ModuleValue::Regex(regex) => println!(" = /{}/", regex.as_regex().as_str()),
        ModuleValue::Boolean(b) => println!(" = {b:?}"),
        ModuleValue::Object(obj) => {
            if obj.is_empty() {
                println!(" = {{}}");
                return;
            }

            println!();

            // For improved readability, we sort the keys before printing. Cost is of no concern,
            // this is only for CLI debugging.
            let mut keys: Vec<_> = obj.keys().collect();
            keys.sort_unstable();
            for key in keys {
                print!("{:indent$}{}", "", key);
                print_module_value(&obj[key], indent + 4);
            }
        }
        ModuleValue::Array(array) => {
            if array.is_empty() {
                println!(" = []");
                return;
            }

            println!();
            for (index, subval) in array.iter().enumerate() {
                print!("{:indent$}[{}]", "", index);
                print_module_value(subval, indent + 4);
            }
        }
        ModuleValue::Dictionary(dict) => {
            if dict.is_empty() {
                println!(" = {{}}");
                return;
            }

            println!();

            // For improved readability, we sort the keys before printing. Cost is of no concern,
            // this is only for CLI debugging.
            let mut keys: Vec<_> = dict.keys().collect();
            keys.sort_unstable();
            for key in keys {
                print!("{:indent$}[{}]", "", ByteString(key));
                print_module_value(&dict[key], indent + 4);
            }
        }
        ModuleValue::Function(_) => println!("[function]"),
        ModuleValue::Undefined => println!("[undef]"),
    }
}

struct ByteString<'a>(&'a [u8]);

impl std::fmt::Display for ByteString<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match std::str::from_utf8(self.0) {
            Ok(s) => write!(f, "{s:?}"),
            Err(_) => write!(f, "{{ {} }}", hex::encode(self.0)),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn verify_cli() {
        super::build_command().debug_assert();
    }
}
