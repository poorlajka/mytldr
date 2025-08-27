use clap::{ Parser, ValueEnum };
use std::thread;
use termimad::crossterm::style::{Attributes, Color};
use indicatif::{ ProgressBar, ProgressStyle, MultiProgress };
use std::collections::HashMap;
use std::env;
use termimad::{ Alignment, CompoundStyle, LineStyle, ListItemsIndentationMode, MadSkin, ScrollBarStyle, StyledChar, TableBorderChars };
use std::fs;
use regex::Regex;
use std::path::{ Path, PathBuf };
use git2::{ Repository, FetchOptions, RemoteCallbacks };
use std::process::{Command, Stdio};
use serde::{ Deserialize, Serialize };
use anyhow::{ Result, anyhow };
use path_absolutize::Absolutize;
use std::time::Duration;
use std::sync::{ Arc, Mutex };
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{self, BufRead, BufReader, Read, stdout, Write};

static NAME: &'static str = env!("CARGO_PKG_NAME");
static VERSION: &'static str = env!("CARGO_PKG_VERSION");
static ABOUT_MSG: &'static str = r#"

Art by Hayley Jane Wakenshaw

        ,-""""""-.
     /\j__/\  (  \`--.
     \`@_@'/  _)  >--.`.     Pager: A personal 
    _{.:Y:_}_{{_,'    ) )    page/note-viewer
   {_}`-^{_} ```     (_/

"#;

type MarkdownPage = String;

#[derive(Debug, Deserialize, Serialize)]
struct Config {
    page_db: PageDb,
    style: Style,
    default_flags: DefaultFlags,
}

#[derive(Debug, Deserialize, Serialize)]
struct PageDb {
    git_repos: Vec<Vec<String>>,
    git_download_dir: String,
    local_dirs: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Style {
}

#[derive(Debug, Deserialize, Serialize)]
struct DefaultFlags {

}

impl Default for Config {
    fn default() -> Self {
        Config {
            page_db: PageDb {
                git_repos: Vec::new(),
                git_download_dir: String::from("./online_pages"),
                local_dirs: Vec::new(),
            },
            style: Style {

            },
            default_flags: DefaultFlags {

            },
        }
    }
}

#[derive(Clone, Debug, ValueEnum)]
enum Shell {
    Bash,
    Zsh,
    Fish,
}

#[derive(Parser, Debug)]
#[command(
help_template = "\
{about}

USAGE:
    {usage}

OPTIONS:
{all-args}
{after-help}"
)]
#[command(
    name = NAME, 
    version = VERSION, 
    about = ABOUT_MSG,
    after_help = "\u{200B}",
    disable_help_flag = true,
    disable_version_flag = true,
    arg_required_else_help = true,
)]
struct Args {
    /// Show documentation 
    #[arg(long)]
    documentation: bool,

    /// Sync online page-db against git repos
    #[arg(long)]
    sync: bool,

    /// Generate shell completions 
    #[arg(long)]
    completions: Option<Shell>,

    /// Combine multiple pages with the same name (otherwise only show one)
    #[arg(short, long)]
    combine: bool,

    /// Show long version of page[s]
    #[arg(short, long)]
    long: bool,

    /// Search for pattern in page[s]
    #[arg(short, long)]
    search: Option<String>,

    /// Show page[s] in interactive tui mode
    #[arg(short, long)]
    interactive: bool,

    /// Name of page[s] to show
    page_name: Option<String>,

    /// Show usage 
    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,

    /// Show version 
    #[arg(short, long, action = clap::ArgAction::Version)]
    version: Option<bool>,
}

fn show_page(page: &str, skin: &MadSkin, _args: &Args) {
    skin.print_text(page);
    println!("");
}

fn sync_git_repos(git_urls: &Vec<Vec<String>>, parent_dir: &Path) -> Result<()> {
    fs::create_dir_all(parent_dir)?;
    let multi_progress = MultiProgress::new();
    multi_progress.println("Cloning online page repos from git")?;
    let mut threads = vec![];
    for entry in git_urls.iter().rev() {
        let url = &entry[0];

        let repo_name = url
            .split('/')
            .last()
            .unwrap_or("unknown")
            .trim_end_matches(".git");

        let target_dir = parent_dir.join(repo_name);
        if target_dir.exists() {
            fs::remove_dir_all(&target_dir).unwrap();
        }

        let progress_bar = multi_progress.add(ProgressBar::new_spinner());
        let url_clone = url.clone();
        threads.push(thread::spawn(move || {
            thread::sleep(Duration::from_millis(500));
            clone_repo(&url_clone, &target_dir, &progress_bar.clone()).unwrap();
        }));
    }
    for thread in threads {
        let _ = thread.join();
    }

    Ok(())
}

enum CloneState {
    ReceivingObjects,
    ResolvingDeltas,
    UpdatingFiles,
    Finished,
}

impl CloneState {
    fn new() -> Self {
        CloneState::ReceivingObjects
    }

    fn next(&self) -> Self {
        match self {
            CloneState::ReceivingObjects => CloneState::ResolvingDeltas,
            CloneState::ResolvingDeltas => CloneState::UpdatingFiles,
            CloneState::UpdatingFiles => CloneState::Finished,
            CloneState::Finished => CloneState::Finished,
        }
    }

    fn text(&self) -> String {
        match self {
            CloneState::ReceivingObjects => String::from("Receiving objects"),
            CloneState::ResolvingDeltas => String::from("Resolving deltas"),
            CloneState::UpdatingFiles => String::from("Updating files"),
            CloneState::Finished => String::from("Finished"),
        }
    }

    fn style(&self) -> ProgressStyle {
        let template = match self {
            CloneState::ReceivingObjects => "[{bar:40.cyan/blue}] {pos}/{len} {msg}",
            CloneState::ResolvingDeltas => "[{bar:40.yellow/cyan}] {pos}/{len} {msg}",
            CloneState::UpdatingFiles => "[{bar:40.green/yellow}] {pos}/{len} {msg}",
            CloneState::Finished => "[{bar:40.green/yellow}] {pos}/{len} {msg}",
        };

        let progress_chars = match self {
            CloneState::ReceivingObjects => "##-",
            CloneState::ResolvingDeltas => "=>#",
            CloneState::UpdatingFiles => "->=",
            CloneState::Finished => "->=",
        };

        ProgressStyle::default_bar()
            .template(template)
            .unwrap()
            .progress_chars(progress_chars)
    }
}

fn clone_repo(url: &str, dest: &Path, pb: &ProgressBar) -> anyhow::Result<()> {
    /*
        It would have been less hacky to use libgit2 for this but it was 10-100x slower 
        which made cloning larger repos such as tldr really tedious imo.
        So instead I opted to use the native git binary and manually parse the progress 
        output with some regex to drive indicatif progress bars.

        Needed to do some fiddling with the output from git since it iterates the progress
        report using carriage returns. Solution based on: https://askubuntu.com/a/990280 
    */

    let mut cmd = Command::new("git");
    cmd.arg("clone")
        .arg("--progress")
        .arg(url)
        .arg(dest)
        .stderr(Stdio::piped());

    let mut child = cmd.spawn()?;
    let mut stderr = child.stderr.take().expect("Failed to capture stderr");
    let mut reader = BufReader::new(stderr);


    pb.enable_steady_tick(Duration::from_millis(100));
    let repo_name = dest.components().last().expect("This cannot be empty").as_os_str();
    pb.set_message(format!("Beginning cloning for {:?}", repo_name));

    let mut buffer = Vec::new();
    let mut temp = [0u8; 1024];
    let mut clone_state = CloneState::new();

    while let Ok(n) = reader.read(&mut temp) {
        if n == 0 { break; }
        buffer.extend_from_slice(&temp[..n]);
        // Replace \r with \n
        while let Some(pos) = buffer.iter().position(|&b| b == b'\r') {
            buffer[pos] = b'\n';
        }

        // Process complete lines
        while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
            let line = String::from_utf8_lossy(&buffer[..pos]).to_string();
            buffer.drain(..=pos);

            let pattern = &format!(r"{}:\s*(?:\d+%)?\s*\((\d+)/(\d+)\)", clone_state.text());

            let progress_regex = Regex::new(pattern).unwrap();

            let mut last_received = 0;
            if let Some(caps) = progress_regex.captures(&line) {
                let received: u64 = caps[1].parse().unwrap_or(0);
                let total: u64 = caps[2].parse().unwrap_or(0);
                pb.set_length(total);
                pb.set_position(received);
                if received > last_received {
                    pb.set_style(clone_state.style());
                    pb.set_message(format!("Cloning {:?}: {}", repo_name, clone_state.text()));
                }

                if received == total {
                    clone_state = clone_state.next();
                    last_received = 0;
                }
            }
        }
    }

    let status = child.wait()?;
    if status.success() {
        pb.set_style(clone_state.style());
        pb.finish_with_message(format!("✅ Finished cloning {:?}", repo_name));
    } else {
        pb.set_style(clone_state.style());
        pb.finish_with_message(format!("❌ Failed cloning {:?}", repo_name));
    }

    Ok(())
}

fn get_page<I>(page_name: &str, db_iter: I, args: &Args) -> MarkdownPage 
where 
    I: Iterator<Item = PathBuf>,
{
    let mut pages = db_iter
        .filter_map(|dir| {
        let page_path = dir.join(format!("{page_name}.md"));

        if page_path.exists() && page_path.is_file() {
            fs::read_to_string(&page_path).ok()
        } else {
            None
        }
    }).peekable();

    if pages.peek().is_none() {
        format!("No result found for: {page_name}")
    }
    else {
        if args.combine {
            pages.reduce(|s1, s2| format!("{s1}\n{s2}"))
                .expect("I check for this in the if above")
        }
        else {
            pages.next()
                .expect("I check for this in the if above")
        }
    }
}

fn validate_config(_config: &Config) -> Vec<String> {
    Vec::new()
}

fn get_skin(_style: &Style) -> MadSkin {
    let c = CompoundStyle::new(Some(Color::White), None, Attributes::none());
    let l = LineStyle::new(c.clone(), Alignment::Left);
    let s = StyledChar::nude('*');
    MadSkin {
        paragraph: l.clone(),
        bold: c.clone(),
        italic: c.clone(),
        strikeout: c.clone(),
        inline_code: c.clone(),
        code_block: l.clone(),
        headers: [l.clone(), l.clone() ,l.clone() ,l.clone() ,l.clone() ,l.clone() ,l.clone() ,l.clone()],
        scrollbar: ScrollBarStyle {track: s.clone(), thumb: s.clone()},
        table: l,
        bullet: s.clone(),
        quote_mark: s.clone(),
        horizontal_rule: s.clone(),
        ellipsis: c,
        table_border_chars: &TableBorderChars {
            horizontal: '-',
            vertical: '|',
            top_left_corner: '/',
            top_right_corner: '\\',
            bottom_right_corner: '/',
            bottom_left_corner: '\\',
            top_junction: '-',
            right_junction: '-',
            bottom_junction: '-',
            left_junction: '-',
            cross: '+',
        },
        list_items_indentation_mode: ListItemsIndentationMode::Block,
        special_chars: HashMap::new(),
    }
}

fn get_online_hashmap(git_repos: &Vec<Vec<String>>) -> HashMap<String, String> {
    let online_hashmap: HashMap<_, _> = git_repos
        .iter()
        .filter_map(|x| {
            let repo_name = x[0]
                .split('/')
                .last()
                .unwrap_or("unknown")
                .trim_end_matches(".git");

            if x.len() == 2 {
                Some((String::from(repo_name), x[1].clone()))
            }
            else {
                None
            }
        })
        .collect();

    online_hashmap
}

fn main() -> Result<()> {
    /* 
       Ensure config exists, if not copy the default config 
    */
    let config_path = env::home_dir()
        .ok_or(anyhow!("User home dir could not be found"))?
        .join(Path::new(".config/pager/config.toml"));

    if !config_path.exists() {
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let toml_config = toml::to_string_pretty(&Config::default())?;
        fs::write(&config_path, &toml_config)?;
    }

    /* 
        Parse and validate config 
    */
    let toml_str = fs::read_to_string(&config_path)?;
    let config = toml::from_str(&toml_str)?;
    let config_errors = validate_config(&config);

    if !config_errors.is_empty() {
        for error in config_errors {
            println!("{error}");
        }
        return Ok(()); 
    }

    let PageDb {
        git_repos: repos, 
        git_download_dir,
        local_dirs,
    } = &config.page_db;


    let config_dir = config_path
        .parent()
        .ok_or(anyhow!("'Aint no way, aint no fucking way' -Future"))?;

    let download_dir = Path::new(git_download_dir)
        .absolutize_from(config_dir)?
        .to_path_buf();

    /* 
        Parse cli args 
    */
    let args = Args::parse();
    if args.sync {
        sync_git_repos(repos, &download_dir)?;
        return Ok(());
    }

    /* 
        Lookup and show page 
    */
    let online_hashmap = get_online_hashmap(repos);
    if let Some(page_name) = &args.page_name {
        let db_iter = fs::read_dir(download_dir)?
            .flat_map(|entry| {
                match entry {
                    Ok(e) => {
                        if e.path().is_dir() {
                            let file_name = e.file_name();
                            let dir_name = file_name.to_str().unwrap_or("");
                            let mut path = e.path();
                            if let Some(subdir) = online_hashmap.get(dir_name) {
                                for c in subdir.split("/") {
                                    if c == "*" {
                                        // This is some ugly ass fuckshit
                                        let mut paths = Vec::new();
                                        for entry in fs::read_dir(path).unwrap() {
                                            let e = entry.unwrap();
                                            let metadata = e.metadata().unwrap();
                                            if metadata.is_dir() {
                                                paths.push(e.path());
                                            }
                                        }
                                        return paths;
                                    }
                                    else {
                                        path = path.join(c);
                                    }
                                }
                            }
                            vec![path]
                        }
                        else {
                            Vec::new()
                        }
                    },
                    _ => Vec::new(),
                }
            }).chain(local_dirs
                .iter()
                .filter_map(|dir| Path::new(dir)
                    .absolutize_from(&config_dir).ok() // Imo it should be fine to throw away bad paths
                    .map(|abs| abs.to_path_buf())
                )
            );

        let page = get_page(&page_name, db_iter, &args);
        let skin = get_skin(&config.style);
        show_page(&page, &skin, &args);
    }
    /* 
    else {
        println!("\nPlease enter the name of the page[s] you wish to see");
        println!("Run \"pager --help\" to see usage\n");
    }
    */

    Ok(())
}
