//! Command-line entry point for netplay analytics tools.

use crate::analytics::config::AnalyticsConfig;
use crate::analytics::file_relay_query::{FileRelayAnalyticsDb, FileRelayEventFilter};
use crate::analytics::file_relay_report::{
    build_file_relay_report, print_file_relay_events, print_file_relay_report,
};
use crate::analytics::live::{LiveDiagnosticsClient, LiveDiagnosticsConfig};
use crate::analytics::query::{AnalyticsDb, SessionKey};
use crate::analytics::report::{build_fleet_report, build_session_reports, print_report};
use crate::config::PostgresTelemetryConfig;
use crate::observability::{
    NetplayPerformanceSample, NetplayTelemetryEvent, NetplayTelemetryRecord,
    PostgresTelemetryWriter,
};
use crate::rooms::RoomId;

pub async fn run_cli() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let command = args.first().map(String::as_str).unwrap_or("help");

    if matches!(command, "help" | "--help" | "-h") {
        print_help();
        return Ok(());
    }

    match command {
        "live" => {
            run_live(&args).await?;
        }
        "schema" => {
            let config = AnalyticsConfig::from_env()?;
            let db = AnalyticsDb::connect(config.clone()).await?;
            let file_relay_db = FileRelayAnalyticsDb::connect(config).await?;
            db.apply_schema().await?;
            file_relay_db.apply_schema().await?;
            println!("analytics schema applied");
        }
        "probe" => {
            let config = AnalyticsConfig::from_env()?;
            let db = AnalyticsDb::connect(config.clone()).await?;
            run_probe(config, &db).await?;
        }
        "purge-probes" => {
            let db = connect_db().await?;
            db.delete_probe_rows().await?;
            println!("analytics probe rows removed");
        }
        "sessions" => {
            let db = connect_db().await?;
            let limit = option_usize(&args, "--limit").unwrap_or(25);
            for session in db.recent_sessions(limit).await? {
                println!(
                    "{} epoch={} invite={} duration={}s",
                    session.room_id,
                    session.session_epoch,
                    session.invite_code,
                    session.ended_ms.saturating_sub(session.started_ms) / 1000
                );
            }
        }
        "report" => {
            let db = connect_db().await?;
            let limit = option_usize(&args, "--limit").unwrap_or(25);
            let sessions = db.recent_sessions(limit).await?;
            let events = db.events_for_sessions(&sessions).await?;
            let samples = db.samples_for_sessions(&sessions).await?;
            let session_reports = build_session_reports(&sessions, &events, &samples);
            let fleet_report = build_fleet_report(&session_reports);

            print_report(&fleet_report, &session_reports);
        }
        "raw" => {
            let db = connect_db().await?;
            run_raw(&db, &args).await?;
        }
        "file-relay" => {
            let config = AnalyticsConfig::from_env()?;
            let db = FileRelayAnalyticsDb::connect(config).await?;
            run_file_relay(&db, &args).await?;
        }
        _ => {
            print_help();
        }
    }

    Ok(())
}

async fn run_file_relay(
    db: &FileRelayAnalyticsDb,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let limit = option_usize(args, "--limit").unwrap_or(100);
    let filter = FileRelayEventFilter {
        room_id: option_string(args, "--room"),
        transfer_id: option_string(args, "--transfer"),
    };
    let events = db.recent_events(filter, limit).await?;

    match args.get(1).map(String::as_str) {
        Some("raw") => print_file_relay_events(&events),
        Some("report") => {
            let report = build_file_relay_report(&events);
            print_file_relay_report(&report, &events);
        }
        _ => print_help(),
    }

    Ok(())
}

async fn connect_db() -> Result<AnalyticsDb, Box<dyn std::error::Error>> {
    let config = AnalyticsConfig::from_env()?;
    Ok(AnalyticsDb::connect(config).await?)
}

async fn run_live(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let config = LiveDiagnosticsConfig::from_env()?;
    let client = LiveDiagnosticsClient::new(config)?;
    let payload = match args.get(1).map(String::as_str) {
        Some("metrics") => client.metrics().await?,
        Some("rooms") => client.rooms().await?,
        Some("room") => {
            let invite_code = required_option(args, "--invite")?;
            client.room(&invite_code).await?
        }
        Some("events") => {
            let limit = option_usize(args, "--limit").unwrap_or(100);
            if let Some(invite_code) = option_string(args, "--invite") {
                client.room_events(&invite_code, limit).await?
            } else {
                client.recent_events(limit).await?
            }
        }
        _ => {
            print_help();
            return Ok(());
        }
    };

    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}

async fn run_raw(db: &AnalyticsDb, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    match args.get(1).map(String::as_str) {
        Some("recent") => {
            let limit = option_usize(args, "--limit").unwrap_or(5);
            let sessions = db.recent_sessions(limit).await?;
            print_raw_sessions(db, &sessions).await?;
        }
        Some("session") => {
            let Some(room_id) = option_string(args, "--room") else {
                return Err("--room is required for raw session".into());
            };
            let sessions = if let Some(session_epoch) = option_usize(args, "--epoch") {
                vec![SessionKey {
                    room_id,
                    session_epoch: session_epoch as u64,
                    invite_code: String::new(),
                    started_ms: 0,
                    ended_ms: 0,
                }]
            } else {
                let limit = option_usize(args, "--limit").unwrap_or(25);
                db.sessions_for_room(&room_id, limit).await?
            };
            print_raw_sessions(db, &sessions).await?;
        }
        _ => print_help(),
    }

    Ok(())
}

async fn print_raw_sessions(
    db: &AnalyticsDb,
    sessions: &[SessionKey],
) -> Result<(), Box<dyn std::error::Error>> {
    let events = db.events_for_sessions(sessions).await?;
    let samples = db.samples_for_sessions(sessions).await?;

    println!("events:");
    for event in events {
        println!(
            "{} {} epoch={} seq={} {} {}",
            event.timestamp_ms,
            event.room_id,
            event.session_epoch,
            event.event_seq,
            event.kind,
            event.detail
        );
    }

    println!("samples:");
    for sample in samples {
        println!(
            "{} {} epoch={} p{} state={} local={:?} canonical={} released={:?} next_release={:?} accepted={:?} delta={:?} rtt={:?} jitter={:?} predict={:?} stalls={:?} catchup={:?} late={:?} audio={:?}",
            sample.timestamp_ms,
            sample.room_id,
            sample.session_epoch,
            sample.player_index + 1,
            sample.runtime_state,
            sample.local_frame,
            sample.canonical_frame,
            sample.released_frame,
            sample.next_release_frame,
            sample.accepted_input_frame,
            sample.frame_delta,
            sample.round_trip_ms,
            sample.jitter_ms,
            sample.prediction_frames,
            sample.stall_count,
            sample.catch_up_frames,
            sample.late_input_frames,
            sample.audio_underruns
        );
    }

    Ok(())
}

fn option_usize(args: &[String], name: &str) -> Option<usize> {
    option_string(args, name).and_then(|value| value.parse().ok())
}

fn option_string(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == name)
        .map(|window| window[1].clone())
}

fn required_option(
    args: &[String],
    name: &'static str,
) -> Result<String, Box<dyn std::error::Error>> {
    option_string(args, name).ok_or_else(|| format!("{name} is required").into())
}

fn print_help() {
    println!("ShadowBoy netplay analytics");
    println!("  live metrics");
    println!("  live rooms");
    println!("  live room --invite AB23-CD");
    println!("  live events [--invite AB23-CD] [--limit 100]");
    println!("  schema");
    println!("  probe");
    println!("  purge-probes");
    println!("  sessions --limit 25");
    println!("  report --limit 25");
    println!("  raw recent --limit 5");
    println!("  raw session --room <room_uuid> [--epoch <session_epoch>] [--limit 25]");
    println!(
        "  file-relay report [--room <room_or_lobby_id>] [--transfer <transfer_id>] [--limit 100]"
    );
    println!(
        "  file-relay raw [--room <room_or_lobby_id>] [--transfer <transfer_id>] [--limit 100]"
    );
}

async fn run_probe(
    config: AnalyticsConfig,
    db: &AnalyticsDb,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = PostgresTelemetryWriter::new(PostgresTelemetryConfig {
        dsn: config.dsn,
        tables: config.tables,
    });
    let now = timestamp_ms();
    let room_id = RoomId::new();
    let batch = vec![
        NetplayTelemetryRecord::RoomEvent(NetplayTelemetryEvent {
            timestamp_ms: now,
            room_id,
            invite_code: "PROB-E1".to_string(),
            event_seq: 1,
            room_epoch: 1,
            session_epoch: now,
            kind: "telemetryProbe".to_string(),
            detail: "operator telemetry write probe".to_string(),
        }),
        NetplayTelemetryRecord::PerformanceSample(NetplayPerformanceSample {
            timestamp_ms: now,
            room_id,
            invite_code: "PROB-E1".to_string(),
            event_seq: 2,
            room_epoch: 1,
            session_epoch: now,
            player_index: 0,
            runtime_state: "playing".to_string(),
            local_frame: Some(60),
            canonical_frame: 60,
            released_frame: Some(60),
            next_release_frame: 61,
            accepted_input_frame: Some(60),
            frame_delta: Some(0),
            round_trip_ms: Some(1),
            jitter_ms: Some(0),
            prediction_frames: Some(0),
            stall_count: Some(0),
            catch_up_frames: Some(0),
            late_input_frames: Some(0),
            audio_underruns: Some(0),
        }),
    ];

    writer.write_batch(&batch).await?;
    db.delete_probe_rows().await?;
    println!("analytics probe write succeeded room_id={room_id} session_epoch={now}");

    Ok(())
}

fn timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::ZERO)
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
