//! File-relay analytics reports.

use std::collections::{BTreeMap, BTreeSet};

use crate::analytics::file_relay_query::FileRelayEventRow;

/// Aggregate transfer report.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FileRelayReport {
    pub event_count: usize,
    pub transfer_count: usize,
    pub room_count: usize,
    pub rom_transfers: usize,
    pub save_state_transfers: usize,
    pub failed_events: usize,
    pub cancelled_events: usize,
    pub expired_events: usize,
    pub completed_transfers: usize,
    pub total_size_bytes: u64,
}

/// Builds a high-signal report from recent transfer events.
pub fn build_file_relay_report(events: &[FileRelayEventRow]) -> FileRelayReport {
    let mut transfer_ids = BTreeSet::new();
    let mut room_ids = BTreeSet::new();
    let mut final_status_by_transfer = BTreeMap::<String, String>::new();
    let mut size_by_transfer = BTreeMap::<String, u64>::new();
    let mut report = FileRelayReport {
        event_count: events.len(),
        ..FileRelayReport::default()
    };

    for event in events {
        transfer_ids.insert(event.transfer_id.clone());
        room_ids.insert(event.room_id.clone());
        final_status_by_transfer.insert(event.transfer_id.clone(), event.status.clone());
        size_by_transfer
            .entry(event.transfer_id.clone())
            .or_insert(event.size_bytes);

        match event.kind.as_str() {
            "rom" => report.rom_transfers += usize::from(event.phase == "created"),
            "save-state" => report.save_state_transfers += usize::from(event.phase == "created"),
            _ => {}
        }
        report.failed_events += usize::from(event.status == "failed");
        report.cancelled_events += usize::from(event.status == "cancelled");
        report.expired_events += usize::from(event.status == "expired");
    }

    report.transfer_count = transfer_ids.len();
    report.room_count = room_ids.len();
    report.completed_transfers = final_status_by_transfer
        .values()
        .filter(|status| status.as_str() == "complete")
        .count();
    report.total_size_bytes = size_by_transfer.values().sum();
    report
}

/// Prints the file-relay summary and recent event table.
pub fn print_file_relay_report(report: &FileRelayReport, events: &[FileRelayEventRow]) {
    println!("File relay analytics report");
    println!("events: {}", report.event_count);
    println!(
        "transfers: {} across {} rooms",
        report.transfer_count, report.room_count
    );
    println!(
        "created: {} ROM | {} save-state",
        report.rom_transfers, report.save_state_transfers
    );
    println!(
        "complete: {} | failed events: {} | cancelled: {} | expired: {}",
        report.completed_transfers,
        report.failed_events,
        report.cancelled_events,
        report.expired_events
    );
    println!("unique transfer bytes: {}", report.total_size_bytes);
    println!();
    print_file_relay_events(events);
}

/// Prints recent file-relay events.
pub fn print_file_relay_events(events: &[FileRelayEventRow]) {
    println!(
        "{:<13} {:<34} {:<12} {:<12} {:<12} {:>10} {:>10} {:>10} detail",
        "timestamp", "transfer", "kind", "phase", "status", "size", "up", "down"
    );

    for event in events {
        println!(
            "{:<13} {:<34} {:<12} {:<12} {:<12} {:>10} {:>10} {:>10} {}",
            event.timestamp_ms,
            event.transfer_id,
            event.kind,
            event.phase,
            event.status,
            event.size_bytes,
            event.uploaded_bytes,
            event.downloaded_bytes,
            event.detail
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_counts_unique_transfers() {
        let events = vec![
            event("tx_1", "room-1", "rom", "created", "created", 10),
            event(
                "tx_1",
                "room-1",
                "rom",
                "downloadChunkServed",
                "complete",
                10,
            ),
            event("tx_2", "room-1", "save-state", "created", "created", 20),
        ];

        let report = build_file_relay_report(&events);

        assert_eq!(report.transfer_count, 2);
        assert_eq!(report.room_count, 1);
        assert_eq!(report.rom_transfers, 1);
        assert_eq!(report.save_state_transfers, 1);
        assert_eq!(report.completed_transfers, 1);
        assert_eq!(report.total_size_bytes, 30);
    }

    fn event(
        transfer_id: &str,
        room_id: &str,
        kind: &str,
        phase: &str,
        status: &str,
        size_bytes: u64,
    ) -> FileRelayEventRow {
        FileRelayEventRow {
            timestamp_ms: 0,
            transfer_id: transfer_id.to_string(),
            room_id: room_id.to_string(),
            kind: kind.to_string(),
            phase: phase.to_string(),
            status: status.to_string(),
            size_bytes,
            uploaded_bytes: 0,
            downloaded_bytes: 0,
            chunk_index: None,
            chunk_count: 1,
            detail: String::new(),
        }
    }
}
