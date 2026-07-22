//! Verifies the production Compose-to-file-relay configuration contract.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const COOLIFY_COMPOSE: &str = include_str!("../docker-compose.coolify.yml");
const COOLIFY_ENV_EXAMPLE: &str = include_str!("../coolify.env.example");
const FILE_RELAY_CONFIG_SOURCE: &str = include_str!("../src/file_relay/config.rs");
const COMPOSE_RENDER_TIMEOUT: Duration = Duration::from_secs(15);

#[test]
fn coolify_deployment_passes_through_every_file_relay_configuration_variable() {
    let configured_names = file_relay_configuration_names();

    assert!(!configured_names.is_empty(), "file-relay config names");
    for name in configured_names {
        let compose_mapping = format!("- {name}");
        assert!(
            COOLIFY_COMPOSE
                .lines()
                .any(|line| line.trim() == compose_mapping),
            "docker-compose.coolify.yml must pass {name} through without converting omission to a blank value"
        );
        assert!(
            COOLIFY_ENV_EXAMPLE
                .lines()
                .any(|line| line.starts_with(&format!("{name}="))),
            "coolify.env.example must document {name}"
        );
    }
}

#[test]
#[ignore = "requires the Docker CLI with the Compose plugin; run the documented Coolify deployment contract command"]
fn docker_compose_render_preserves_optional_file_relay_environment() {
    let omitted_environment = render_file_relay_environment(&BTreeMap::new());
    assert!(omitted_environment.is_empty());

    let minimum_environment = BTreeMap::from([
        (
            "SB_NETPLAY_FILE_RELAY_URL".to_string(),
            documented_value("SB_NETPLAY_FILE_RELAY_URL"),
        ),
        (
            "SB_NETPLAY_FILE_RELAY_TOKEN".to_string(),
            documented_value("SB_NETPLAY_FILE_RELAY_TOKEN"),
        ),
    ]);
    assert_eq!(
        render_file_relay_environment(&minimum_environment),
        minimum_environment
    );

    let one_sided_environment = BTreeMap::from([(
        "SB_NETPLAY_FILE_RELAY_URL".to_string(),
        documented_value("SB_NETPLAY_FILE_RELAY_URL"),
    )]);
    assert_eq!(
        render_file_relay_environment(&one_sided_environment),
        one_sided_environment
    );
}

fn file_relay_configuration_names() -> BTreeSet<&'static str> {
    FILE_RELAY_CONFIG_SOURCE
        .split('"')
        .filter(|value| {
            value.starts_with("SB_NETPLAY_FILE_RELAY_")
                || value.starts_with("SB_NETPLAY_ROM_RELAY_")
                || value.starts_with("SB_NETPLAY_DIRECT_ROM_RELAY_")
        })
        .collect()
}

fn documented_value(name: &str) -> String {
    COOLIFY_ENV_EXAMPLE
        .lines()
        .find_map(|line| line.split_once('=').filter(|(key, _)| *key == name))
        .map(|(_, value)| value.to_string())
        .unwrap_or_else(|| panic!("coolify.env.example must document {name}"))
}

fn render_file_relay_environment(
    source_environment: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let output = render_compose(source_environment);
    assert!(
        output.status.success(),
        "docker compose config failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let rendered: Value = serde_json::from_slice(&output.stdout).expect("rendered Compose JSON");
    let environment = rendered["services"]["netplay"]["environment"]
        .as_object()
        .expect("rendered netplay environment");

    file_relay_configuration_names()
        .into_iter()
        .filter_map(|name| {
            environment.get(name).and_then(|value| {
                if value.is_null() {
                    None
                } else {
                    Some((
                        name.to_string(),
                        value
                            .as_str()
                            .unwrap_or_else(|| panic!("rendered {name} must be a string"))
                            .to_string(),
                    ))
                }
            })
        })
        .collect()
}

fn render_compose(source_environment: &BTreeMap<String, String>) -> Output {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut command = Command::new("docker");
    command
        .current_dir(manifest_dir)
        .args([
            "compose",
            "--env-file",
            "tests/fixtures/empty-compose.env.fixture",
            "--project-directory",
            manifest_dir.to_str().expect("UTF-8 manifest path"),
            "-f",
            "docker-compose.coolify.yml",
            "config",
            "--format",
            "json",
        ])
        .env_clear();
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    command.envs(source_environment);
    output_with_timeout(&mut command, COMPOSE_RENDER_TIMEOUT)
}

fn output_with_timeout(command: &mut Command, timeout: Duration) -> Output {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Docker Compose must be available");
    let started_at = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().expect("Docker Compose output"),
            Ok(None) if started_at.elapsed() < timeout => {
                thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.kill();
                let output = child
                    .wait_with_output()
                    .expect("timed-out Docker Compose output");
                panic!(
                    "docker compose config exceeded {timeout:?}:\nstdout:\n{}\nstderr:\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("failed while waiting for Docker Compose: {error}");
            }
        }
    }
}
