use core::time::Duration;
use crossbeam_channel::Receiver;
use ibc::Height;
use std::sync::{Arc, Mutex};
use tracing::{error, trace};

use crate::chain::handle::ChainHandle;
use crate::foreign_client::HasExpiredOrFrozenError;
use crate::link::{error::LinkError, Link, RelaySummary};
use crate::object::Packet;
use crate::telemetry;
use crate::util::retry::{retry_with_index, RetryResult};
use crate::util::task::{spawn_background_task, Next, TaskError, TaskHandle};
use crate::worker::retry_strategy;

use super::error::RunError;
use super::WorkerCmd;

/// Whether or not to clear pending packets at this `step` for the given height.
/// Packets are cleared on the first iteration if `clear_on_start` is true.
/// Subsequently, packets are cleared only if `clear_interval` is not `0` and
/// if we have reached the interval.
fn should_clear_packets(
    is_first_run: &mut bool,
    clear_on_start: bool,
    clear_interval: u64,
    height: Height,
) -> bool {
    if *is_first_run {
        *is_first_run = false;
        clear_on_start
    } else {
        clear_interval != 0 && height.revision_height % clear_interval == 0
    }
}

fn handle_link_error_in_task(e: LinkError) -> TaskError<RunError> {
    if e.is_expired_or_frozen_error() {
        TaskError::Fatal(RunError::link(e))
    } else {
        TaskError::Ignore(RunError::link(e))
    }
}

pub fn spawn_packet_worker<ChainA: ChainHandle, ChainB: ChainHandle>(
    path: Packet,
    // Mutex is used to prevent race condition between the packet workers
    link: Arc<Mutex<Link<ChainA, ChainB>>>,
) -> TaskHandle {
    spawn_background_task(
        format!("PacketWorker({})", link.lock().unwrap().a_to_b),
        Some(Duration::from_millis(1000)),
        move || {
            let relay_path = &link.lock().unwrap().a_to_b;

            relay_path
                .refresh_schedule()
                .map_err(handle_link_error_in_task)?;

            relay_path
                .execute_schedule()
                .map_err(handle_link_error_in_task)?;

            let summary = relay_path.process_pending_txs();

            if !summary.is_empty() {
                trace!("Packet worker produced relay summary: {:?}", summary);
            }

            telemetry!(packet_metrics(&path, &summary));

            Ok(Next::Continue)
        },
    )
}

pub fn spawn_packet_cmd_worker<ChainA: ChainHandle, ChainB: ChainHandle>(
    cmd_rx: Receiver<WorkerCmd>,
    // Mutex is used to prevent race condition between the packet workers
    link: Arc<Mutex<Link<ChainA, ChainB>>>,
    clear_on_start: bool,
    clear_interval: u64,
    path: Packet,
) -> TaskHandle {
    let mut is_first_run: bool = true;
    spawn_background_task(
        format!("PacketCmdWorker({})", link.lock().unwrap().a_to_b),
        Some(Duration::from_millis(200)),
        move || {
            if let Ok(cmd) = cmd_rx.try_recv() {
                retry_with_index(retry_strategy::worker_stubborn_strategy(), |index| {
                    handle_packet_cmd(
                        &mut is_first_run,
                        &link.lock().unwrap(),
                        clear_on_start,
                        clear_interval,
                        &path,
                        cmd.clone(),
                        index,
                    )
                })
                .map_err(|e| TaskError::Fatal(RunError::retry(e)))?;
            }

            Ok(Next::Continue)
        },
    )
}

/// Receives worker commands, which may be:
///     - IbcEvent => then it updates schedule
///     - NewBlock => schedules packet clearing
///     - Shutdown => exits
///
/// Regardless of the incoming command, this method
/// also refreshes and executes any scheduled operational
/// data that is ready.
fn handle_packet_cmd<ChainA: ChainHandle, ChainB: ChainHandle>(
    is_first_run: &mut bool,
    link: &Link<ChainA, ChainB>,
    clear_on_start: bool,
    clear_interval: u64,
    path: &Packet,
    cmd: WorkerCmd,
    index: u64,
) -> RetryResult<(), u64> {
    let result = match cmd {
        WorkerCmd::IbcEvents { batch } => link.a_to_b.update_schedule(batch),

        // Handle the arrival of an event signaling that the
        // source chain has advanced to a new block.
        WorkerCmd::NewBlock {
            height,
            new_block: _,
        } => {
            let do_clear_packet =
                should_clear_packets(is_first_run, clear_on_start, clear_interval, height);

            // Schedule the clearing of pending packets. This may happen once at start,
            // and may be _forced_ at predefined block intervals.
            link.a_to_b
                .schedule_packet_clearing(Some(height), do_clear_packet)
        }

        WorkerCmd::ClearPendingPackets => link.a_to_b.schedule_packet_clearing(None, true),
    };

    if let Err(e) = result {
        error!(
            path = %path.short_name(),
            "[{}] worker: handling command encountered error: {}",
            link.a_to_b, e
        );

        return RetryResult::Retry(index);
    }

    // The calls to refresh_schedule and execute_schedule depends on
    // earlier calls to update_schedule and schedule_packet_clearing.
    // Hence they must be retried in the same function body so that
    // the same WorkerCmd is used for retrying the whole execution.
    //
    // The worker for spawn_packet_worker is still needed to handle
    // the case when no PacketCmd arriving, so that it can still
    // do the refresh and execute schedule.
    // This follows the original logic here:
    // https://github.com/informalsystems/ibc-rs/blob/e7a6403888f48754ddb80e35ebe2281fb7c51c04/relayer/src/worker/packet.rs#L127-L133

    let schedule_result = link
        .a_to_b
        .refresh_schedule()
        .and_then(|_| link.a_to_b.execute_schedule());

    if let Err(e) = schedule_result {
        if e.is_expired_or_frozen_error() {
            error!(
                "[{}] worker aborting due to expired or frozen client",
                link.a_to_b
            );
            return RetryResult::Err(index);
        } else {
            error!(
                "[{}] worker: schedule execution encountered error: {}",
                link.a_to_b, e
            );
            return RetryResult::Retry(index);
        }
    }

    let summary = link.a_to_b.process_pending_txs();

    if !summary.is_empty() {
        trace!("Packet worker produced relay summary: {:?}", summary);
    }

    telemetry!(packet_metrics(path, &summary));

    RetryResult::Ok(())
}

#[cfg(feature = "telemetry")]
fn packet_metrics(path: &Packet, summary: &RelaySummary) {
    receive_packet_metrics(path, summary);
    acknowledgment_metrics(path, summary);
    timeout_metrics(path, summary);
}

#[cfg(feature = "telemetry")]
fn receive_packet_metrics(path: &Packet, summary: &RelaySummary) {
    use ibc::events::IbcEvent::WriteAcknowledgement;

    let count = summary
        .events
        .iter()
        .filter(|e| matches!(e, WriteAcknowledgement(_)))
        .count();

    telemetry!(
        ibc_receive_packets,
        &path.src_chain_id,
        &path.src_channel_id,
        &path.src_port_id,
        count as u64,
    );
}

#[cfg(feature = "telemetry")]
fn acknowledgment_metrics(path: &Packet, summary: &RelaySummary) {
    use ibc::events::IbcEvent::AcknowledgePacket;

    let count = summary
        .events
        .iter()
        .filter(|e| matches!(e, AcknowledgePacket(_)))
        .count();

    telemetry!(
        ibc_acknowledgment_packets,
        &path.src_chain_id,
        &path.src_channel_id,
        &path.src_port_id,
        count as u64,
    );
}

#[cfg(feature = "telemetry")]
fn timeout_metrics(path: &Packet, summary: &RelaySummary) {
    use ibc::events::IbcEvent::TimeoutPacket;
    let count = summary
        .events
        .iter()
        .filter(|e| matches!(e, TimeoutPacket(_)))
        .count();

    telemetry!(
        ibc_timeout_packets,
        &path.src_chain_id,
        &path.src_channel_id,
        &path.src_port_id,
        count as u64,
    );
}
