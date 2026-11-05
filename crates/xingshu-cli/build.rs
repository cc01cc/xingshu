use std::ffi::OsString;

const BUILD_CHANNEL_ENV: &str = "XINGSHU_CLI_BUILD_CHANNEL";

fn main() {
    println!("cargo:rerun-if-env-changed={BUILD_CHANNEL_ENV}");

    let channel = match std::env::var_os(BUILD_CHANNEL_ENV) {
        None => "source",
        Some(value) => normalize_channel(value),
    };

    println!("cargo:rustc-env={BUILD_CHANNEL_ENV}={channel}");
}

fn normalize_channel(value: OsString) -> &'static str {
    match value.into_string() {
        Ok(value) if value.is_empty() || value == "source" => "source",
        Ok(value) if value == "release" => "release",
        Ok(value) => panic!(
            "{BUILD_CHANNEL_ENV} must be unset, empty, `source`, or `release`; received {value:?}"
        ),
        Err(_) => panic!("{BUILD_CHANNEL_ENV} must be valid UTF-8"),
    }
}
