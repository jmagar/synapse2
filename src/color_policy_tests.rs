use super::{ColorChoice, install_color_choice, resolve};

#[test]
fn explicit_color_choice_overrides_terminal_detection() {
    install_color_choice(ColorChoice::Never);
    assert!(!resolve(true));

    install_color_choice(ColorChoice::Always);
    assert!(resolve(false));

    install_color_choice(ColorChoice::Auto);
    assert!(!resolve(false));
}
