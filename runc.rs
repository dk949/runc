use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::path::Path;
use std::process::{exit, Command, Output, Stdio};
use std::result::Result;
use std::str;

const VERISON_STRING: &str = "2.0.0";

#[derive(Debug)]
enum ExitCode {
    Ok = 0,
    ArgumentError,
    LanguageError,
    EditorError,
    FileError,
    RunnerError,
    CodeError,
}

type RunError = (ExitCode, String);

fn run_file(
    program: &[&'static str],
    args: Option<Vec<String>>,
    argv: Option<Vec<String>>,
    file: String,
    _: &mut Vec<String>,
) -> Result<Output, io::Error> {
    Command::new(program[0])
        .args(&program[1..])
        .args(args.unwrap_or(Vec::new()))
        .arg(file)
        .args(argv.unwrap_or(Vec::new()))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
}

struct Lang {
    runner: fn(
        args: Option<Vec<String>>,
        argv: Option<Vec<String>>,
        file: String,
        used_files: &mut Vec<String>,
    ) -> Result<Output, io::Error>,

    // has to include the dot. e.g '.py' not 'py'
    extension: &'static str,

    // list of executables that have to be present in order to run the program
    req: &'static [&'static str],
}
impl fmt::Debug for Lang {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "runner: <funciton>, req: {:?}, extension: {}",
            self.req, self.extension
        )
    }
}

#[derive(Debug)]
struct Runner {
    langs: HashMap<&'static str, Lang>,
    cache_dir: Option<String>,
    lang: String,
    editor: String,
    file: Option<String>,
    // It's a cache, so it's internally mutable
    used_files: RefCell<Vec<String>>,
}

macro_rules! bind_lang {
    ($lang:expr) => {
        |args: Option<Vec<String>>,
         argv: Option<Vec<String>>,
         file: String,
         used_files: &mut Vec<String>| run_file($lang, args, argv, file, used_files)
    };
}

trait Handleable {
    fn handle_command(self: Self) -> ExitCode;
}

impl Handleable for Result<Output, io::Error> {
    fn handle_command(self: Self) -> ExitCode {
        match self {
            Ok(res) => match res.status.code() {
                Some(code) => match code {
                    0 => {
                        println!(
                            "Succeeded\nstdout:\n\t{}\nstderr:\n\t{}",
                            str::from_utf8(&res.stdout).unwrap_or("error"),
                            str::from_utf8(&res.stderr).unwrap_or("error")
                        );
                        ExitCode::Ok
                    }
                    err => {
                        println!(
                            "Failed ({})\nstdout:\n\t{}\nstderr:\n\t{}",
                            err,
                            str::from_utf8(&res.stdout).unwrap_or("error"),
                            str::from_utf8(&res.stderr).unwrap_or("error")
                        );
                        ExitCode::CodeError
                    }
                },
                None => {
                    println!("Process terminated by a signal");
                    ExitCode::CodeError
                }
            },
            Err(err) => {
                println!("Could not run process: {}", err);
                ExitCode::RunnerError
            }
        }
    }
}

trait HistWritable {
    fn write_hist(&mut self, new_hist: bool) -> Result<(), RunError>;
}

impl Drop for Runner {
    fn drop(&mut self) {
        println!("dropped");
        dbg!(&self.used_files);
        for file in self.used_files.borrow().iter() {
            if Path::new(file).exists() {
                // FIXME
                // Guess I'll ignore the errors for now
                fs::remove_file(file)
                    .expect("INTERNAL ERROR failed to remove temporary file after run");
            }
        }
    }
}

impl Runner {
    const CACHE_NAME: &'static str = "runc_cache";

    fn get_hist_file(&self) -> Option<String> {
        Some(
            Path::new(self.cache_dir.as_ref()?)
                .join(Self::CACHE_NAME.to_string() + self.langs.get(&self.lang.as_str())?.extension)
                .to_string_lossy()
                .to_string(),
        )
    }
    fn load_hist(&self, new_hist: bool) -> Result<Vec<u8>, RunError> {
        // FIXME: snippets
        let mut ret = Vec::new();
        if new_hist {
            return Ok(ret);
        }
        if let Some(file) = self.get_hist_file() {
            let file = Path::new(&file);
            if file.exists() {
                let mut file =
                    File::open(file).expect("INTERNAL ERROR: failed to open the hist file.");
                file.read(&mut ret)
                    .expect("INTERNAL ERROR: failed to read hist file");
            }
        }
        Ok(ret)
    }

    #[inline]
    fn extract_arg(arg: Option<String>) -> Option<Vec<String>> {
        arg.map::<Vec<_>, _>(|s| s.split(' ').map(|s| s.to_string()).collect())
    }

    fn make_file(&self, new_hist: bool) -> Result<String, RunError> {
        let file_name = std::env::temp_dir().join(format!(
            "runc_runner{}",
            self.langs
                .get(self.lang.as_str())
                .expect("INTERNAL ERROR: expected language to have been checked before the call to make_file")
                .extension
        ));
        self.used_files
            .borrow_mut()
            .push(file_name.to_string_lossy().to_string());
        dbg!(&self.used_files);
        let mut file = File::create(&file_name).or(Err((
            ExitCode::FileError,
            "could not open file".to_string(),
        )))?;
        file.write_all(&self.load_hist(new_hist)?).or(Err((
            ExitCode::FileError,
            "could not write to file".to_string(),
        )))?;
        Ok(file_name.to_string_lossy().to_string())
    }

    fn get_cache_dir(no_hist: bool) -> Option<String> {
        if no_hist {
            return None;
        }
        let cache = if let Ok(xdg_cache) = std::env::var("XDG_CACHE_HOME") {
            xdg_cache
        } else {
            let home = std::env::var("HOME").ok()?;
            let home = Path::new(&home);
            home.join(".cache").to_string_lossy().to_string()
        };
        let cache = Path::new(&cache);
        if !cache.exists() || !cache.is_dir() {
            return None;
        }
        let cache = cache.join(Self::CACHE_NAME);
        if !cache.exists() {
            fs::create_dir(&cache).ok()?;
        }

        return Some(cache.to_string_lossy().to_string());
    }

    fn get_editor() -> Result<String, RunError> {
        if let Ok(editor) = std::env::var("EDITOR") {
            Ok(editor)
        } else {
            Err((
                ExitCode::EditorError,
                "Could not determine editor. Try setting EDITOR environment variable.".to_string(),
            ))
        }
    }

    fn open_editor(&self, new_hist: bool) -> Result<String, RunError> {
        let f = self.make_file(new_hist)?;
        match Command::new(&self.editor)
            .arg(&f)
            // turns out this `inherit` thing is really important :\
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
        {
            Ok(res) => match res.status.code() {
                Some(code) => {
                    if code == 0 {
                        Ok(f)
                    } else {
                        Err((
                            ExitCode::EditorError,
                            format!(
                                "Failed to run the editor. Command {} {} failed with {}:\n\n{}\n\n{}",
                                self.editor,
                                f,
                                code,
                                str::from_utf8(&res.stdout).unwrap_or("failed decoding stderr"),
                                str::from_utf8(&res.stderr).unwrap_or("failed decoding stderr")
                            ),
                        ))
                    }
                }
                None => Err((
                    ExitCode::EditorError,
                    "editor was closed with a signal".to_string(),
                )),
            },
            Err(_) => Err((
                ExitCode::EditorError,
                format!(
                    "failed to start the editor {}. make sure it is in PATH",
                    self.editor
                ),
            )),
        }
    }
    fn store_hist() -> Result<(), RunError> {
        // TODO
        Ok(())
    }

    fn init(mut self, new_hist: bool) -> Result<Self, RunError> {
        if !self.langs.contains_key(&self.lang.as_str()) {
            return Err((
                ExitCode::LanguageError,
                format!("unsupported language \"{}\"", self.lang),
            ));
        }
        let file = self.open_editor(new_hist)?;
        self.file = Some(file);
        Self::store_hist()?;
        Ok(self)
    }

    fn new(lang: String, no_hist: bool, new_hist: bool) -> Result<Self, RunError> {
        Runner {
            #[cfg_attr(rustfmt, rustfmt_skip)]
            langs: HashMap::from([
                ("python", Lang { runner: bind_lang!(&[&"python"]), extension: ".py", req: &["python"]})
            ]),
            cache_dir: Self::get_cache_dir(no_hist),
            lang: lang,
            editor: Self::get_editor()?,
            file: None,
            used_files: RefCell::new(Vec::new()),
        }.init(new_hist)
    }

    fn run(&self, compiler_args: Option<String>, prog_args: Option<String>) -> ExitCode {
        (self
            .langs
            .get(&self.lang.as_str())
            .expect("INTERNAL ERROR: expected langauge to be set by the time run is called")
            .runner)(
            Self::extract_arg(compiler_args),
            Self::extract_arg(prog_args),
            self.file
                .as_ref()
                .expect("expected self.file to be set at this point")
                .to_string(), // FIXME: do I have to copy here? (not that it matters much, but still)
            &mut Vec::new(),
        )
        .handle_command()
    }
}

fn gen_epilog() -> &'static str {
    "epilog"
}

#[derive(Clone, Debug)]
struct Arg<T> {
    val: T,
    long: &'static str,
    short: char,
    help: &'static str,
}

#[derive(Debug, Clone)]
struct Args {
    no_hist: Arg<bool>,
    new_hist: Arg<bool>,
    ls: Arg<bool>,
    aliases: Arg<bool>,

    // '--args', metavar='ARGS'
    compiler_args: Arg<Option<String>>,

    //'--argv', metavar='ARGS'
    prog_args: Arg<Option<String>>,

    // metavar='LANG'
    lang: Arg<Option<String>>,
}

impl Args {
    fn new() -> Self {
        Args {
            no_hist: Arg::<bool>                {val: false, long: "temp"       , short: 't' , help: "ignore history and use default snippet"                                            },
            new_hist: Arg::<bool>               {val: false, long: "new-history", short: 'n' , help: "reset current language history to default"                                         },
            ls: Arg::<bool>                     {val: false, long: "ls"         , short: 'l' , help: "list available languages"                                                          },
            aliases: Arg::<bool>                {val: false, long: "aliases"    , short: 'a' , help: "list available language aliases"                                                   },
            compiler_args: Arg::<Option<String>>{val: None , long: "args="      , short: '\0', help: "space separated list of arguments to be passed to the compiler or the interpreter."},
            prog_args: Arg::<Option<String>>    {val: None , long: "argv="      , short: '\0', help: "space separated list of arguments to be passed to the executed program"            },
            lang: Arg::<Option<String>>         {val: None , long: "\0"         , short: '\0', help: "language to be ran"                                                                },
        }
    }
}

#[cfg_attr(rustfmt, rustfmt_skip)]
fn help(prog: &'static str, description: &'static str, args: Args) -> !{
    println!("usage: {} [OPTIONS] LANG", prog);
    println!("       {} [OPTIONS]\n", prog);

    println!("{}\n", description);

    println!("positional arguments:");
    println!("  LANG               {}\n", args.lang.help);


    println!("options:");
    println!("  -{}, --{:12}{}", 'h', "help", "show this help message and exit");
    println!("  -{}, --{:12}{}", args.no_hist.short, args.no_hist.long, args.no_hist.help);
    println!("  -{}, --{:12}{}", args.new_hist.short,args.new_hist.long, args.new_hist.help);
    println!("  -{}, --{:12}{}", args.ls.short,args.ls.long, args.ls.help);
    println!("  -{}, --{:12}{}", args.aliases.short,args.aliases.long, args.aliases.help);
    println!( "      --{:12}{}", args.compiler_args.long, args.compiler_args.help);
    println!( "      --{:12}{}\n", args.prog_args.long, args.prog_args.help);

    println!("{}\n", gen_epilog());
    exit(ExitCode::Ok as i32)
}

fn version(prog: &'static str) -> ! {
    println!("{}: v{}", prog, VERISON_STRING);
    exit(ExitCode::Ok as i32)
}

fn parse_args() -> Result<Args, String> {
    let prog = "runc";
    let description = "Open the EDITOR. Write some code. Have it executed.";

    let mut args = Args::new();

    fn str_arg(arg: &String, val: &mut Arg<Option<String>>) -> Option<String> {
        if arg[2..] == val.long[..val.long.len() - 1] {
            return Some(format!("Invalid argument format, try `{}=`", arg));
        }
        if arg[2..].starts_with(val.long) {
            if let Some(s) = arg.split('=').nth(1) {
                val.val = Some(s.to_string());
                return None;
            } else {
                panic!("INTERNAL LOGIC ERROR: no `=` in arg");
            }
        }
        return None;
    }

    for arg in std::env::args().skip(1) {
        if arg.starts_with("--") {
            if &arg[2..] == "help" {
                help(prog, description, args.clone());
            }
            args.no_hist.val = args.no_hist.val || &arg[2..] == args.no_hist.long;
            args.new_hist.val = args.new_hist.val || &arg[2..] == args.new_hist.long;
            args.ls.val = args.ls.val || &arg[2..] == args.ls.long;
            args.aliases.val = args.aliases.val || &arg[2..] == args.aliases.long;
            if let Some(err) = str_arg(&arg, &mut args.compiler_args) {
                return Err(err);
            }
            if let Some(err) = str_arg(&arg, &mut args.prog_args) {
                return Err(err);
            }
            continue;
        }
        if arg.starts_with('-') {
            arg[1..].chars().for_each(|ch| {
                if ch == 'h' {
                    help(prog, description, args.clone());
                }
                if ch == 'v' {
                    version(prog);
                }
                args.no_hist.val = args.no_hist.val || ch == args.no_hist.short;
                args.new_hist.val = args.new_hist.val || ch == args.new_hist.short;
                args.ls.val = args.ls.val || ch == args.ls.short;
                args.aliases.val = args.aliases.val || ch == args.aliases.short;
            });
            continue;
        }

        if args.lang.val.is_some() {
            return Err(format!("unexpected positional argument `{}`", arg));
        }
        args.lang.val = Some(arg);
    }

    return Ok(args);
}

fn main() {
    let args = parse_args();
    if args.is_err() {
        println!("error: {}", args.unwrap_err());
        exit(ExitCode::ArgumentError as i32);
    }
    let args = args.unwrap();

    if args.ls.val {
        println!("Avaliable language:\n___________________");
        //list(map(partial(print, "   "), Runner._langs))
        exit(ExitCode::Ok as i32);
    }
    if args.aliases.val {
        println!("Avaliable aliases:\n___________________");
        //list(map(lambda a: print(str(a[0]).rjust(10), ':', str(a[1]).ljust(10)), Runner._aliases.items()))
        exit(ExitCode::Ok as i32);
    }

    if let Some(lang) = args.lang.val {
        match Runner::new(lang, args.no_hist.val, args.new_hist.val) {
            Ok(runner) => exit(runner.run(args.compiler_args.val, args.prog_args.val) as i32),
            Err(err) => {
                println!("{}", err.1);
                exit(err.0 as i32)
            }
        };
    } else {
        println!("Bad args. try '-h/--help'");
        exit(ExitCode::ArgumentError as i32);
    }
}
