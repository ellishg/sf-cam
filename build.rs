fn main() {
    assert!(
        std::path::Path::new("cfg.toml").exists(),
        "`cfg.toml` does not exit"
    );

    embuild::espidf::sysenv::output();
}
