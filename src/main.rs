use clap::{ Parser, ValueEnum };
use termimad::crossterm::style::{Attributes, Color};
use indicatif::{ ProgressBar, ProgressStyle };
use std::collections::HashMap;
use std::env;
use termimad::{ Alignment, CompoundStyle, LineStyle, ListItemsIndentationMode, MadSkin, ScrollBarStyle, StyledChar, TableBorderChars };
use std::fs;
use std::path::{ Path, PathBuf };
use git2::{ Repository, FetchOptions, RemoteCallbacks };
use serde::{ Deserialize, Serialize };
use anyhow::{ Result, anyhow };
use path_absolutize::Absolutize;
use std::time::Duration;
use std::sync::{ Arc, Mutex };

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
    git_repos: Vec<String>,
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

fn sync_git_repos(git_urls: &Vec<String>, local_dir: &Path) -> Result<()> {
    fs::create_dir_all(local_dir)?;

    let spinner = ProgressBar::new_spinner();
    spinner.set_message("Cloning repository...");
    spinner.enable_steady_tick(Duration::from_millis(100));
    spinner.set_style(ProgressStyle::default_spinner()
        .template("{spinner:.green} {msg}")
        .unwrap());

    let progress = Arc::new(Mutex::new(ProgressBar::new(0)));

    let mut callbacks = RemoteCallbacks::new();
    let progress_clone = progress.clone();
    callbacks.transfer_progress(move |stats| {
        let pb = progress_clone.lock().unwrap();

        if pb.length().is_none() {
            pb.set_length(stats.total_objects() as u64);
            pb.set_style(ProgressStyle::default_bar()
                .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} objects ({eta})")
                .unwrap()
                .progress_chars("=> "));
            pb.set_message("Downloading objects");
        }

        pb.set_position(stats.received_objects() as u64);
        true
    });

    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(callbacks);


    spinner.set_message("Starting download...");

    for git_url in git_urls {
        let repo_name = git_url
            .rsplit('/')
            .next().ok_or(anyhow!("Malformed repo url"))?
            .strip_suffix(".git")
            .unwrap_or_else(|| git_url.rsplit('/').next()
            .expect("next() was checked above"));

        let clone_dir = local_dir.join(repo_name);

        match Repository::clone(&git_url, &clone_dir) {
            Ok(repo) => println!("successfully cloned {}", repo.path().display()),
            Err(e) => println!("failed to clone: {e}"),
        }
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
    if let Some(page_name) = &args.page_name {
        let db_iter = fs::read_dir(download_dir)?
            .filter_map(|entry| {
                match entry {
                    Ok(e) if e.path().is_dir() => Some(e.path()),
                    _ => None,
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
    else {
        println!("\nPlease enter the name of the page[s] you wish to see");
        println!("Run \"pager --help\" to see usage\n");
    }

    Ok(())
}
