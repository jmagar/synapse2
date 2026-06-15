use anyhow::{Result, bail};

use crate::color_policy::{self, ColorChoice};

pub(crate) const PRIMARY_ANSI: &str = "\x1b[38;2;230;244;251m";
pub(crate) const CYAN_ANSI: &str = "\x1b[38;2;41;182;246m";

pub(crate) fn color_enabled() -> bool {
    color_policy::enabled_stdout()
}

pub(crate) fn color_enabled_stderr() -> bool {
    color_policy::enabled_stderr()
}

pub(crate) fn install_color_from_args(args: &mut Vec<String>) -> Result<()> {
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg == "--" {
            break;
        }
        if arg == "--no-color" {
            color_policy::install_color_choice(ColorChoice::Never);
            args.remove(index);
            continue;
        }
        if let Some(value) = arg.strip_prefix("--color=") {
            color_policy::install_color_choice(parse_color_value(value)?);
            args.remove(index);
            continue;
        }
        if arg == "--color" {
            if let Some(next) = args.get(index + 1).map(String::as_str)
                && matches!(next, "always" | "never" | "auto")
            {
                color_policy::install_color_choice(parse_color_value(next)?);
                args.remove(index + 1);
                args.remove(index);
                continue;
            }
            color_policy::install_color_choice(ColorChoice::Always);
            args.remove(index);
            continue;
        }
        index += 1;
    }
    Ok(())
}

fn parse_color_value(value: &str) -> Result<ColorChoice> {
    match value {
        "always" => Ok(ColorChoice::Always),
        "never" => Ok(ColorChoice::Never),
        "auto" => Ok(ColorChoice::Auto),
        other => bail!("--color expects always|never|auto, got `{other}`"),
    }
}
