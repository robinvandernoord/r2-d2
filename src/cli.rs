use clap::Parser;
use clap_complete::Shell;

pub const fn get_styles() -> clap::builder::Styles {
    clap::builder::Styles::styled()
        .usage(
            anstyle::Style::new()
                .bold()
                .underline()
                .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Yellow))),
        )
        .header(
            anstyle::Style::new()
                .bold()
                .underline()
                .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Yellow))),
        )
        .literal(
            anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Green))),
        )
        .invalid(
            anstyle::Style::new()
                .bold()
                .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Red))),
        )
        .error(
            anstyle::Style::new()
                .bold()
                .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Red))),
        )
        .valid(
            anstyle::Style::new()
                .bold()
                .underline()
                .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Green))),
        )
        .placeholder(
            anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::White))),
        )
}

pub trait Process {
    async fn process(self) -> anyhow::Result<i32>;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Parser)]
#[clap(version, styles=get_styles())]
pub struct Args {
    #[arg(long = "generate", value_enum)]
    pub generator: Option<Shell>,

    #[clap(subcommand)]
    pub cmd: Commands,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Parser)]
pub struct OverviewOptions {}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Parser)]
pub struct UploadOptions {}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Parser)]
pub enum Commands {
    Overview(OverviewOptions),
    Upload(UploadOptions),
}

impl Process for Commands {
    async fn process(self) -> anyhow::Result<i32> {
        match self {
            Commands::Overview(opts) => {
                opts.process().await
            }
            Commands::Upload(opts) => {
                opts.process().await
            }
        }
    }
}
