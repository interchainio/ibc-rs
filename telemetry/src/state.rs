use core::fmt;

use opentelemetry::{
    global,
    metrics::{Counter, UpDownCounter},
    KeyValue,
};
use opentelemetry_prometheus::PrometheusExporter;

use ibc::ics24_host::identifier::{ChainId, ChannelId, ClientId, PortId};
use prometheus::proto::MetricFamily;

#[derive(Copy, Clone, Debug)]
pub enum WorkerType {
    Client,
    Connection,
    Channel,
    Packet,
}

impl fmt::Display for WorkerType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Client => write!(f, "client"),
            Self::Connection => write!(f, "connection"),
            Self::Channel => write!(f, "channel"),
            Self::Packet => write!(f, "packet"),
        }
    }
}

#[derive(Debug)]
pub struct TelemetryState {
    exporter: PrometheusExporter,

    /// Number of workers per object
    workers: UpDownCounter<i64>,

    /// Number of client updates per client
    ibc_client_updates: Counter<u64>,

    /// Number of client misbehaviours per client
    ibc_client_misbehaviours: Counter<u64>,

    /// Number of receive packets relayed, per channel
    receive_packets: Counter<u64>,

    /// Number of acknowledgment packets relayed, per channel
    acknowledgment_packets: Counter<u64>,

    /// Number of timeout packets relayed, per channel
    timeout_packets: Counter<u64>,
}

impl TelemetryState {
    /// Gather the metrics for export
    pub fn gather(&self) -> Vec<MetricFamily> {
        self.exporter.registry().gather()
    }

    /// Update the number of workers per object
    pub fn worker(&self, worker_type: WorkerType, count: i64) {
        let labels = &[KeyValue::new("type", worker_type.to_string())];
        self.workers.add(count, labels);
    }

    /// Update the number of client updates per client
    pub fn ibc_client_update(&self, chain: &ChainId, client: &ClientId, count: u64) {
        let labels = &[
            KeyValue::new("chain", chain.to_string()),
            KeyValue::new("client", client.to_string()),
        ];

        self.ibc_client_updates.add(count, labels);
    }

    /// Number of client misbehaviours per client
    pub fn ibc_client_misbehaviour(&self, chain: &ChainId, client: &ClientId, count: u64) {
        let labels = &[
            KeyValue::new("chain", chain.to_string()),
            KeyValue::new("client", client.to_string()),
        ];

        self.ibc_client_misbehaviours.add(count, labels);
    }

    /// Number of receive packets relayed, per channel
    pub fn ibc_receive_packets(
        &self,
        src_chain: &ChainId,
        src_channel: &ChannelId,
        src_port: &PortId,
        count: u64,
    ) {
        let labels = &[
            KeyValue::new("src_chain", src_chain.to_string()),
            KeyValue::new("src_channel", src_channel.to_string()),
            KeyValue::new("src_port", src_port.to_string()),
        ];

        self.receive_packets.add(count, labels);
    }

    pub fn ibc_acknowledgment_packets(
        &self,
        src_chain: &ChainId,
        src_channel: &ChannelId,
        src_port: &PortId,
        count: u64,
    ) {
        let labels = &[
            KeyValue::new("src_chain", src_chain.to_string()),
            KeyValue::new("src_channel", src_channel.to_string()),
            KeyValue::new("src_port", src_port.to_string()),
        ];

        self.acknowledgment_packets.add(count, labels);
    }

    pub fn ibc_timeout_packets(
        &self,
        src_chain: &ChainId,
        src_channel: &ChannelId,
        src_port: &PortId,
        count: u64,
    ) {
        let labels = &[
            KeyValue::new("src_chain", src_chain.to_string()),
            KeyValue::new("src_channel", src_channel.to_string()),
            KeyValue::new("src_port", src_port.to_string()),
        ];

        self.timeout_packets.add(count, labels);
    }
}

impl Default for TelemetryState {
    fn default() -> Self {
        let exporter = opentelemetry_prometheus::exporter().init();
        let meter = global::meter("hermes");

        Self {
            exporter,

            workers: meter
                .i64_up_down_counter("workers")
                .with_description("Number of workers per object")
                .init(),

            ibc_client_updates: meter
                .u64_counter("ibc_client_updates")
                .with_description("Number of client updates performed per client")
                .init(),

            ibc_client_misbehaviours: meter
                .u64_counter("ibc_client_misbehaviours")
                .with_description("Number of misbehaviours detected per client")
                .init(),

            receive_packets: meter
                .u64_counter("ibc_receive_packets")
                .with_description("Number of receive packets relayed per channel")
                .init(),

            acknowledgment_packets: meter
                .u64_counter("ibc_acknowledgment_packets")
                .with_description("Number of acknowledgment packets relayed per channel")
                .init(),

            timeout_packets: meter
                .u64_counter("ibc_timeout_packets")
                .with_description("Number of timeout packets relayed per channel")
                .init(),
        }
    }
}
