use std::path::PathBuf;

use clap::Parser;

use zingolib_tui_core::palette;

#[derive(Parser, Debug, Clone)]
#[command(name = "zingolib-tui", version, about)]
pub struct Cli {
    #[arg(long)]
    pub server: Option<String>,

    #[arg(long, value_name = "DIR")]
    pub data_dir: Option<PathBuf>,

    #[arg(long, short = 'w', value_name = "NAME")]
    pub wallet: Option<String>,

    #[arg(long, value_name = "UFVK", requires = "birthday")]
    pub ufvk: Option<String>,

    #[arg(long, value_name = "HEIGHT")]
    pub birthday: Option<u32>,

    #[arg(long)]
    pub fake: bool,

    #[arg(long)]
    pub ascii_qr: bool,

    #[arg(long, value_name = "FILE")]
    pub log_file: Option<PathBuf>,

    #[arg(
        long,
        value_name = "THEME",
        value_parser = clap::builder::PossibleValuesParser::new(palette::ids())
    )]
    pub theme: Option<String>,

    #[arg(long, default_value = "auto", value_parser = ["auto", "24bit", "256"])]
    pub color_depth: String,

    #[arg(long, default_value = "braille", value_parser = ["blocks", "braille", "off"])]
    pub dither: String,
}
